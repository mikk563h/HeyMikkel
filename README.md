# Hey Mikkel

Minimal Tauri MVP for a local AI voice assistant.

## What works in this MVP

- Save OpenAI API key locally.
- Hold `CMD` or the in-app button to record speech.
- Transcribe speech through OpenAI.
- Rewrite spoken Danish into polished text.
- Detect screen-reading commands like "kig på min skærm".
- Capture the primary screen and send it with the spoken instruction to OpenAI.
- Generate LinkedIn-style replies in Danish.
- Show the answer in an overlay-style app window.
- Copy or paste the result into the active text field.
- Refine the result: shorter, more personal, more professional, try again.

## Run

Install dependencies:

```bash
npm install
```

Run the frontend check:

```bash
npm run build
```

Run the desktop app:

```bash
npm run tauri -- dev
```

## Required local setup

Tauri needs Rust and Cargo. If `npm run tauri -- info` says Rust is missing, install it from:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

On macOS, the app may ask for microphone, screen recording, accessibility and automation permissions. These are needed for voice capture, screenshots and paste-at-cursor.

If you see `Invalid constraint` or `0 devices` in the terminal, restart the Tauri app after the first start. Also confirm Hey Mikkel is enabled for Microphone in System Settings.

### No “Sound in” / input devices in macOS (e.g. Mac mini)

If **System Settings → Sound → Sound in** says **no input devices**, the OS is not exposing any microphone. **Desktop Macs (mini, Studio, Pro) often have no built-in mic** — connect a **USB mic, USB headset, or interface**. The app cannot invent hardware. After plugging in, check **Sound in** again, try `sudo killall coreaudiod` in Terminal, or reboot.

### Work on another computer

See [ARBEJD-ANDEN-MAC.md](./ARBEJD-ANDEN-MAC.md) (Danish) for GitHub + clone + `npm install` + `npm run tauri dev` on a second machine.

## MVP test flow

1. Open the app and add an OpenAI API key.
2. Open LinkedIn and find a post.
3. Hold `CMD` or the in-app button.
4. Say: "Hej Mikkel, kig på min skærm og lav et godt svar til det her opslag."
5. Release the hotkey/button.
6. Copy or insert the generated answer.
