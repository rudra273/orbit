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
const providers = require('./providers');

// Load a git-ignored .env from the repo root (KEY=value lines) into process.env
// so API keys can be supplied env-var style without touching any saved file.
(function loadDotEnv() {
  try {
    const envPath = path.join(__dirname, '..', '.env');
    const raw = fs.readFileSync(envPath, 'utf8');
    for (const line of raw.split('\n')) {
      const m = line.match(/^\s*([A-Z0-9_]+)\s*=\s*(.*)\s*$/i);
      if (!m) continue;
      const key = m[1];
      let val = m[2].replace(/^["']|["']$/g, '');
      if (!(key in process.env)) process.env[key] = val;
    }
  } catch {
    // no .env — fine
  }
})();

let win = null;
let activeChat = null; // AbortController for the in-flight chat request

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

  // Open links (e.g. from rendered markdown) in the default browser, never
  // inside the overlay — and block any in-app navigation away from the UI.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https?:/.test(url)) shell.openExternal(url);
    return { action: 'deny' };
  });
  win.webContents.on('will-navigate', (e, url) => {
    if (!url.startsWith('file://')) {
      e.preventDefault();
      if (/^https?:/.test(url)) shell.openExternal(url);
    }
  });

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

// ---- Provider bridge (streamed via IPC to avoid CORS) -----------------------
// Provider metadata + which ones currently have a usable key (stored or env).
ipcMain.handle('providers:list', () => {
  const s = store.getAll();
  return providers.list().map((p) => ({
    ...p,
    hasKey: !p.needsKey || !!providers.resolveKey(p.id, s.apiKeys),
    // surface whether the key came from an env var (so the UI can show it's set)
    keyFromEnv: p.needsKey && !((s.apiKeys || {})[p.id] || '').trim() && !!providers.resolveKey(p.id, s.apiKeys)
  }));
});

// List the active provider's models. The renderer passes the provider id so the
// dropdown can repopulate when the user switches providers.
ipcMain.handle('models:list', async (evt, providerId) => {
  const s = store.getAll();
  const id = providerId || s.provider || 'local';
  const provider = providers.get(id);
  try {
    const models = await provider.listModels({
      ollamaHost: s.ollamaHost,
      apiKey: providers.resolveKey(id, s.apiKeys)
    });
    return { ok: true, provider: id, models };
  } catch (e) {
    return { ok: false, provider: id, error: String(e), models: [] };
  }
});

ipcMain.handle('chat:stop', () => {
  if (activeChat) activeChat.abort();
  activeChat = null;
  return true;
});

ipcMain.handle('chat:send', async (evt, payload) => {
  const s = store.getAll();
  const id = payload.provider || s.provider || 'local';
  const provider = providers.get(id);

  const send = (channel, data) => {
    if (win && !win.isDestroyed()) win.webContents.send(channel, data);
  };

  activeChat = new AbortController();
  const opts = {
    ollamaHost: s.ollamaHost,
    apiKey: providers.resolveKey(id, s.apiKeys),
    model: payload.model || s.model,
    messages: payload.messages,
    temperature: payload.temperature ?? s.temperature,
    thinking: !!(payload.thinking ?? s.thinking),
    signal: activeChat.signal
  };
  const handlers = {
    onThinking: (t) => send('chat:thinking', t),
    onToken: (t) => send('chat:token', t),
    // model requires reasoning — couldn't honor "Think OFF" for this one
    onForcedThinking: () => send('chat:forcedThinking', { model: opts.model, provider: id })
  };

  try {
    await provider.streamChat(opts, handlers);
    send('chat:done', { model: opts.model, provider: id });
    return { ok: true };
  } catch (e) {
    if (activeChat === null || /abort/i.test(String(e))) {
      send('chat:done', { model: opts.model, provider: id, aborted: true });
      return { ok: true }; // user-aborted — finalize the partial message
    }
    // Local fallback: some Ollama models reject the `think` param entirely.
    if (id === 'local' && /think|reasoning|does not support/i.test(String(e))) {
      try {
        send('chat:warn', 'This model does not support a separate thinking mode; answering normally.');
        await provider.streamChat({ ...opts, thinking: undefined }, handlers);
        send('chat:done', { model: opts.model, provider: id });
        return { ok: true };
      } catch (e2) {
        send('chat:error', String(e2));
        return { ok: false, error: String(e2) };
      }
    }
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

// ---- Chat history IPC ----
ipcMain.handle('chats:list', () => store.listChats());
ipcMain.handle('chats:get', (evt, id) => store.getChat(id));
ipcMain.handle('chats:save', (evt, chat) => store.saveChat(chat));
ipcMain.handle('chats:delete', (evt, id) => store.deleteChat(id));

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
