use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
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

    let (model, user_content) = match screenshot_base64 {
        Some(image) if !image.trim().is_empty() => (
            "gpt-4o",
            json!([
                { "type": "text", "text": user_prompt },
                {
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/png;base64,{image}"),
                        "detail": "high"
                    }
                }
            ]),
        ),
        _ => ("gpt-4o-mini", json!(user_prompt)),
    };

    let payload = json!({
        "model": model,
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
    // screencapture bruger ScreenCaptureKit internt og virker på macOS 14–16.
    // CGWindowListCreateImage (screenshots-crate) er fjernet i macOS 16 Tahoe.
    let tmp = std::env::temp_dir().join("heymikkel_cap.png");

    let output = std::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-m", &tmp.to_string_lossy()])
        .output()
        .map_err(|e| format!("SCREEN_PERMISSION: Kunne ikke starte screencapture: {e}"))?;

    if !output.status.success() {
        return Err("SCREEN_PERMISSION: Skærmoptagelse fejlede — slå Hey Mikkel til under Fortrolighed → Skærmoptagelse og genstart Hey Mikkel.".to_string());
    }

    let bytes = std::fs::read(&tmp)
        .map_err(|_| "SCREEN_PERMISSION: Screenshot-fil mangler — genstart Hey Mikkel og prøv igen.".to_string())?;
    let _ = std::fs::remove_file(&tmp);

    if bytes.len() < 1000 {
        return Err("SCREEN_PERMISSION: Screenshot er tomt — genstart Hey Mikkel efter at have givet tilladelse under Fortrolighed → Skærmoptagelse.".to_string());
    }

    Ok(STANDARD.encode(bytes))
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

/// Skjuler overlay FØRST (så forrige app får fokus igen), venter 180 ms, indsætter derefter.
/// Løser problemet hvor Cmd+V gik til overlay-vinduet i stedet for teksteditor/browser.
#[tauri::command]
async fn insert_text_from_overlay(app: AppHandle, text: String) -> Result<(), String> {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.set_ignore_cursor_events(true);
        let _ = overlay.hide();
    }
    tauri::async_runtime::spawn_blocking(move || {
        thread::sleep(Duration::from_millis(180));
        insert_text(text)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Trigrer PTT-flow manuelt (3 sek) uden at kræve Option-tast eller Accessibility.
/// Bruges til at verificere at overlay + mikrofon + transskription virker.
#[tauri::command]
async fn trigger_ptt_test(app: AppHandle) -> Result<(), String> {
    if let Some(overlay) = app.get_webview_window("overlay") {
        apply_overlay_to_primary(&overlay)?;
        let _ = overlay.set_always_on_top(true);
        let _ = overlay.show();
        let _ = overlay.set_ignore_cursor_events(false);
    }
    let _ = app.emit_to(EventTarget::webview_window("overlay"), "push-to-talk-start", ());
    tauri::async_runtime::spawn_blocking(|| thread::sleep(Duration::from_millis(3000)))
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit_to(EventTarget::webview_window("overlay"), "push-to-talk-stop", ());
    Ok(())
}

/// Spiller macOS system-lyd via afplay — omgår WebKit AudioContext-restriktionen.
#[tauri::command]
fn play_ui_sound(sound: String) {
    #[cfg(target_os = "macos")]
    {
        let path = match sound.as_str() {
            "start" => "/System/Library/Sounds/Tink.aiff",
            "stop"  => "/System/Library/Sounds/Pop.aiff",
            _       => return,
        };
        let _ = std::process::Command::new("afplay")
            .args([path, "-v", "0.55"])
            .spawn();
    }
}

/// Returnerer om Hey Mikkel har skærmoptagelse-tilladelse.
/// CGPreflightScreenCaptureAccess returnerer true/false uden at vise TCC-dialog (macOS 10.15+).
#[tauri::command]
fn get_screen_recording_status() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGPreflightScreenCaptureAccess() -> bool;
        }
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    { true }
}

/// Anmoder eksplicit om skærmoptagelse-tilladelse via CGRequestScreenCaptureAccess (macOS 10.15+).
/// Viser TCC-dialog én gang hvis tilladelse mangler. Kalder `open_screen_recording_settings` bagefter.
#[tauri::command]
fn request_screen_recording_permission(app: AppHandle) -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGRequestScreenCaptureAccess() -> bool;
        }
        let _ = app;
        let granted = unsafe { CGRequestScreenCaptureAccess() };
        if !granted {
            let _ = open_screen_recording_settings();
        }
        granted
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        true
    }
}

/// Returnerer om macOS Accessibility er accorderet (device_query kræver det for at se Option-tasten).
#[tauri::command]
fn get_accessibility_status() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos_accessibility_client::accessibility::application_is_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
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
fn open_screen_recording_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        for url in [
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture",
        ] {
            let s = std::process::Command::new("open").arg(url).status();
            if s.map(|c| c.success()).unwrap_or(false) {
                return Ok(());
            }
        }
        return Err("Kunne ikke åbne Skærmoptagelse — åbn manuelt under Systemindstillinger → Fortrolighed → Skærmoptagelse.".to_string());
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

#[tauri::command]
fn create_calendar_event(
    title: String,
    date: String,       // YYYY-MM-DD
    start_time: String, // HH:MM
    end_time: String,   // HH:MM
    location: String,
    notes: String,
) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        // Byg AppleScript der opretter begivenhed i den første tilgængelige kalender
        let script = format!(
            r#"
set startStr to "{date} {start_time}:00"
set endStr to "{date} {end_time}:00"
tell application "Calendar"
    set targetCal to first calendar whose writable is true
    tell targetCal
        set newEvent to make new event with properties {{summary:"{title}", start date:date startStr, end date:date endStr}}
        if "{location}" is not "" then
            set location of newEvent to "{location}"
        end if
        if "{notes}" is not "" then
            set description of newEvent to "{notes}"
        end if
    end tell
    reload calendars
end tell
return "OK"
"#,
            title = title.replace('"', "'"),
            date = date,
            start_time = start_time,
            end_time = end_time,
            location = location.replace('"', "'"),
            notes = notes.replace('"', "'"),
        );

        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("Kunne ikke køre osascript: {e}"))?;

        if output.status.success() {
            Ok(format!("Begivenhed oprettet: {} d. {} kl. {}", title, date, start_time))
        } else {
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            Err(format!("Kalender-fejl: {err}"))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(format!("Begivenhed oprettet: {title}"))
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
fn restart_app() {
    // Genstart processen — nødvendigt for at macOS TCC-cache opdateres efter ny tilladelse.
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(0);
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

/// Spørger macOS direkte om Option-modifier-bit er sat — kræver IKKE Accessibility.
/// Bruger NSEvent.modifierFlags (AppKit class method, thread-safe).
/// NSEventModifierFlagOption = 1 << 19 = 0x80000
/// NSEventModifierFlagCommand = 1 << 20 = 0x100000 (ekskluderes så Cmd+Option ikke trigger)
#[cfg(target_os = "macos")]
fn check_option_held() -> bool {
    use objc2::{class, msg_send};
    unsafe {
        let cls = class!(NSEvent);
        let flags: usize = msg_send![cls, modifierFlags];
        flags & (1 << 19) != 0 && flags & (1 << 20) == 0
    }
}

#[cfg(not(target_os = "macos"))]
fn check_option_held() -> bool {
    false
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
    // NSEvent.modifierFlags kræver IKKE Accessibility — vi starter med det samme.
    // Accessibility vises stadig i UI hvis det mangler (kræves til tekst-indsætning).
    thread::spawn(move || {
        #[cfg(target_os = "macos")]
        {
            // Vis prompt én gang; blokerer ikke PTT-loopen.
            let _ = macos_accessibility_client::accessibility::application_is_trusted_with_prompt();
        }

        let mut is_talking = false;
        let mut pending_since: Option<Instant> = None;

        loop {
            let option_held = check_option_held();

            if option_held {
                if !is_talking {
                    let started_at = pending_since.get_or_insert_with(Instant::now);
                    if started_at.elapsed() >= Duration::from_millis(80) {
                        if let Some(main) = app_handle.get_webview_window("main") {
                            let _ = main.hide();
                        }
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
            } else {
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
            insert_text_from_overlay,
            trigger_ptt_test,
            play_ui_sound,
            get_accessibility_status,
            get_screen_recording_status,
            request_screen_recording_permission,
            create_calendar_event,
            request_accessibility_permission,
            set_overlay_interactive,
            show_settings_window,
            hide_main_window,
            open_microphone_privacy,
            open_sound_input_settings,
            open_screentime_settings,
            open_screen_recording_settings,
            open_accessibility_settings,
            restart_app,
            request_macos_av_microphone,
            mic_native::native_mic_start,
            mic_native::native_mic_stop
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
