const api = window.orbit;

// ---- State ------------------------------------------------------------------
let settings = {};
let history = []; // [{role, content}]
let streaming = false;
let cur = null; // { contentEl, thinkEl, thinkBody, content, thinking }

const $ = (id) => document.getElementById(id);
const els = {
  messages: $('messages'),
  input: $('input'),
  send: $('sendBtn'),
  stop: $('stopBtn'),
  model: $('modelSelect'),
  think: $('thinkBtn'),
  listen: $('listenBtn'),
  transcript: $('transcript'),
  transcriptBody: $('transcriptBody'),
  answer: $('answerBtn'),
  settingsBtn: $('settingsBtn'),
  hide: $('hideBtn'),
  settings: $('settings'),
  settingsClose: $('settingsClose'),
  toast: $('toast')
};

// ---- Boot -------------------------------------------------------------------
(async function init() {
  settings = await api.getSettings();
  applyTheme();
  updateThinkBtn();
  await refreshModels();
  bindUI();
  bindStream();
  renderEmpty();
  autoGrow();
})();

function updateThinkBtn() {
  const on = !!settings.thinking;
  els.think.classList.toggle('active', on);
  els.think.textContent = on ? '🧠 Think: ON' : '🧠 Think: OFF';
}

function applyTheme() {
  document.body.className = settings.theme === 'dark' ? 'theme-dark' : 'theme-glass';
  document.body.style.opacity = settings.opacity ?? 1;
}

async function refreshModels() {
  const r = await api.listModels();
  const list = r.models && r.models.length ? r.models : [settings.model];
  for (const sel of [els.model, $('setModel')]) {
    if (!sel) continue;
    sel.innerHTML = '';
    for (const m of list) {
      const o = document.createElement('option');
      o.value = o.textContent = m;
      sel.appendChild(o);
    }
    sel.value = settings.model;
  }
  if (!r.ok) toast('⚠️ Ollama not reachable — is it running?');
}

// ---- UI events --------------------------------------------------------------
function bindUI() {
  els.send.onclick = sendMessage;
  els.stop.onclick = () => api.stopChat();
  els.hide.onclick = () => api.hide();

  els.input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  });
  els.input.addEventListener('input', autoGrow);

  els.model.onchange = async () => {
    settings = await api.setSettings({ model: els.model.value });
    $('setModel').value = settings.model;
  };

  els.think.onclick = async () => {
    settings = await api.setSettings({ thinking: !settings.thinking });
    updateThinkBtn();
    toast(settings.thinking ? '🧠 Thinking ON' : '🧠 Thinking OFF');
  };

  els.listen.onclick = toggleListen;
  els.answer.onclick = answerFromTranscript;

  els.settingsBtn.onclick = openSettings;
  els.settingsClose.onclick = () => els.settings.classList.add('hidden');

  api.onAudioStatus(onAudioStatus);

  api.onFocusInput(() => els.input.focus());
  api.onClearChat(clearChat);

  bindSettingsForm();
}

function autoGrow() {
  els.input.style.height = 'auto';
  els.input.style.height = Math.min(els.input.scrollHeight, 120) + 'px';
}

// ---- Chat -------------------------------------------------------------------
function sendMessage() {
  const text = els.input.value.trim();
  if (!text || streaming) return;
  els.input.value = '';
  autoGrow();

  history.push({ role: 'user', content: text });
  addMessage('user', text);

  const messages = [{ role: 'system', content: settings.systemPrompt }, ...history];
  startAssistant();
  setStreaming(true);

  api
    .sendChat({
      model: settings.model,
      messages,
      thinking: settings.thinking,
      temperature: settings.temperature
    })
    .then((res) => {
      if (res && res.ok === false) toast('⚠️ ' + (res.error || 'request failed'));
    });
}

function bindStream() {
  api.onThinking((t) => {
    if (!cur) return;
    revealThink();
    cur.thinking += t;
    cur.thinkBody.textContent = cur.thinking;
    scroll();
  });
  api.onToken((t) => {
    if (!cur) return;
    cur.content += t;
    cur.contentEl.textContent = cur.content;
    scroll();
  });
  api.onDone(() => {
    if (cur) history.push({ role: 'assistant', content: cur.content });
    finishStream();
  });
  api.onError((m) => {
    if (cur) cur.contentEl.textContent = (cur.content || '') + '\n⚠️ ' + m;
    finishStream();
  });
  api.onWarn((m) => toast(m));
}

function setStreaming(on) {
  streaming = on;
  els.send.classList.toggle('hidden', on);
  els.stop.classList.toggle('hidden', !on);
}
function finishStream() {
  setStreaming(false);
  cur = null;
}

function startAssistant() {
  removeEmpty();
  const wrap = document.createElement('div');
  wrap.className = 'msg assistant';
  wrap.innerHTML = '<div class="role">orbit</div>';

  const think = document.createElement('details');
  think.className = 'think hidden';
  think.innerHTML = '<summary>🧠 Thinking…</summary>';
  const thinkBody = document.createElement('div');
  thinkBody.className = 'think-body';
  think.appendChild(thinkBody);

  const bubble = document.createElement('div');
  bubble.className = 'bubble';
  bubble.textContent = '';

  wrap.appendChild(think);
  wrap.appendChild(bubble);
  els.messages.appendChild(wrap);

  cur = { contentEl: bubble, thinkEl: think, thinkBody, content: '', thinking: '' };
  scroll();
}
function revealThink() {
  if (cur && cur.thinkEl.classList.contains('hidden')) {
    cur.thinkEl.classList.remove('hidden');
    cur.thinkEl.open = true;
  }
}

function addMessage(role, text) {
  removeEmpty();
  const wrap = document.createElement('div');
  wrap.className = 'msg ' + role;
  wrap.innerHTML = `<div class="role">${role === 'user' ? 'you' : 'orbit'}</div>`;
  const bubble = document.createElement('div');
  bubble.className = 'bubble';
  bubble.textContent = text;
  wrap.appendChild(bubble);
  els.messages.appendChild(wrap);
  scroll();
}

function clearChat() {
  history = [];
  els.messages.innerHTML = '';
  renderEmpty();
}
function renderEmpty() {
  if (els.messages.children.length) return;
  const e = document.createElement('div');
  e.className = 'empty';
  e.innerHTML = '<div class="big">🛰️</div>Ask Orbit anything.<br/>Running locally on <b>' +
    (settings.model || 'your model') + '</b>.';
  els.messages.appendChild(e);
}
function removeEmpty() {
  const e = els.messages.querySelector('.empty');
  if (e) e.remove();
}
function scroll() {
  els.messages.scrollTop = els.messages.scrollHeight;
}

// ---- Settings form ----------------------------------------------------------
function openSettings() {
  $('setModel').value = settings.model;
  $('setThinking').checked = !!settings.thinking;
  $('setTemp').value = settings.temperature;
  $('tempVal').textContent = settings.temperature;
  $('setSystem').value = settings.systemPrompt;
  $('setStealth').checked = !!settings.stealth;
  $('setOpacity').value = settings.opacity;
  $('opVal').textContent = settings.opacity;
  $('setTheme').value = settings.theme;
  $('setHost').value = settings.ollamaHost;
  $('hkToggle').value = settings.hotkeys.toggle;
  $('hkFocus').value = settings.hotkeys.focus;
  $('hkClear').value = settings.hotkeys.clear;
  $('setAudioSource').value = settings.audioSource;
  $('setWhisper').value = settings.whisperModel;
  $('setAutoShow').checked = !!settings.autoShowOnSpeech;
  els.settings.classList.remove('hidden');
}

function bindSettingsForm() {
  const patch = async (p) => { settings = await api.setSettings(p); };

  $('setModel').onchange = (e) => { patch({ model: e.target.value }); els.model.value = e.target.value; };
  $('setThinking').onchange = async (e) => {
    await patch({ thinking: e.target.checked });
    updateThinkBtn();
  };
  $('setTemp').oninput = (e) => { $('tempVal').textContent = e.target.value; patch({ temperature: parseFloat(e.target.value) }); };
  $('setSystem').onchange = (e) => patch({ systemPrompt: e.target.value });
  $('setStealth').onchange = (e) => patch({ stealth: e.target.checked });
  $('setOpacity').oninput = (e) => { $('opVal').textContent = e.target.value; settings.opacity = parseFloat(e.target.value); document.body.style.opacity = e.target.value; patch({ opacity: parseFloat(e.target.value) }); };
  $('setTheme').onchange = (e) => { settings.theme = e.target.value; applyTheme(); patch({ theme: e.target.value }); };
  $('setHost').onchange = async (e) => { await patch({ ollamaHost: e.target.value }); refreshModels(); };
  $('setAudioSource').onchange = (e) => patch({ audioSource: e.target.value });
  $('setWhisper').onchange = (e) => patch({ whisperModel: e.target.value });
  $('setAutoShow').onchange = (e) => patch({ autoShowOnSpeech: e.target.checked });

  // Hotkey capture
  for (const [id, key] of [['hkToggle', 'toggle'], ['hkFocus', 'focus'], ['hkClear', 'clear']]) {
    const inp = $(id);
    inp.addEventListener('keydown', async (e) => {
      e.preventDefault();
      const accel = toAccelerator(e);
      if (!accel) return;
      inp.value = accel;
      const hotkeys = { ...settings.hotkeys, [key]: accel };
      await patch({ hotkeys });
    });
  }
}

function toAccelerator(e) {
  const parts = [];
  if (e.metaKey) parts.push('CommandOrControl');
  if (e.ctrlKey && !e.metaKey) parts.push('Control');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  let k = e.key;
  if (['Meta', 'Control', 'Alt', 'Shift'].includes(k)) return null;
  if (k === ' ') k = 'Space';
  else if (k === 'Backspace') k = 'Backspace';
  else if (k.length === 1) k = k.toUpperCase();
  parts.push(k);
  return parts.join('+');
}

// ---- Audio listening mode ---------------------------------------------------
const SR = 16000; // AudioContext resamples sources to this for Whisper
const THRESH = 0.012; // RMS speech threshold
const SILENCE_HANG_MS = 700; // silence after speech that ends a segment
const MIN_SEG_SEC = 0.4; // ignore blips shorter than this

let listening = false;
let audioCtx = null;
let processor = null;
let mediaStreams = [];
let srcNodes = [];
let speechBuf = [];
let speaking = false;
let silenceMs = 0;
let transcriptText = '';

async function toggleListen() {
  if (listening) return stopListen();
  return startListen();
}

function withTimeout(promise, ms, msg) {
  return Promise.race([
    promise,
    new Promise((_, rej) => setTimeout(() => rej(new Error(msg)), ms))
  ]);
}

async function startListen() {
  try {
    els.listen.textContent = '🎙️ Starting…';
    const wanted = settings.audioSource || 'system';

    await api.audioStart(); // boots the whisper sidecar (may download model first run)

    // Attempt capture. The attempt itself is what makes macOS prompt AND register
    // "Orbit" in the Screen Recording / Microphone lists — so we must try it, not
    // bail out beforehand.
    mediaStreams = [];
    try {
      if (wanted === 'mic' || wanted === 'both') {
        mediaStreams.push(await navigator.mediaDevices.getUserMedia({ audio: true }));
      }
      if (wanted === 'system' || wanted === 'both') {
        const s = await withTimeout(
          navigator.mediaDevices.getDisplayMedia({ video: true, audio: true }),
          15000,
          'system-audio capture timed out'
        );
        s.getVideoTracks().forEach((t) => t.stop()); // we only want the audio
        mediaStreams.push(s);
      }
    } catch (capErr) {
      await cleanupAudio();
      await api.audioStop();
      els.listen.textContent = '🎙️ Listen';
      const perm = await api.permStatus();
      if (wanted !== 'mic' && perm.screen !== 'granted') {
        await api.openPerm('screen');
        toast('macOS asked for permission. Turn ON “Orbit” under Screen Recording, then QUIT & relaunch Orbit.');
      } else if (perm.mic === 'denied') {
        await api.openPerm('mic');
        toast('Turn ON “Orbit” under Microphone in System Settings, then try again.');
      } else {
        toast('Audio capture failed: ' + (capErr.message || capErr));
      }
      updateListenUI();
      return;
    }
    if (!mediaStreams.length) throw new Error('no audio source');

    audioCtx = new AudioContext({ sampleRate: SR });
    processor = audioCtx.createScriptProcessor(4096, 1, 1);
    srcNodes = mediaStreams.map((s) => {
      const node = audioCtx.createMediaStreamSource(s);
      node.connect(processor);
      return node;
    });
    // ScriptProcessor only runs when connected to the destination, but we
    // mute it (gain 0) so we don't echo the call back through the speakers.
    const mute = audioCtx.createGain();
    mute.gain.value = 0;
    processor.connect(mute);
    mute.connect(audioCtx.destination);
    processor.onaudioprocess = onAudio;

    listening = true;
    updateListenUI();
    els.transcript.classList.remove('hidden');
    toast('🎙️ Listening — say something');
  } catch (e) {
    toast('Audio error: ' + (e.message || e));
    await cleanupAudio();
    updateListenUI();
  }
}

async function stopListen() {
  await cleanupAudio();
  if (speechBuf.length) finalizeSegment();
  await api.audioStop();
  listening = false;
  updateListenUI();
  toast('Stopped listening');
}

async function cleanupAudio() {
  if (processor) {
    processor.onaudioprocess = null;
    processor.disconnect();
    processor = null;
  }
  srcNodes.forEach((n) => n.disconnect());
  srcNodes = [];
  mediaStreams.forEach((s) => s.getTracks().forEach((t) => t.stop()));
  mediaStreams = [];
  if (audioCtx) {
    try { await audioCtx.close(); } catch {}
    audioCtx = null;
  }
}

function onAudio(e) {
  const input = e.inputBuffer.getChannelData(0);
  let sum = 0;
  for (let i = 0; i < input.length; i++) sum += input[i] * input[i];
  const rms = Math.sqrt(sum / input.length);
  const frameMs = (input.length / SR) * 1000;

  if (rms > THRESH) {
    speaking = true;
    silenceMs = 0;
    speechBuf.push(new Float32Array(input));
  } else if (speaking) {
    speechBuf.push(new Float32Array(input));
    silenceMs += frameMs;
    if (silenceMs > SILENCE_HANG_MS) finalizeSegment();
  }
}

function finalizeSegment() {
  const total = speechBuf.reduce((a, b) => a + b.length, 0);
  const buf = speechBuf;
  speechBuf = [];
  speaking = false;
  silenceMs = 0;
  if (total < SR * MIN_SEG_SEC) return; // too short to be speech

  const merged = new Float32Array(total);
  let o = 0;
  for (const c of buf) {
    merged.set(c, o);
    o += c.length;
  }
  api.transcribe(merged.buffer).then((r) => {
    if (r && r.text) addTranscript(r.text);
  });
}

function addTranscript(text) {
  transcriptText += (transcriptText ? ' ' : '') + text;
  const line = document.createElement('div');
  line.textContent = text;
  els.transcriptBody.appendChild(line);
  els.transcriptBody.scrollTop = els.transcriptBody.scrollHeight;
  els.transcript.classList.remove('hidden');
  if (settings.autoShowOnSpeech) api.show();
}

function answerFromTranscript() {
  if (!transcriptText.trim()) {
    toast('Nothing transcribed yet');
    return;
  }
  els.input.value =
    'Here is what was just said in my call:\n"' +
    transcriptText.trim() +
    '"\n\nHelp me respond / answer this.';
  autoGrow();
  sendMessage();
  transcriptText = '';
}

function onAudioStatus(status) {
  if (status === 'loading') els.listen.textContent = '🎙️ Loading…';
  else if (status === 'ready') updateListenUI();
  else if (status === 'crashed') {
    toast('⚠️ Transcriber crashed (is mlx-whisper installed? run setup)');
    listening = false;
    cleanupAudio();
    updateListenUI();
  } else if (typeof status === 'string' && status.startsWith('error')) {
    toast('⚠️ ' + status);
  }
}

function updateListenUI() {
  els.listen.classList.toggle('active', listening);
  els.listen.textContent = listening ? '🎙️ Listening' : '🎙️ Listen';
  if (!listening && !transcriptText) els.transcript.classList.add('hidden');
}

// ---- Toast ------------------------------------------------------------------
let toastTimer = null;
function toast(msg) {
  els.toast.textContent = msg;
  els.toast.classList.remove('hidden');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => els.toast.classList.add('hidden'), 2600);
}
