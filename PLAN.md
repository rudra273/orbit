# Orbit — Plan (Mac-first)

A transparent, always-on-top desktop AI copilot powered by your **local** `gemma4:12b-mlx`
(via Ollama). It floats over every app, can listen to call/system audio, transcribe it,
and answer with your local model. No cloud, fully private.

## Stack
- **Electron** (Node 26) — the overlay shell: transparent, frameless, always-on-top,
  float over all Spaces, hide-from-screen-share, global hotkeys.
- **Vanilla JS + HTML/CSS** renderer (no heavy framework — keeps it fast & simple).
- **Ollama HTTP API** (`localhost:11434`) — chat + model list. No SDK needed.
- **Python 3.12 sidecar** — `mlx-whisper` for Apple-Silicon transcription + system-audio capture.
- **electron-store** — persist settings.

## Project layout
```
orbit/
  package.json
  electron/
    main.js          # window, hotkeys, IPC, sidecar lifecycle, store
    preload.js       # secure contextBridge
  renderer/
    index.html
    styles.css
    app.js           # overlay UI: chat, transcript, settings
  sidecar/
    transcribe.py    # mlx-whisper + audio capture, line-JSON over stdout
    requirements.txt
  PLAN.md
```

## Features (this build)

### Window / overlay
- Transparent, frameless, rounded glass UI.
- Always-on-top, visible on all Spaces, ignores Mission Control.
- Draggable; remembers size & position.
- **Stealth toggle** — `setContentProtection` so it's hidden from screen-shares/recordings.
- Global hotkeys (customizable): toggle show/hide, focus input, clear chat, screenshot-to-ask.
- Click-through mode toggle (mouse passes through when idle).

### Chat (local model)
- Streaming responses from Ollama (`/api/chat`).
- **Thinking ON/OFF button** — toggles the model's reasoning (`think` param). When ON,
  the thinking is shown in a collapsible, dimmed block above the answer; when OFF it's hidden.
- **Model switcher** — dropdown auto-populated from installed Ollama models (`/api/tags`);
  switch active model live; remembers last choice.
- Conversation memory within a session + one-click clear.
- Stop/regenerate.

### Audio copilot
- Capture **system audio** (the call) via ScreenCaptureKit (`getDisplayMedia`) and/or mic.
- Python `mlx-whisper` sidecar transcribes in near-real-time → live transcript pane.
- **Voice-activity trigger** — when speech is detected, auto-surface the overlay.
- "Answer this" button → sends recent transcript to the model as context.

### Settings panel
- Active model + **thinking on/off** default.
- Temperature, system prompt / persona (editable).
- Whisper model (tiny / base / small / medium).
- Audio source: system / mic / both; auto-show-on-speech on/off.
- Stealth (content-protection) on/off; window opacity; theme (dark/glass).
- Global hotkey rebinding.
- Ollama host/port.
- All persisted via electron-store.

## Build phases
1. **Scaffold** — `package.json`, Electron boot, transparent always-on-top window, dev script.
2. **Chat core** — Ollama streaming, model switcher, thinking toggle, system prompt, history.
3. **Settings + persistence** — settings panel, electron-store, hotkey rebinding, stealth.
4. **Audio sidecar** — Python `mlx-whisper`, system-audio capture, live transcript, VAD auto-surface.
5. **Polish** — glass styling, animations, tray icon, packaging notes.

## macOS permissions the user will grant on first run
- **Screen Recording** (required for system-audio capture)
- **Microphone** (if mic source used)
- **Accessibility** (only if we add global typing/automation later)

## Out of scope for now (easy add-ons later)
- RAG over your documents, coding/file-edit tools, Windows support
  (Electron makes Windows mostly a matter of swapping the audio-capture module).
