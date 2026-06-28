// Tiny JSON-file settings store (no external deps, CJS-safe).
const fs = require('fs');
const path = require('path');
const { app } = require('electron');

const DEFAULTS = {
  // model / chat
  ollamaHost: 'http://127.0.0.1:11434',
  model: 'gemma4:12b-mlx',
  thinking: false,
  temperature: 0.7,
  systemPrompt:
    'You are Orbit, a concise, helpful local AI copilot. Answer directly and briefly unless asked for detail.',

  // window / overlay
  opacity: 0.96,
  theme: 'glass', // 'glass' | 'dark'
  stealth: false, // hide from screen-share / recording
  clickThrough: false,
  bounds: { width: 440, height: 560, x: null, y: null },

  // hotkeys (Electron accelerator strings)
  hotkeys: {
    toggle: 'CommandOrControl+Shift+Space',
    focus: 'CommandOrControl+Shift+K',
    clear: 'CommandOrControl+Shift+Backspace'
  },

  // audio
  audioSource: 'system', // 'system' | 'mic' | 'both'
  whisperModel: 'base',
  autoShowOnSpeech: true
};

let cachePath = null;
let data = null;

function file() {
  if (!cachePath) cachePath = path.join(app.getPath('userData'), 'orbit-settings.json');
  return cachePath;
}

function load() {
  if (data) return data;
  try {
    const raw = fs.readFileSync(file(), 'utf8');
    data = { ...DEFAULTS, ...JSON.parse(raw) };
  } catch {
    data = { ...DEFAULTS };
  }
  return data;
}

function save() {
  try {
    fs.writeFileSync(file(), JSON.stringify(data, null, 2));
  } catch (e) {
    console.error('settings save failed', e);
  }
}

module.exports = {
  DEFAULTS,
  getAll() {
    return { ...load() };
  },
  get(key) {
    return load()[key];
  },
  set(patch) {
    load();
    data = { ...data, ...patch };
    save();
    return { ...data };
  }
};
