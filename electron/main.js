const {
  app,
  BrowserWindow,
  globalShortcut,
  ipcMain,
  session,
  desktopCapturer,
  systemPreferences,
  shell
} = require('electron');
const path = require('path');
const os = require('os');
const fs = require('fs');
const { spawn } = require('child_process');
const store = require('./store');

let win = null;
let activeChat = null; // AbortController for the in-flight Ollama request

// ---- Transcription sidecar state -------------------------------------------
let sidecar = null;
let sidecarReady = false;
let reqId = 0;
const pending = new Map();

function createWindow() {
  const s = store.getAll();
  const b = s.bounds || {};

  win = new BrowserWindow({
    width: b.width || 440,
    height: b.height || 560,
    x: b.x ?? undefined,
    y: b.y ?? undefined,
    frame: false,
    transparent: true,
    resizable: true,
    hasShadow: false,
    skipTaskbar: true,
    fullscreenable: false,
    alwaysOnTop: true,
    backgroundColor: '#00000000',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  });

  // Float above virtually everything, on every Space, even over fullscreen apps.
  win.setAlwaysOnTop(true, 'screen-saver');
  win.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });

  // Stealth: hide window from screen capture / screen-share when enabled.
  win.setContentProtection(!!s.stealth);

  win.loadFile(path.join(__dirname, '..', 'renderer', 'index.html'));

  // Persist size/position.
  const saveBounds = () => {
    if (!win) return;
    const wb = win.getBounds();
    store.set({ bounds: { ...store.get('bounds'), ...wb } });
  };
  win.on('resized', saveBounds);
  win.on('moved', saveBounds);

  win.on('closed', () => {
    win = null;
  });

  if (process.argv.includes('--dev')) win.webContents.openDevTools({ mode: 'detach' });
}

// ---- Global hotkeys ---------------------------------------------------------
function registerHotkeys() {
  globalShortcut.unregisterAll();
  const hk = store.get('hotkeys') || {};
  const safe = (accel, fn) => {
    if (!accel) return;
    try {
      globalShortcut.register(accel, fn);
    } catch (e) {
      console.error('hotkey register failed', accel, e);
    }
  };

  safe(hk.toggle, () => {
    if (!win) return;
    if (win.isVisible() && win.isFocused()) win.hide();
    else {
      win.show();
      win.focus();
    }
  });
  safe(hk.focus, () => {
    if (!win) return;
    win.show();
    win.focus();
    win.webContents.send('focus-input');
  });
  safe(hk.clear, () => win && win.webContents.send('clear-chat'));
}

// ---- Ollama bridge (streamed via IPC to avoid CORS) -------------------------
ipcMain.handle('models:list', async () => {
  const host = store.get('ollamaHost');
  try {
    const r = await fetch(`${host}/api/tags`);
    const j = await r.json();
    return { ok: true, models: (j.models || []).map((m) => m.name) };
  } catch (e) {
    return { ok: false, error: String(e), models: [] };
  }
});

ipcMain.handle('chat:stop', () => {
  if (activeChat) activeChat.abort();
  activeChat = null;
  return true;
});

ipcMain.handle('chat:send', async (evt, payload) => {
  const s = store.getAll();
  const host = s.ollamaHost;
  const body = {
    model: payload.model || s.model,
    messages: payload.messages,
    stream: true,
    options: { temperature: payload.temperature ?? s.temperature }
  };
  // Send `think` EXPLICITLY (true OR false). Reasoning models default to
  // thinking ON, so omitting the flag is not enough to disable it.
  const wantThinking = !!(payload.thinking ?? s.thinking);

  const send = (channel, data) => {
    if (win && !win.isDestroyed()) win.webContents.send(channel, data);
  };

  const run = async (includeThinkParam) => {
    if (includeThinkParam) body.think = wantThinking;
    else delete body.think; // fallback for models that reject the param entirely
    activeChat = new AbortController();
    const resp = await fetch(`${host}/api/chat`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: activeChat.signal
    });
    if (!resp.ok) {
      const text = await resp.text();
      throw new Error(`Ollama ${resp.status}: ${text}`);
    }
    const reader = resp.body.getReader();
    const decoder = new TextDecoder();
    let buf = '';
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      let nl;
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl).trim();
        buf = buf.slice(nl + 1);
        if (!line) continue;
        let obj;
        try {
          obj = JSON.parse(line);
        } catch {
          continue;
        }
        if (obj.message) {
          if (obj.message.thinking) send('chat:thinking', obj.message.thinking);
          if (obj.message.content) send('chat:token', obj.message.content);
        }
        if (obj.done) send('chat:done', { model: body.model });
      }
    }
  };

  try {
    await run(true);
    return { ok: true };
  } catch (e) {
    // If the model doesn't accept a `think` param at all, retry without it.
    if (activeChat !== null && /think|reasoning|does not support/i.test(String(e))) {
      try {
        send('chat:warn', 'This model does not support a separate thinking mode; answering normally.');
        await run(false);
        return { ok: true };
      } catch (e2) {
        send('chat:error', String(e2));
        return { ok: false, error: String(e2) };
      }
    }
    if (activeChat === null) return { ok: true }; // user-aborted
    send('chat:error', String(e));
    return { ok: false, error: String(e) };
  } finally {
    activeChat = null;
  }
});

// ---- Transcription sidecar --------------------------------------------------
function sidecarPython() {
  // Resolve the venv python across dev (run from repo) and packaged (Orbit.app).
  const candidates = [
    process.env.ORBIT_PYTHON,
    path.join(__dirname, '..', '.venv', 'bin', 'python'), // dev: repo/.venv
    '/Users/rudra/projects/orbit/.venv/bin/python' // packaged: point back at the repo venv
  ].filter(Boolean);
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  return 'python3.12';
}

function sidecarScript() {
  const candidates = [
    path.join(__dirname, '..', 'sidecar', 'transcribe.py'),
    '/Users/rudra/projects/orbit/sidecar/transcribe.py'
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  return candidates[0];
}

function sendToWin(channel, data) {
  if (win && !win.isDestroyed()) win.webContents.send(channel, data);
}

function startSidecar() {
  if (sidecar) return;
  const py = sidecarPython();
  const script = sidecarScript();
  sidecar = spawn(py, [script, store.get('whisperModel') || 'base'], {
    stdio: ['pipe', 'pipe', 'pipe']
  });
  let buf = '';
  sidecar.stdout.on('data', (d) => {
    buf += d.toString();
    let nl;
    while ((nl = buf.indexOf('\n')) >= 0) {
      const line = buf.slice(0, nl).trim();
      buf = buf.slice(nl + 1);
      if (line) handleSidecarLine(line);
    }
  });
  sidecar.stderr.on('data', (d) => console.error('[whisper]', d.toString().trim()));
  sidecar.on('exit', (code) => {
    sidecar = null;
    sidecarReady = false;
    for (const p of pending.values()) p.resolve('');
    pending.clear();
    sendToWin('audio:status', code === 0 ? 'stopped' : 'crashed');
  });
  sendToWin('audio:status', 'loading');
}

function handleSidecarLine(line) {
  let m;
  try {
    m = JSON.parse(line);
  } catch {
    return;
  }
  if (m.type === 'log') {
    if (m.msg === 'ready') {
      sidecarReady = true;
      sendToWin('audio:status', 'ready');
    }
  } else if (m.type === 'result') {
    const p = pending.get(m.id);
    if (p) {
      p.resolve(m.text || '');
      pending.delete(m.id);
    }
  } else if (m.type === 'error') {
    console.error('[whisper error]', m.msg);
    sendToWin('audio:status', 'error: ' + m.msg);
    if (m.id != null) {
      const p = pending.get(m.id);
      if (p) {
        p.resolve('');
        pending.delete(m.id);
      }
    }
  }
}

ipcMain.handle('audio:start', () => {
  startSidecar();
  return { ready: sidecarReady };
});

ipcMain.handle('audio:stop', () => {
  if (sidecar) {
    sidecar.kill();
    sidecar = null;
    sidecarReady = false;
  }
  return true;
});

ipcMain.handle('audio:transcribe', async (evt, arrayBuffer) => {
  if (!sidecar || !sidecarReady) return { text: '' };
  const id = ++reqId;
  const fp = path.join(os.tmpdir(), `orbit-seg-${id}.f32`);
  try {
    fs.writeFileSync(fp, Buffer.from(arrayBuffer));
  } catch (e) {
    return { text: '', error: String(e) };
  }
  const text = await new Promise((resolve) => {
    pending.set(id, { resolve });
    sidecar.stdin.write(JSON.stringify({ id, path: fp }) + '\n');
  });
  try {
    fs.unlinkSync(fp);
  } catch {}
  return { text };
});

ipcMain.handle('win:show', () => {
  if (win) {
    win.show();
    win.focus();
  }
});

// Report macOS capture permission status so the UI can guide the user.
ipcMain.handle('perm:status', () => {
  if (process.platform !== 'darwin') return { screen: 'granted', mic: 'granted' };
  return {
    screen: systemPreferences.getMediaAccessStatus('screen'),
    mic: systemPreferences.getMediaAccessStatus('microphone')
  };
});

// Open the relevant System Settings privacy pane.
ipcMain.handle('perm:open', (evt, kind) => {
  const url =
    kind === 'mic'
      ? 'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone'
      : 'x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture';
  shell.openExternal(url);
});

// ---- Settings + window control IPC -----------------------------------------
ipcMain.handle('settings:get', () => store.getAll());
ipcMain.handle('settings:set', (evt, patch) => {
  const next = store.set(patch);
  if ('stealth' in patch && win) win.setContentProtection(!!next.stealth);
  if ('clickThrough' in patch && win) win.setIgnoreMouseEvents(!!next.clickThrough, { forward: true });
  if ('hotkeys' in patch) registerHotkeys();
  return next;
});

ipcMain.handle('win:hide', () => win && win.hide());
ipcMain.handle('win:setIgnoreMouse', (evt, ignore) =>
  win && win.setIgnoreMouseEvents(!!ignore, { forward: true })
);

// Yellow light → minimize to a slim composer-only strip; click again to restore.
let expandedHeight = null;
ipcMain.handle('win:compact', () => {
  if (!win) return { compact: false };
  const b = win.getBounds();
  const COMPACT_H = 132;
  const isCompact = b.height <= COMPACT_H + 4;
  if (isCompact) {
    win.setBounds({ ...b, height: expandedHeight || 560 }, true);
    return { compact: false };
  }
  expandedHeight = b.height;
  win.setBounds({ ...b, height: COMPACT_H }, true);
  return { compact: true };
});

// Green light → widen a step (not fullscreen); cycles through a few widths and
// wraps back to the narrow default. Keeps the window on-screen.
const WIDTHS = [440, 560, 680, 800];
ipcMain.handle('win:widen', () => {
  if (!win) return { width: 440 };
  const b = win.getBounds();
  // pick the next width larger than current, else wrap to the smallest
  const next = WIDTHS.find((w) => w > b.width + 8) ?? WIDTHS[0];
  win.setBounds({ ...b, width: next }, true);
  return { width: next };
});

app.whenReady().then(() => {
  // Route getDisplayMedia() requests to capture the screen's system audio
  // (ScreenCaptureKit loopback) without showing the macOS picker.
  session.defaultSession.setDisplayMediaRequestHandler(
    (request, callback) => {
      desktopCapturer
        .getSources({ types: ['screen'] })
        .then((sources) => {
          if (!sources || !sources.length) {
            callback(); // deny -> getDisplayMedia rejects in renderer (no hang)
          } else {
            callback({ video: sources[0], audio: 'loopback' });
          }
        })
        .catch((err) => {
          console.error('getSources failed:', err);
          callback(); // deny cleanly instead of leaving the request hanging
        });
    },
    { useSystemPicker: false }
  );

  createWindow();
  registerHotkeys();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('will-quit', () => {
  globalShortcut.unregisterAll();
  if (sidecar) sidecar.kill();
});
// Keep running as a floating utility even with no windows (macOS).
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
