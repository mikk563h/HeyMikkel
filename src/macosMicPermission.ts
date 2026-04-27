/**
 * macOS: Anmoder om mikrofon via AVFoundation i Rust (korrekt completion block), så
 * "Hey Mikkel" vises under Systemindstillinger → Fortrolighed og sikkerhed → Mikrofon.
 */
import { invoke } from "@tauri-apps/api/core";

export async function requestNativeMicrophonePermission(): Promise<void> {
  if (!window.__TAURI_INTERNALS__) return;
  try {
    await invoke("request_macos_av_microphone");
  } catch {
    // Ignorer — getUserMedia forsøges stadig bagefter
  }
}
