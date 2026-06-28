# 🛰️ Orbit

A transparent, always-on-top **local** AI copilot for macOS. Floats over every app,
chats with your Ollama models, and can listen to your call/system audio and transcribe
it on-device with MLX Whisper. Nothing leaves your machine.

## Requirements
- macOS on Apple Silicon
- [Ollama](https://ollama.com) running with at least one model (e.g. `gemma4:12b-mlx`)
- Node.js 18+ and Python 3.12

## Setup
```bash
# 1. Node deps (Electron)
npm install

# 2. Python transcription sidecar (one-time)
./setup.sh          # creates .venv and installs mlx-whisper
```

## Run
```bash
npm start
```

## Features
- **Overlay** — transparent, frameless, always-on-top, floats over all Spaces. Drag by the title bar.
- **Chat** — streaming replies from your local Ollama model.
- **🧠 Think: ON/OFF** — toggles the model's reasoning; shown in a collapsible block when on.
- **Model switcher** — dropdown auto-populated from your installed Ollama models.
- **🎙️ Listen** — captures system/call audio (or mic), transcribes live with MLX Whisper,
  and can auto-surface the window when speech is detected. "Answer this" feeds the
  transcript to the model.
- **Settings** — model, temperature, system prompt, whisper model, audio source,
  stealth (hide from screen-share), opacity, theme, Ollama host, hotkey rebinding.

## Hotkeys (default, rebind in Settings)
- `⌘⇧Space` — show / hide
- `⌘⇧K` — focus input
- `⌘⇧⌫` — clear chat

## Build a real Orbit.app (needed for system-audio capture)
The dev binary (`npm start`) can't get macOS **Screen Recording** permission, so
system/call audio won't work from it. Build a proper app bundle instead:
```bash
./package-app.sh          # builds ~/Applications/Orbit.app (ad-hoc signed)
open ~/Applications/Orbit.app
```
Re-run `./package-app.sh` whenever you change the code to refresh the bundle.
(Mic input works fine from the dev build; only system audio needs the bundle.)

## macOS permissions
On first use of **🎙️ Listen**, macOS will ask for **Screen Recording** permission
(required to capture system audio via ScreenCaptureKit) and **Microphone** if you pick
the mic source. Grant them in System Settings → Privacy & Security, then toggle Listen again.

## Notes
- First time you press Listen, the Whisper model downloads (~150 MB for `base`).
- The model side is just HTTP to `localhost:11434`; no API keys, fully offline.
