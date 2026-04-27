//! Optagelse via Core Audio / cpal — **ikke** WKWebView getUserMedia.
//! På macOS knytter TCC tilladelsen til Hey Mikkel-processen, så appen vises under Systemindstillinger → Mikrofon.

use std::io::Cursor;
use std::sync::mpsc::Receiver;
use std::sync::{Mutex, OnceLock};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{InputCallbackInfo, SampleFormat, Stream, SupportedStreamConfig};
use hound::{SampleFormat as HoundSampleFormat, WavSpec, WavWriter};

struct Active {
    stop_tx: std::sync::mpsc::Sender<()>,
    join: std::thread::JoinHandle<Result<Vec<u8>, String>>,
}

fn state() -> &'static Mutex<Option<Active>> {
    static S: OnceLock<Mutex<Option<Active>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn f32_to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

fn interleaved_f32_to_wav(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<u8>, String> {
    if samples.is_empty() {
        return Err("Ingen lyddata.".into());
    }
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: HoundSampleFormat::Int,
    };
    let mut bytes = Vec::new();
    {
        let cursor = Cursor::new(&mut bytes);
        let mut w = WavWriter::new(cursor, spec).map_err(|e| e.to_string())?;
        for &s in samples {
            w.write_sample(f32_to_i16(s)).map_err(|e| e.to_string())?;
        }
        w.finalize().map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

fn push_f32(buf: &std::sync::Arc<Mutex<Vec<f32>>>, data: &[f32]) {
    if let Ok(mut g) = buf.lock() {
        g.extend_from_slice(data);
    }
}

fn run_capture(stop_rx: Receiver<()>, supported: SupportedStreamConfig) -> Result<Vec<u8>, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "Ingen input-enhed. Tillad evt. Mikrofon for Hey Mikkel (Systemindstillinger).".to_string())?;

    let sc = supported.config();
    let sample_rate = supported.sample_rate().0;
    let ch = supported.channels();
    let buffer = std::sync::Arc::new(Mutex::new(Vec::<f32>::new()));
    let buf = buffer.clone();

    let err_fn = |e| eprintln!("[Hey Mikkel] cpal: {e}");

    let stream: Result<Stream, cpal::BuildStreamError> = match supported.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &sc,
            move |data: &[f32], _i: &InputCallbackInfo| push_f32(&buf, data),
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &sc,
            {
                let buf = buffer.clone();
                move |data: &[i16], _i: &InputCallbackInfo| {
                    if let Ok(mut g) = buf.lock() {
                        g.extend(data.iter().map(|&s| s as f32 / f32::from(i16::MAX)));
                    }
                }
            },
            err_fn,
            None,
        ),
        SampleFormat::I32 => device.build_input_stream(
            &sc,
            {
                let buf = buffer.clone();
                move |data: &[i32], _i: &InputCallbackInfo| {
                    if let Ok(mut g) = buf.lock() {
                        g.extend(data.iter().map(|&s| s as f32 / i32::MAX as f32));
                    }
                }
            },
            err_fn,
            None,
        ),
        SampleFormat::I8 => device.build_input_stream(
            &sc,
            {
                let buf = buffer.clone();
                move |data: &[i8], _i: &InputCallbackInfo| {
                    if let Ok(mut g) = buf.lock() {
                        g.extend(data.iter().map(|&s| s as f32 / f32::from(i8::MAX)));
                    }
                }
            },
            err_fn,
            None,
        ),
        SampleFormat::F64 => device.build_input_stream(
            &sc,
            {
                let buf = buffer.clone();
                move |data: &[f64], _i: &InputCallbackInfo| {
                    if let Ok(mut g) = buf.lock() {
                        g.extend(data.iter().map(|&s| s as f32));
                    }
                }
            },
            err_fn,
            None,
        ),
        fmt => {
            return Err(format!("Mikrofon: lydformat {fmt:?} er ikke understøttet i MVP."));
        }
    };

    let stream = stream.map_err(|e| format!("Kunne ikke åbne mikrofon (TCC/ hardware): {e}"))?;
    stream.play().map_err(|e| e.to_string())?;

    let _ = stop_rx.recv();
    drop(stream);

    let raw = buffer.lock().map_err(|e| e.to_string())?.clone();
    interleaved_f32_to_wav(&raw, sample_rate, ch)
}

#[tauri::command]
pub fn native_mic_start() -> Result<(), String> {
    let mut g = state().lock().map_err(|e| e.to_string())?;
    if g.is_some() {
        return Err("Optagelse kører allerede.".into());
    }
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "Ingen input-enhed. Giv Hey Mikkel mikrofontilladelse (Systemindstillinger).".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Kunne ikke læse standard-mikrofon: {e}"))?;
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let sup = supported;
    let join = std::thread::spawn(move || run_capture(stop_rx, sup));
    *g = Some(Active { stop_tx, join });
    Ok(())
}

#[tauri::command]
pub fn native_mic_stop() -> Result<String, String> {
    let mut g = state().lock().map_err(|e| e.to_string())?;
    let active = g.take().ok_or_else(|| "Ingen aktiv optagelse.".to_string())?;
    let _ = active.stop_tx.send(());
    let wav = active
        .join
        .join()
        .map_err(|_| "Lydtråd stoppede uventet.".to_string())??;
    Ok(STANDARD.encode(wav))
}
