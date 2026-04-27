/**
 * macOS: beder systemet om mikrofontilladelse (AVAudioApplication + AVCapture)
 * så TCC/ Systemindstillinger får en reel dialog og ofte føjer appen under Mikrofon.
 */
import { invoke } from "@tauri-apps/api/core";

export async function requestNativeMicrophonePermission(): Promise<void> {
  if (!window.__TAURI_INTERNALS__) return;
  await invoke("request_macos_av_microphone");
}
