const api = window.orbit;

// ---- State ------------------------------------------------------------------
let settings = {};
let history = []; // [{role, content}]
let streaming = false;
let cur = null; // { contentEl, thinkEl, thinkBody, content, thinking }
let currentChatId = null; // id of the chat being viewed (null = fresh, unsaved)

const $ = (id) => document.getElementById(id);
const els = {
  messages: $('messages'),
  input: $('input'),
  send: $('sendBtn'),
  stop: $('stopBtn'),
  model: $('modelSelect'),
  skill: $('skillSelect'),
  think: $('thinkBtn'),
  mic: $('micBtn'),
  listen: $('listenBtn'),
  transcript: $('transcript'),
  transcriptBody: $('transcriptBody'),
  answer: $('answerBtn'),
  settingsBtn: $('settingsBtn'),
  settings: $('settings'),
  settingsClose: $('settingsClose'),
  toast: $('toast'),
  tlClose: $('tlClose'),
  tlMin: $('tlMin'),
  tlZoom: $('tlZoom'),
  sidebarBtn: $('sidebarBtn'),
  sidebar: $('sidebar'),
  sidebarScrim: $('sidebarScrim'),
  newChatBtn: $('newChatBtn'),
  chatList: $('chatList')
};

// small helper: build an <svg><use href="#id"/></svg> string
function icon(id, cls) {
  return `<svg class="${cls || ''}" viewBox="0 0 24 24"><use href="#${id}"/></svg>`;
}

// ---- Markdown ---------------------------------------------------------------
// Compact, dependency-free markdown → HTML for assistant messages.
// Escapes HTML first (content is local/self-authored, but we stay safe), then
// applies a practical subset: fenced + inline code, bold/italic, headings,
// lists, blockquotes, links, and paragraph/line breaks. Tolerates an unclosed
// ``` fence mid-stream so streaming text doesn't flicker.
function escapeHtml(s) {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function inlineMd(s) {
  // inline code first, so its contents aren't touched by other rules
  const codes = [];
  s = s.replace(/`([^`]+)`/g, (_, c) => {
    codes.push(c);
    return '@@CODE' + (codes.length - 1) + '@@';
  });
  // links [text](url)
  s = s.replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, '<a href="$2">$1</a>');
  // bold then italic (bold uses ** or __, italic uses single * or _)
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/__([^_]+)__/g, '<strong>$1</strong>');
  s = s.replace(/(^|[^*])\*([^*\n]+)\*/g, '$1<em>$2</em>');
  s = s.replace(/(^|[^_])_([^_\n]+)_/g, '$1<em>$2</em>');
  // restore inline code
  s = s.replace(/@@CODE(\d+)@@/g, (_, i) => '<code>' + codes[+i] + '</code>');
  return s;
}

function renderMarkdown(src) {
  const text = escapeHtml(src || '');
  const lines = text.split('\n');
  let html = '';
  let i = 0;
  let listType = null; // 'ul' | 'ol' | null

  const closeList = () => { if (listType) { html += '</' + listType + '>'; listType = null; } };

  while (i < lines.length) {
    let line = lines[i];

    // fenced code block ``` (optional language)
    const fence = line.match(/^\s*```(\w*)\s*$/);
    if (fence) {
      closeList();
      const lang = fence[1];
      const buf = [];
      i++;
      while (i < lines.length && !/^\s*```\s*$/.test(lines[i])) { buf.push(lines[i]); i++; }
      i++; // skip closing fence (if missing, we just consumed to EOF — fine mid-stream)
      html += '<pre><code' + (lang ? ' class="lang-' + lang + '"' : '') + '>' +
        buf.join('\n') + '</code></pre>';
      continue;
    }

    // heading #..######
    const h = line.match(/^\s*(#{1,6})\s+(.*)$/);
    if (h) {
      closeList();
      const lvl = h[1].length;
      html += '<h' + lvl + ' class="md-h">' + inlineMd(h[2]) + '</h' + lvl + '>';
      i++;
      continue;
    }

    // blockquote (escapeHtml has already turned a leading ">" into "&gt;")
    if (/^\s*&gt;\s?/.test(line)) {
      closeList();
      html += '<blockquote>' + inlineMd(line.replace(/^\s*&gt;\s?/, '')) + '</blockquote>';
      i++;
      continue;
    }

    // unordered list item
    const ul = line.match(/^\s*[-*+]\s+(.*)$/);
    if (ul) {
      if (listType !== 'ul') { closeList(); html += '<ul>'; listType = 'ul'; }
      html += '<li>' + inlineMd(ul[1]) + '</li>';
      i++;
      continue;
    }
    // ordered list item
    const ol = line.match(/^\s*\d+\.\s+(.*)$/);
    if (ol) {
      if (listType !== 'ol') { closeList(); html += '<ol>'; listType = 'ol'; }
      html += '<li>' + inlineMd(ol[1]) + '</li>';
      i++;
      continue;
    }

    // blank line → paragraph break
    if (/^\s*$/.test(line)) {
      closeList();
      i++;
      continue;
    }

    // plain paragraph — gather consecutive non-special lines
    closeList();
    const para = [line];
    i++;
    while (
      i < lines.length &&
      !/^\s*$/.test(lines[i]) &&
      !/^\s*```/.test(lines[i]) &&
      !/^\s*#{1,6}\s/.test(lines[i]) &&
      !/^\s*[-*+]\s/.test(lines[i]) &&
      !/^\s*\d+\.\s/.test(lines[i]) &&
      !/^\s*&gt;\s?/.test(lines[i])
    ) { para.push(lines[i]); i++; }
    html += '<p>' + inlineMd(para.join('<br/>')) + '</p>';
  }
  closeList();
  return html;
}

// ---- Boot -------------------------------------------------------------------
(async function init() {
  settings = await api.getSettings();
  applyTheme();
  updateThinkBtn();
  await refreshModels();
  refreshSkills();
  bindUI();
  bindStream();
  renderEmpty();
  autoGrow();
})();

function updateThinkBtn() {
  const on = !!settings.thinking;
  els.think.classList.toggle('active', on);
  els.think.title = on ? 'Thinking mode: ON' : 'Thinking mode: OFF';
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
  if (!r.ok) toast('Ollama not reachable — is it running?');
}

// ---- Skills -----------------------------------------------------------------
// Populate the titlebar picker (+ the Settings copy) from settings.skills.
// '' = None / General (base prompt only).
function refreshSkills() {
  const skills = settings.skills || [];
  const sel = els.skill;
  sel.innerHTML = '';
  const none = document.createElement('option');
  none.value = '';
  none.textContent = 'General';
  sel.appendChild(none);
  for (const sk of skills) {
    const o = document.createElement('option');
    o.value = sk.id;
    o.textContent = sk.name;
    sel.appendChild(o);
  }
  sel.value = settings.activeSkill || '';
  updateSkillUI();
  renderSkillList();
}

function updateSkillUI() {
  els.skill.classList.toggle('active', !!settings.activeSkill);
  els.skill.value = settings.activeSkill || '';
}

function activeSkillObj() {
  if (!settings.activeSkill) return null;
  return (settings.skills || []).find((s) => s.id === settings.activeSkill) || null;
}

// Base prompt always applies; the active skill's prompt is layered beneath it.
function composeSystemPrompt() {
  const base = settings.systemPrompt || '';
  const sk = activeSkillObj();
  if (!sk) return base;
  return base + '\n\n--- Active skill: ' + sk.name + ' ---\n' + sk.prompt;
}

// Persist the skills array, then refresh both the picker and the editor.
async function saveSkills(skills) {
  settings = await api.setSettings({ skills });
  refreshSkills();
}

let skillSlug = 0;
function makeSkillId(name) {
  const base = (name || 'skill').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  return (base || 'skill') + '-' + (++skillSlug) + '-' + (settings.skills || []).length;
}

// Build the editable list inside Settings.
function renderSkillList() {
  const wrap = $('skillList');
  if (!wrap) return;
  wrap.innerHTML = '';
  const skills = settings.skills || [];
  if (!skills.length) {
    const e = document.createElement('div');
    e.className = 'muted small';
    e.textContent = 'No skills yet. Add one to specialize Orbit.';
    wrap.appendChild(e);
    return;
  }
  for (const sk of skills) {
    const item = document.createElement('div');
    item.className = 'skill-item' + (settings.activeSkill === sk.id ? ' is-active' : '');

    const head = document.createElement('div');
    head.className = 'skill-item-head';

    const dot = document.createElement('span');
    dot.className = 'skill-active-dot';
    dot.title = 'Active skill indicator';

    const name = document.createElement('input');
    name.className = 'skill-name';
    name.type = 'text';
    name.value = sk.name;
    name.onchange = () => updateSkill(sk.id, { name: name.value.trim() || 'Untitled' });

    const del = document.createElement('button');
    del.className = 'skill-del';
    del.title = 'Delete skill';
    del.innerHTML = icon('i-trash');
    let confirmTimer = null;
    const resetDel = () => {
      del.classList.remove('confirm');
      del.innerHTML = icon('i-trash');
      del.title = 'Delete skill';
      clearTimeout(confirmTimer);
      confirmTimer = null;
    };
    del.onclick = () => {
      if (del.classList.contains('confirm')) { resetDel(); deleteSkill(sk.id); return; }
      // arm: ask for a second click to confirm
      del.classList.add('confirm');
      del.textContent = 'Delete?';
      del.title = 'Click again to confirm';
      confirmTimer = setTimeout(resetDel, 3000);
    };

    head.appendChild(dot);
    head.appendChild(name);
    head.appendChild(del);

    const prompt = document.createElement('textarea');
    prompt.className = 'skill-prompt';
    prompt.value = sk.prompt;
    prompt.placeholder = 'What should Orbit do in this skill?';
    prompt.onchange = () => updateSkill(sk.id, { prompt: prompt.value });

    item.appendChild(head);
    item.appendChild(prompt);
    wrap.appendChild(item);
  }
}

function updateSkill(id, patch) {
  const skills = (settings.skills || []).map((s) => (s.id === id ? { ...s, ...patch } : s));
  saveSkills(skills);
}

async function deleteSkill(id) {
  const skills = (settings.skills || []).filter((s) => s.id !== id);
  // if we deleted the active skill, fall back to General
  const patch = { skills };
  if (settings.activeSkill === id) patch.activeSkill = '';
  settings = await api.setSettings(patch);
  refreshSkills();
  toast('Skill deleted');
}

async function addSkill() {
  const skills = [...(settings.skills || [])];
  const id = makeSkillId('new skill');
  skills.push({ id, name: 'New skill', prompt: '' });
  await saveSkills(skills);
  // focus the new skill's name field for immediate editing
  const last = $('skillList').querySelector('.skill-item:last-child .skill-name');
  if (last) { last.focus(); last.select(); }
}

// ---- UI events --------------------------------------------------------------
function bindUI() {
  els.send.onclick = sendMessage;
  els.stop.onclick = () => api.stopChat();
  els.mic.onclick = toggleDictation;

  // macOS-style window controls (top-left) — each does something distinct
  els.tlClose.onclick = () => api.hide();                  // red: hide overlay
  els.tlMin.onclick = async () => {                        // yellow: minimize to strip
    const r = await api.compact();
    document.body.classList.toggle('compact', !!(r && r.compact));
  };
  els.tlZoom.onclick = () => api.widen();                  // green: widen a step

  // Sidebar / chat history
  els.sidebarBtn.onclick = toggleSidebar;
  els.sidebarScrim.onclick = closeSidebar;
  els.newChatBtn.onclick = () => { newChat(); closeSidebar(); };

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

  els.skill.onchange = async () => {
    settings = await api.setSettings({ activeSkill: els.skill.value });
    updateSkillUI();
    const sk = activeSkillObj();
    toast(sk ? 'Skill: ' + sk.name : 'Skill off — general mode');
  };

  els.think.onclick = async () => {
    settings = await api.setSettings({ thinking: !settings.thinking });
    updateThinkBtn();
    toast(settings.thinking ? 'Thinking mode on' : 'Thinking mode off');
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

  const messages = [{ role: 'system', content: composeSystemPrompt() }, ...history];
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
      if (res && res.ok === false) toast(res.error || 'Request failed');
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
    cur.contentEl.innerHTML = renderMarkdown(cur.content);
    scroll();
  });
  api.onDone(() => {
    if (cur) history.push({ role: 'assistant', content: cur.content });
    finishStream();
    persistCurrentChat();
  });
  api.onError((m) => {
    if (cur) {
      cur.contentEl.innerHTML = renderMarkdown((cur.content || '') + '\n\n— ' + m);
      if (cur.content) history.push({ role: 'assistant', content: cur.content });
    }
    finishStream();
    persistCurrentChat();
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
  if (cur && cur.contentEl) cur.contentEl.classList.remove('streaming');
  cur = null;
}

function startAssistant() {
  removeEmpty();
  const wrap = document.createElement('div');
  wrap.className = 'msg assistant';
  wrap.innerHTML = '<div class="role">orbit</div>';

  const think = document.createElement('details');
  think.className = 'think hidden';
  think.innerHTML = '<summary>' + icon('i-spark', 'tk-ic') + 'Thinking…</summary>';
  const thinkBody = document.createElement('div');
  thinkBody.className = 'think-body';
  think.appendChild(thinkBody);

  const bubble = document.createElement('div');
  bubble.className = 'bubble streaming';
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
  newChat();
}
function renderEmpty() {
  if (els.messages.children.length) return;
  const e = document.createElement('div');
  e.className = 'empty';
  const sk = activeSkillObj();
  const line = sk
    ? '<b>' + sk.name + '</b> skill · on-device with <b>' + (settings.model || 'your model') + '</b>.'
    : 'Running fully on-device with <b>' + (settings.model || 'your model') + '</b>.';
  e.innerHTML =
    icon('i-orbit', 'mark') +
    '<div class="title">Ask Orbit anything</div>' + line;
  els.messages.appendChild(e);
}
function removeEmpty() {
  const e = els.messages.querySelector('.empty');
  if (e) e.remove();
}
function scroll() {
  els.messages.scrollTop = els.messages.scrollHeight;
}

// ---- Sidebar / chat history -------------------------------------------------
function toggleSidebar() {
  if (els.sidebar.classList.contains('closed')) openSidebar();
  else closeSidebar();
}
function openSidebar() {
  els.sidebar.classList.remove('closed');
  els.sidebarScrim.classList.remove('hidden');
  renderChatList();
}
function closeSidebar() {
  els.sidebar.classList.add('closed');
  els.sidebarScrim.classList.add('hidden');
}

// Derive a short title from the first user message.
function titleFrom(msgs) {
  const first = (msgs || []).find((m) => m.role === 'user');
  const t = (first ? first.content : 'New chat').trim().replace(/\s+/g, ' ');
  return t.length > 48 ? t.slice(0, 47) + '…' : t || 'New chat';
}

// Persist the in-memory `history` as a chat. Creates an id on first save.
async function persistCurrentChat() {
  if (!history.length) return; // nothing worth saving
  if (!currentChatId) currentChatId = 'c' + Date.now() + Math.floor(Math.random() * 1000);
  const existing = await api.getChat(currentChatId);
  const chat = {
    id: currentChatId,
    title: existing && existing.titleEdited ? existing.title : titleFrom(history),
    titleEdited: existing ? existing.titleEdited : false,
    messages: history,
    createdAt: existing ? existing.createdAt : Date.now(),
    updatedAt: Date.now()
  };
  await api.saveChat(chat);
}

// Start a fresh, unsaved chat.
function newChat() {
  history = [];
  currentChatId = null;
  els.messages.innerHTML = '';
  renderEmpty();
  els.input.focus();
}

// Load a saved chat into the view.
async function openChat(id) {
  const chat = await api.getChat(id);
  if (!chat) return;
  currentChatId = id;
  history = chat.messages.slice();
  els.messages.innerHTML = '';
  for (const m of history) {
    if (m.role === 'user') addMessage('user', m.content);
    else renderAssistantStatic(m.content);
  }
  if (!history.length) renderEmpty();
  closeSidebar();
  scroll();
}

// Render a finished assistant message (markdown, no streaming caret).
function renderAssistantStatic(content) {
  removeEmpty();
  const wrap = document.createElement('div');
  wrap.className = 'msg assistant';
  wrap.innerHTML = '<div class="role">orbit</div>';
  const bubble = document.createElement('div');
  bubble.className = 'bubble';
  bubble.innerHTML = renderMarkdown(content);
  wrap.appendChild(bubble);
  els.messages.appendChild(wrap);
}

async function renderChatList() {
  const chats = await api.listChats();
  els.chatList.innerHTML = '';
  if (!chats.length) {
    const e = document.createElement('div');
    e.className = 'empty-hist';
    e.textContent = 'No saved chats yet.';
    els.chatList.appendChild(e);
    return;
  }
  for (const c of chats) {
    const row = document.createElement('div');
    row.className = 'chat-row' + (c.id === currentChatId ? ' active' : '');
    row.onclick = (e) => { if (e.target.closest('.chat-del')) return; openChat(c.id); };

    const meta = document.createElement('div');
    meta.className = 'chat-meta';
    const name = document.createElement('div');
    name.className = 'chat-name';
    name.textContent = c.title || 'Untitled';
    const sub = document.createElement('div');
    sub.className = 'chat-sub';
    sub.textContent = relativeTime(c.updatedAt) + ' · ' + c.count + ' msg';
    meta.appendChild(name);
    meta.appendChild(sub);

    const del = document.createElement('button');
    del.className = 'chat-del';
    del.title = 'Delete chat';
    del.innerHTML = icon('i-trash');
    let t = null;
    const reset = () => { del.classList.remove('confirm'); del.innerHTML = icon('i-trash'); clearTimeout(t); t = null; };
    del.onclick = (e) => {
      e.stopPropagation();
      if (del.classList.contains('confirm')) { reset(); removeChat(c.id); return; }
      del.classList.add('confirm');
      del.textContent = 'Delete?';
      t = setTimeout(reset, 3000);
    };

    row.appendChild(meta);
    row.appendChild(del);
    els.chatList.appendChild(row);
  }
}

async function removeChat(id) {
  await api.deleteChat(id);
  if (id === currentChatId) newChat();
  renderChatList();
  toast('Chat deleted');
}

function relativeTime(ts) {
  if (!ts) return '';
  const diff = Date.now() - ts;
  const m = Math.floor(diff / 60000);
  if (m < 1) return 'just now';
  if (m < 60) return m + 'm ago';
  const h = Math.floor(m / 60);
  if (h < 24) return h + 'h ago';
  const d = Math.floor(h / 24);
  if (d < 7) return d + 'd ago';
  return new Date(ts).toLocaleDateString();
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
  renderSkillList();
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
  $('addSkillBtn').onclick = addSkill;

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
    els.listen.classList.add('active');
    els.listen.title = 'Starting…';
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
      els.listen.classList.remove('active');
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
    toast('Listening — say something');
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
  if (status === 'loading') els.listen.title = 'Loading transcriber…';
  else if (status === 'ready') updateListenUI();
  else if (status === 'crashed') {
    toast('Transcriber crashed (is mlx-whisper installed? run setup)');
    listening = false;
    cleanupAudio();
    updateListenUI();
  } else if (typeof status === 'string' && status.startsWith('error')) {
    toast(String(status));
  }
}

function updateListenUI() {
  els.listen.classList.toggle('active', listening);
  els.listen.title = listening ? 'Listening to call audio — click to stop' : 'Listen to call / system audio';
  if (!listening && !transcriptText) els.transcript.classList.add('hidden');
}

// ---- Voice dictation (mic -> chat input) ------------------------------------
const DICT_SILENCE_MS = 1200; // auto-stop after this much trailing silence
let dictating = false;
let dictCtx = null;
let dictProc = null;
let dictSrc = null;
let dictStream = null;
let dictBuf = [];
let dictSpoke = false;
let dictSilence = 0;

async function toggleDictation() {
  if (dictating) return stopDictation(true);
  return startDictation();
}

async function startDictation() {
  try {
    els.mic.classList.add('active');
    els.mic.classList.remove('busy');
    await api.audioStart(); // ensure whisper sidecar is up

    dictStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    dictCtx = new AudioContext({ sampleRate: SR });
    dictProc = dictCtx.createScriptProcessor(4096, 1, 1);
    dictSrc = dictCtx.createMediaStreamSource(dictStream);
    dictSrc.connect(dictProc);
    const mute = dictCtx.createGain();
    mute.gain.value = 0;
    dictProc.connect(mute);
    mute.connect(dictCtx.destination);

    dictBuf = [];
    dictSpoke = false;
    dictSilence = 0;
    dictProc.onaudioprocess = onDictAudio;
    dictating = true;
    toast('Speak now — pause when done');
  } catch (e) {
    await teardownDictation();
    els.mic.classList.remove('active', 'busy');
    const perm = await api.permStatus();
    if (perm.mic === 'denied') {
      await api.openPerm('mic');
      toast('Enable Microphone for Orbit in System Settings, then try again.');
    } else {
      toast('Mic error: ' + (e.message || e));
    }
  }
}

function onDictAudio(e) {
  const input = e.inputBuffer.getChannelData(0);
  let sum = 0;
  for (let i = 0; i < input.length; i++) sum += input[i] * input[i];
  const rms = Math.sqrt(sum / input.length);
  const frameMs = (input.length / SR) * 1000;
  dictBuf.push(new Float32Array(input));
  if (rms > THRESH) {
    dictSpoke = true;
    dictSilence = 0;
  } else if (dictSpoke) {
    dictSilence += frameMs;
    if (dictSilence > DICT_SILENCE_MS) stopDictation(true);
  }
}

async function teardownDictation() {
  if (dictProc) {
    dictProc.onaudioprocess = null;
    dictProc.disconnect();
    dictProc = null;
  }
  if (dictSrc) {
    dictSrc.disconnect();
    dictSrc = null;
  }
  if (dictStream) {
    dictStream.getTracks().forEach((t) => t.stop());
    dictStream = null;
  }
  if (dictCtx) {
    try { await dictCtx.close(); } catch {}
    dictCtx = null;
  }
}

async function stopDictation(transcribe) {
  if (!dictating) return;
  dictating = false;
  const buf = dictBuf;
  const spoke = dictSpoke;
  dictBuf = [];
  await teardownDictation();
  els.mic.classList.remove('active');

  const total = buf.reduce((a, b) => a + b.length, 0);
  if (!transcribe || !spoke || total < SR * 0.3) {
    els.mic.classList.remove('busy');
    return;
  }
  const merged = new Float32Array(total);
  let o = 0;
  for (const c of buf) {
    merged.set(c, o);
    o += c.length;
  }
  els.mic.classList.add('busy');
  const r = await api.transcribe(merged.buffer);
  els.mic.classList.remove('busy');
  if (r && r.text) {
    els.input.value = (els.input.value ? els.input.value.trim() + ' ' : '') + r.text;
    autoGrow();
    els.input.focus();
  } else {
    toast("Didn't catch that — try again");
  }
}

// ---- Toast ------------------------------------------------------------------
let toastTimer = null;
function toast(msg) {
  els.toast.textContent = msg;
  els.toast.classList.remove('hidden');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => els.toast.classList.add('hidden'), 2600);
}
