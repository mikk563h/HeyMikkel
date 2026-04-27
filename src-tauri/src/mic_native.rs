//! Optagelse via Core Audio / cpal — **ikke** WKWebView getUserMedia.
//! På macOS knytter TCC tilladelsen til Hey Mikkel-processen, så appen vises under Systemindstillinger → Mikrofon.

use std::collections::HashSet;
use std::io::Cursor;
use std::sync::mpsc::Receiver;
use std::sync::{Mutex, OnceLock};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, InputCallbackInfo, SampleFormat, Stream, SupportedStreamConfig};
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

/// Saml input-enheder: først systemets “standard” input, derefter resten (uden dubletter efter navn).
fn enumerate_input_devices(host: &cpal::Host) -> Result<Vec<Device>, String> {
    let mut out: Vec<Device> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if let Some(d) = host.default_input_device() {
        if let Ok(n) = d.name() {
            seen.insert(n);
        }
        out.push(d);
    }
    for d in host
        .input_devices()
        .map_err(|e| format!("Kunne ikke liste lydkort/input: {e}"))?
    {
        let key = d.name().unwrap_or_default();
        if key.is_empty() {
            out.push(d);
            continue;
        }
        if seen.insert(key) {
            out.push(d);
        }
    }
    Ok(out)
}

/// Prøv hver fysiske/virtuelle input i rækkefølge, indtil Core Audio svarer. Det løser ofte, at
/// “Standard”-enheden er i en defekt state, mens en anden (fx indbygget) virker.
fn pick_device_and_config() -> Result<(Device, SupportedStreamConfig), String> {
    let host = cpal::default_host();
    let devices = enumerate_input_devices(&host)?;
    if devices.is_empty() {
        return Err(
            "Fandt ingen mikrofon. Vælg et input under Systemindstillinger → Lyd → Input.".into(),
        );
    }
    let mut last_err: Option<String> = None;
    for d in devices {
        let label = d.name().unwrap_or_else(|_| "ukendt enhed".to_string());
        eprintln!("[Hey Mikkel] Prøver input: {label}");
        match pick_input_config(&d) {
            Ok(cfg) => {
                eprintln!("[Hey Mikkel] Bruger input: {label}");
                return Ok((d, cfg));
            }
            Err(e) => {
                eprintln!("[Hey Mikkel] {label} — {e}");
                last_err = Some(e);
            }
        }
    }
    Err(mic_exhausted_message(last_err))
}

fn mic_exhausted_message(last: Option<String>) -> String {
    const TIP: &str = "Hey Mikkel har ofte allerede mikrofontilladelse. Fejlen sidder i **hvilket lyd-INPUT** macOS bruger. Gør sådan her:\n\
        1) Klik i Hey Mikkel på **«Åbn Lyd — vælg fanen Lyd ind»** — du skal se **Lyd ind**, ikke fanen *Lyd ud* / højtalere. Vælg f.eks. **Indbygget mikrofon** eller USB/headset.\n\
        2) Lige et headset eller slå en Bluetooth-mikro fra/til, hvis du bruger sådan en.\n\
        3) Luk andre der optager lyd (optager, møde-apps).\n\
        4) Genstart lydchippen (én gang i Terminal: **sudo killall coreaudiod**), og prøv Hey Mikkel igen.";

    if let Some(ref s) = last {
        if s.len() < 400 && !s.contains("An unknown error unknown") {
            return format!("{TIP}\n\n(Teknisk: {s})");
        }
    }
    TIP.to_string()
}

/// Prøv standardformat; ellers vælg et understøttet format (f.eks. hvis default fejler på bestemt hardware).
fn pick_input_config(device: &Device) -> Result<SupportedStreamConfig, String> {
    match device.default_input_config() {
        Ok(c) => Ok(c),
        Err(e_default) => {
            let ranges: Vec<cpal::SupportedStreamConfigRange> = match device.supported_input_configs() {
                Ok(i) => i.collect(),
                Err(e) => {
                    return Err(format!(
                        "Kunne ikke læse denne enhed (standard: {e_default}, formater: {e})."
                    ));
                }
            };
            if ranges.is_empty() {
                return Err(format!(
                    "Mikrofonen svarer ikke (standard: {e_default}, ingen andre formater). Prøv en anden input-enhed i macOS-lydindstillinger."
                ));
            }
            for fmt in [
                SampleFormat::F32,
                SampleFormat::I16,
                SampleFormat::I32,
                SampleFormat::F64,
                SampleFormat::I8,
            ] {
                if let Some(r) = ranges.iter().find(|r| r.sample_format() == fmt) {
                    return Ok(r.with_max_sample_rate());
                }
            }
            Ok(ranges[0].with_max_sample_rate())
        }
    }
}

/// **Vigtigt:** `device` skal være den *samme* enhed, som `supported` kommer fra — ellers fejler
/// `build_input_stream` ofte, selv når TCC-mikrofon er slået til.
fn run_capture(device: Device, stop_rx: Receiver<()>, supported: SupportedStreamConfig) -> Result<Vec<u8>, String> {
    let sc = supported.config();
    let sample_rate = supported.sample_rate().0;
    let ch = supported.channels();
    let buffer = std::sync::Arc::new(Mutex::new(Vec::<f32>::new()));
    let buf = buffer.clone();

    let err_fn = |e| eprintln!("[Hey Mikkel] cpal stream: {e}");

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
            return Err(format!("Mikrofon: lydformat {fmt:?} er ikke understøttet i MVP. Vælg et andet input i Lydindstillinger."));
        }
    };

    let stream = stream.map_err(|e| {
        format!("Kunne ikke åbne lydstrøm (mikrofon er tilladt i Systemindstillinger, men lydkort/ format fejlede: {e})")
    })?;
    stream.play().map_err(|e| format!("Kunne ikke starte lydstrøm: {e}"))?;

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
    let (device, supported) = pick_device_and_config()?;
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let sup = supported;
    let join = std::thread::spawn(move || run_capture(device, stop_rx, sup));
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
