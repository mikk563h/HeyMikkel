use std::io::Cursor;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use device_query::{DeviceQuery, DeviceState, Keycode};
use image::{DynamicImage, ImageOutputFormat};
use reqwest::multipart::{Form, Part};
use serde_json::{json, Value};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{
    AppHandle, Emitter, EventTarget, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow,
    WindowEvent,
};

mod mic_native;

/// macOS: skal være "almindelig" forgrundsapp (Regular) for at TCC/Mikrofon-listen i
/// Systemindstillinger opfatter processen korrekt. Accessory + LSUIElement gav ikke post på listen.
#[cfg(target_os = "macos")]
fn set_macos_activation_regular(app: &AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
}

#[cfg(not(target_os = "macos"))]
fn set_macos_activation_regular(_app: &AppHandle) {}

/// Kald fra frontend. Kører på main thread, med synligt vindue, og venter til både
/// AVFAudio (record permission, macOS 14+) **og** AVFoundation (capture) har svar, så TCC
/// typisk får både dialog og plads under Systemindstillinger → Mikrofon.
#[tauri::command]
fn request_macos_av_microphone(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::sync::mpsc;
        use std::time::Duration;

        use block2::RcBlock;
        use objc2::runtime::Bool;
        use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
        use objc2_avf_audio::AVAudioApplication;

        let (tx, rx) = mpsc::channel();
        app
            .run_on_main_thread(move || {
                // 1) AVFAudio: den dialog macOS 14+ forventer til “optag lyd” / cpal-vej.
                // 2) Derefter AVCapture + AVMediaTypeAudio: ældre TCC-klient + backup hvis
                //    AVMediaTypeAudio ellers aldrig blev kaldt (hvis nogen skulle mangle kæden).
                let tx_after_audio = tx.clone();
                let b_after_av_audio = RcBlock::new(move |_: Bool| {
                    unsafe {
                        if let Some(audio) = AVMediaTypeAudio {
                            let tx_cap = tx_after_audio.clone();
                            let b_capture = RcBlock::new(move |_: Bool| {
                                let _ = tx_cap.send(());
                            });
                            AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &b_capture);
                        } else {
                            eprintln!("[Hey Mikkel] AVMediaTypeAudio er null — tjek at AVFoundation linkes.");
                            let _ = tx_after_audio.send(());
                        }
                    }
                });
                unsafe {
                    AVAudioApplication::requestRecordPermissionWithCompletionHandler(&b_after_av_audio);
                }
            })
            .map_err(|e| e.to_string())?;

        rx.recv_timeout(Duration::from_secs(120))
            .map_err(|_| "Mikrofon: ingen svar fra systemet (ventede på bekræftelse). Prøv igen.".to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(())
    }
}

#[tauri::command]
async fn transcribe_audio(
    api_key: String,
    audio_base64: String,
    mime_type: String,
    language: Option<String>,
) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Err("OpenAI API key mangler.".into());
    }

    let audio_bytes = STANDARD
        .decode(audio_base64)
        .map_err(|_| "Kunne ikke læse lydoptagelsen.".to_string())?;

    if audio_bytes.len() < 1200 {
        return Err(
            "For lidt lyd i optagelsen. Hold Option nede lidt længere, tal tydeligt, og tjek mikrofon under Lyd → Lyd ind."
                .into(),
        );
    }

    let file_name = if mime_type.contains("wav") {
        "recording.wav"
    } else if mime_type.contains("mp4") || mime_type.contains("m4a") {
        "recording.m4a"
    } else {
        "recording.webm"
    };

    let audio_part = Part::bytes(audio_bytes)
        .file_name(file_name)
        .mime_str(&mime_type)
        .map_err(|error| error.to_string())?;

    let mut form = Form::new().text("model", "whisper-1").part("file", audio_part);

    if let Some(lang) = language {
        let trimmed = lang.trim();
        if !trimmed.is_empty() {
            form = form.text("language", trimmed.to_string());
        }
    }

    let response = reqwest::Client::new()
        .post("https://api.openai.com/v1/audio/transcriptions")
        .bearer_auth(api_key.trim())
        .multipart(form)
        .send()
        .await
        .map_err(|error| format!("Kunne ikke kontakte OpenAI: {error}"))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|error| format!("Kunne ikke læse OpenAI-svar: {error}"))?;

    if !status.is_success() {
        let mut msg = openai_error_message(&body);
        let lower = msg.to_lowercase();
        if lower.contains("corrupt") || lower.contains("unsupported") || lower.contains("invalid file") {
            msg = "OpenAI kunne ikke læse lyden (ofte for kort optagelse eller for lav lyd). Hold Option nede lidt længere og prøv igen."
                .into();
        }
        return Err(msg);
    }

    body.get("text")
        .and_then(Value::as_str)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "OpenAI returnerede ingen transskription.".to_string())
}

#[tauri::command]
async fn ask_ai(
    api_key: String,
    system_prompt: String,
    user_prompt: String,
    screenshot_base64: Option<String>,
) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Err("OpenAI API key mangler.".into());
    }

    let user_content = match screenshot_base64 {
        Some(image) if !image.trim().is_empty() => json!([
            {
                "type": "text",
                "text": user_prompt
            },
            {
                "type": "image_url",
                "image_url": {
                    "url": format!("data:image/png;base64,{image}")
                }
            }
        ]),
        _ => json!(user_prompt),
    };

    let payload = json!({
        "model": "gpt-4o-mini",
        "temperature": 0.7,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": user_content
            }
        ]
    });

    let response = reqwest::Client::new()
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key.trim())
        .json(&payload)
        .send()
        .await
        .map_err(|error| format!("Kunne ikke kontakte OpenAI: {error}"))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|error| format!("Kunne ikke læse OpenAI-svar: {error}"))?;

    if !status.is_success() {
        return Err(openai_error_message(&body));
    }

    body.pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "OpenAI returnerede intet svar.".to_string())
}

#[tauri::command]
fn capture_screen() -> Result<String, String> {
    let screens = screenshots::Screen::all().map_err(|error| format!("Kunne ikke finde skærme: {error}"))?;
    let screen = screens
        .first()
        .ok_or_else(|| "Ingen skærm fundet.".to_string())?;
    let image = screen
        .capture()
        .map_err(|error| format!("Kunne ikke tage screenshot: {error}"))?;

    let mut png_bytes = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut png_bytes), ImageOutputFormat::Png)
        .map_err(|error| format!("Kunne ikke gemme screenshot: {error}"))?;

    Ok(STANDARD.encode(png_bytes))
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| format!("Clipboard er ikke tilgængeligt: {error}"))?;
    clipboard
        .set_text(text)
        .map_err(|error| format!("Kunne ikke kopiere tekst: {error}"))
}

#[tauri::command]
fn insert_text(text: String) -> Result<(), String> {
    copy_text(text)?;

    #[cfg(target_os = "macos")]
    {
        Command::new("osascript")
            .args([
                "-e",
                r#"tell application "System Events" to keystroke "v" using command down"#,
            ])
            .output()
            .map_err(|error| format!("Kunne ikke indsætte tekst: {error}"))?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        return Err("Direkte indsætning er kun implementeret til macOS i MVP'en.".into());
    }

    Ok(())
}

#[tauri::command]
fn set_overlay_interactive(app: AppHandle, interactive: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("overlay") {
        window
            .set_ignore_cursor_events(!interactive)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn show_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|error| error.to_string())?;
        let _ = window.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn hide_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Åbner macOS systempanelet for mikrofontilladelse.
#[tauri::command]
fn open_microphone_privacy() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        for url in [
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Microphone",
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        ] {
            if std::process::Command::new("open")
                .arg(url)
                .status()
                .map(|c| c.success())
                .unwrap_or(false)
            {
                return Ok(());
            }
        }
    }
    Ok(())
}

/// Fanen **Lyd ind** (hvilket kort/mikrofon der bruges) — ikke det samme som Privatliv → Mikrofon.
#[tauri::command]
fn open_sound_input_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        for url in [
            "x-apple.systempreferences:com.apple.Sound-Settings.extension?input",
            "x-apple.systempreferences:com.apple.Sound-Settings.extension",
        ] {
            if std::process::Command::new("open")
                .arg(url)
                .status()
                .map(|c| c.success())
                .unwrap_or(false)
            {
                return Ok(());
            }
        }
        return Err("Kunne ikke åbne Lyd. Gå manuelt til Systemindstillinger → Lyd → Lyd ind.".to_string());
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

/// Ofte nødvendigt hvis Skærmtid/Indhold låser forældrekontrol for mikrofon (grå toggle).
#[tauri::command]
fn open_screentime_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        for url in [
            "x-apple.systempreferences:com.apple.preferences.screentime",
            "x-apple.systempreferences:com.apple.preference.screentime",
        ] {
            let s = std::process::Command::new("open")
                .arg(url)
                .status();
            if s.map(|c| c.success()).unwrap_or(false) {
                return Ok(());
            }
        }
        return Err("Kunne ikke åbne Skærmtid — åbn manuelt under Systemindstillinger.".to_string());
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        for url in [
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
        ] {
            if std::process::Command::new("open")
                .arg(url)
                .status()
                .map(|c| c.success())
                .unwrap_or(false)
            {
                return Ok(());
            }
        }
        return Err("Kunne ikke åbne Tilgængelighed i Systemindstillinger.".to_string());
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

#[tauri::command]
fn request_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos_accessibility_client::accessibility::application_is_trusted_with_prompt()
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

fn openai_error_message(body: &Value) -> String {
    body.pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("OpenAI returnerede en fejl.")
        .to_string()
}

fn is_option_keycode(key: &Keycode) -> bool {
    matches!(
        key,
        Keycode::LOption | Keycode::ROption | Keycode::LAlt | Keycode::RAlt
    )
}

/// Kun modifiers nede — ellers ignorerer vi PTT (fx Option+ bogstav).
fn is_modifier_only_keycode(key: &Keycode) -> bool {
    matches!(
        key,
        Keycode::LControl
            | Keycode::RControl
            | Keycode::LShift
            | Keycode::RShift
            | Keycode::LAlt
            | Keycode::RAlt
            | Keycode::LOption
            | Keycode::ROption
            | Keycode::Command
            | Keycode::RCommand
            | Keycode::LMeta
            | Keycode::RMeta
            | Keycode::CapsLock
    )
}

fn ptt_chord_ok(keys: &[Keycode]) -> bool {
    let option_down = keys.iter().any(is_option_keycode);
    option_down && keys.iter().all(|k| is_modifier_only_keycode(k))
}

/// Fuldskærm uden macOS "eksklusiv fuldskærm" (som giver sort canvas) — fylder primær skærm.
fn apply_overlay_to_primary<R: Runtime>(overlay: &WebviewWindow<R>) -> Result<(), String> {
    let mon = overlay
        .primary_monitor()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Ingen skærm fundet.".to_string())?;
    let pos = mon.position();
    let size = mon.size();
    overlay
        .set_fullscreen(false)
        .map_err(|e| e.to_string())?;
    overlay
        .set_position(PhysicalPosition::new(pos.x, pos.y))
        .map_err(|e| e.to_string())?;
    overlay
        .set_size(PhysicalSize::new(size.width, size.height))
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn start_command_hold_listener(app_handle: AppHandle) {
    // device_query på macOS kræver Tilgængelighed. Vent først — ellers er get_keys() tom.
    thread::spawn(move || {
        #[cfg(target_os = "macos")]
        {
            let _ = macos_accessibility_client::accessibility::application_is_trusted_with_prompt();
            let mut last_missing_accessibility_event = Instant::now() - Duration::from_secs(10);
            while !macos_accessibility_client::accessibility::application_is_trusted() {
                if last_missing_accessibility_event.elapsed() >= Duration::from_secs(8) {
                    let _ = app_handle.emit_to(
                        EventTarget::webview_window("overlay"),
                        "push-to-talk-permission-missing",
                        (),
                    );
                    let _ = app_handle.emit_to(
                        EventTarget::webview_window("main"),
                        "push-to-talk-permission-missing",
                        (),
                    );
                    last_missing_accessibility_event = Instant::now();
                }
                thread::sleep(Duration::from_millis(200));
            }
            thread::sleep(Duration::from_millis(400));
        }

        let device_state = DeviceState::new();
        let mut is_talking = false;
        let mut pending_since: Option<Instant> = None;

        loop {
            let keys = device_state.get_keys();
            let chord_ok = ptt_chord_ok(&keys);

            if chord_ok {
                if !is_talking {
                    let started_at = pending_since.get_or_insert_with(Instant::now);
                    if started_at.elapsed() >= Duration::from_millis(80) {
                        if let Some(overlay) = app_handle.get_webview_window("overlay") {
                            let _ = apply_overlay_to_primary(&overlay);
                            let _ = overlay.set_always_on_top(true);
                            let _ = overlay.show();
                            let _ = overlay.set_ignore_cursor_events(false);
                        }
                        let _ = app_handle.emit_to(
                            EventTarget::webview_window("overlay"),
                            "push-to-talk-start",
                            (),
                        );
                        is_talking = true;
                    }
                }
            } else if !is_talking {
                pending_since = None;
            }

            if !keys.iter().any(is_option_keycode) {
                pending_since = None;
                if is_talking {
                    let _ = app_handle.emit_to(
                        EventTarget::webview_window("overlay"),
                        "push-to-talk-stop",
                        (),
                    );
                    is_talking = false;
                }
            }

            thread::sleep(Duration::from_millis(30));
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            set_macos_activation_regular(&app.handle());

            if let Some(main) = app.get_webview_window("main") {
                let _ = main.hide();
            }

            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = apply_overlay_to_primary(&overlay);
                let _ = overlay.set_ignore_cursor_events(true);
                let _ = overlay.hide();
            }

            let settings_i = MenuItem::with_id(app, "settings", "Indstillinger…", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Afslut Hey Mikkel", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_i, &quit_i])?;

            let mut tray = TrayIconBuilder::new().menu(&menu);
            #[cfg(target_os = "macos")]
            {
                if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))
                {
                    tray = tray.icon(icon).icon_as_template(true);
                } else if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone());
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone());
                }
            }

            let _ = tray
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            start_command_hold_listener(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            transcribe_audio,
            ask_ai,
            capture_screen,
            copy_text,
            insert_text,
            request_accessibility_permission,
            set_overlay_interactive,
            show_settings_window,
            hide_main_window,
            open_microphone_privacy,
            open_sound_input_settings,
            open_screentime_settings,
            open_accessibility_settings,
            request_macos_av_microphone,
            mic_native::native_mic_start,
            mic_native::native_mic_stop
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
