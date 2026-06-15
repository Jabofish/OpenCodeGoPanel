import { escapeHtml } from './format.js';

export function renderSettingsTab(snapshot, settings, actions, isPinned) {
  const container = document.getElementById('tab-settings');
  if (!container) return;

  const ws = (snapshot?.workspaces || []).find(w => w.id === snapshot?.workspace_id);
  const workspace = ws?.name || snapshot?.workspace_id || 'Not set';
  const taskbarStatus = 'Hidden';

  container.innerHTML = '' +
    '<div class="settings-group">' +
      '<div class="settings-title">Window</div>' +
      buildToggle('setting-pin', 'Always on top', isPinned) +
      buildToggle('setting-mini-badge', 'Mini badge mode', settings.miniBadgeMode) +
      buildStatus('Taskbar icon', taskbarStatus) +
      buildAction('setting-minimize', 'Minimize to taskbar') +
      buildAction('setting-hide-to-tray', 'Hide to tray') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Refresh</div>' +
      buildToggle('setting-auto-refresh', 'Auto refresh', settings.autoRefresh) +
      buildToggle('setting-compact', 'Compact layout', settings.compactMode) +
      buildAction('setting-refresh', 'Refresh now') +
      buildAction('setting-clear-cache', 'Clear cache') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Budget</div>' +
      buildInput('setting-budget', 'Monthly budget (USD)', settings.monthlyBudget ? (settings.monthlyBudget / 100).toFixed(2) : '', 'number', '0.00') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Alerts</div>' +
      buildInput('setting-threshold', 'Usage alert threshold (%, 0=off)', settings.usageThreshold || 0, 'number', '50-95 or 0') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Hotkey</div>' +
      buildInput('setting-hotkey', 'Toggle panel (Ctrl+Shift+?)', settings.hotkey || 'Ctrl+Shift+U', 'text', 'Ctrl+Shift+U') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Account</div>' +
      buildStatus('Workspace', escapeHtml(workspace)) +
      buildAction('setting-login', 'Log in') + 
      buildAction('setting-clear-auth', 'Clear login') +
    '</div>';

  bindToggle('setting-pin', (value) => actions.setPinned(value));
  bindToggle('setting-auto-refresh', (value) => actions.setAutoRefresh(value));
  bindToggle('setting-compact', (value) => actions.setCompactMode(value));
  bindToggle('setting-mini-badge', (value) => actions.setMiniBadgeMode(value));
  bindAction('setting-refresh', actions.refresh);
  bindAction('setting-clear-cache', actions.clearCache);
  bindAction('setting-login', actions.login);
  bindAction('setting-clear-auth', actions.clearAuth);
  bindAction('setting-minimize', actions.minimize);
  bindAction('setting-hide-to-tray', actions.hideToTray);
  bindInput('setting-budget', (value) => {
    const cents = Math.round(parseFloat(value || '0') * 100);
    actions.setBudget(cents >= 0 ? cents : 0);
  });
  bindInput('setting-hotkey', (value) => {
    if (value && value.trim()) {
      actions.setHotkey(value.trim());
    }
  });
  bindInput('setting-threshold', (value) => {
    const v = parseInt(value || '0', 10);
    actions.setThreshold(v >= 50 && v <= 95 ? v : 0);
  });
}

function buildToggle(id, label, checked) {
  return '' +
    '<label class="setting-row setting-toggle">' +
      '<span>' + escapeHtml(label) + '</span>' +
      '<input id="' + id + '" type="checkbox"' + (checked ? ' checked' : '') + '>' +
      '<span class="switch"></span>' +
    '</label>';
}

function buildStatus(label, value) {
  return '' +
    '<div class="setting-row">' +
      '<span>' + escapeHtml(label) + '</span>' +
      '<strong>' + value + '</strong>' +
    '</div>';
}

function buildAction(id, label) {
  return '<button id="' + id + '" class="setting-action">' +
    '<span>' + escapeHtml(label) + '</span>' +
    '<span class="setting-arrow">></span>' +
  '</button>';
}

function buildInput(id, label, value, type, placeholder) {
  const rowClass = 'setting-row setting-input-row setting-input-row-' + id;
  if (type === 'number') {
    const step = id === 'setting-budget' ? '0.01' : '1';
    return '<label class="' + rowClass + '">' +
      '<span>' + escapeHtml(label) + '</span>' +
      '<span class="number-control">' +
        '<input id="' + id + '" type="number" value="' + escapeHtml(value || '') + '"' +
        ' min="0" step="' + step + '"' +
        (placeholder ? ' placeholder="' + escapeHtml(placeholder) + '"' : '') + ' class="setting-input">' +
        '<span class="stepper-buttons">' +
          '<button type="button" class="stepper-btn" data-target="' + id + '" data-dir="up">+</button>' +
          '<button type="button" class="stepper-btn" data-target="' + id + '" data-dir="down">-</button>' +
        '</span>' +
      '</span>' +
    '</label>';
  }

  return '<label class="' + rowClass + '">' +
    '<span>' + escapeHtml(label) + '</span>' +
    '<input id="' + id + '" type="' + (type || 'text') + '" value="' + escapeHtml(value || '') + '"' +
    (placeholder ? ' placeholder="' + escapeHtml(placeholder) + '"' : '') + ' class="setting-input">' +
  '</label>';
}

function bindToggle(id, handler) {
  document.getElementById(id)?.addEventListener('change', (event) => {
    handler(event.target.checked);
  });
}

function bindAction(id, handler) {
  document.getElementById(id)?.addEventListener('click', () => handler());
}

function bindInput(id, handler) {
  const input = document.getElementById(id);
  input?.addEventListener('change', (event) => {
    handler(event.target.value);
  });
  document.querySelectorAll('.stepper-btn[data-target="' + id + '"]').forEach((button) => {
    button.addEventListener('click', () => {
      if (!input) return;
      input.value = nextNumberValue(id, input.value, button.dataset.dir);
      handler(input.value);
    });
  });
}

function nextNumberValue(id, raw, dir) {
  const up = dir === 'up';
  const current = parseFloat(raw || '0') || 0;

  if (id === 'setting-threshold') {
    if (up) return String(Math.min(current <= 0 ? 50 : current + 1, 95));
    return String(current <= 50 ? 0 : current - 1);
  }

  const next = current + (up ? 1 : -1);
  return Math.max(0, next).toFixed(2);
}
