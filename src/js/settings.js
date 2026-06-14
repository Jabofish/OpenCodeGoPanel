import { escapeHtml } from './format.js';

export function renderSettingsTab(snapshot, settings, actions, isPinned) {
  const container = document.getElementById('tab-settings');
  if (!container) return;

  const workspace = snapshot?.workspace_id || 'Not set';
  const taskbarStatus = 'On';

  container.innerHTML = '' +
    '<div class="settings-group">' +
      '<div class="settings-title">Window</div>' +
      buildToggle('setting-pin', 'Always on top', isPinned) +
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
      '<div class="settings-title">Account</div>' +
      buildStatus('Workspace', escapeHtml(workspace)) +
      buildAction('setting-login', 'Log in') +
      buildAction('setting-clear-auth', 'Clear login') +
    '</div>';

  bindToggle('setting-pin', (value) => actions.setPinned(value));
  bindToggle('setting-auto-refresh', (value) => actions.setAutoRefresh(value));
  bindToggle('setting-compact', (value) => actions.setCompactMode(value));
  bindAction('setting-refresh', actions.refresh);
  bindAction('setting-clear-cache', actions.clearCache);
  bindAction('setting-login', actions.login);
  bindAction('setting-clear-auth', actions.clearAuth);
  bindAction('setting-minimize', actions.minimize);
  bindAction('setting-hide-to-tray', actions.hideToTray);
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
  return '' +
    '<button id="' + id + '" class="setting-action">' +
      '<span>' + escapeHtml(label) + '</span>' +
      '<span class="setting-arrow">></span>' +
    '</button>';
}

function bindToggle(id, handler) {
  document.getElementById(id)?.addEventListener('change', (event) => {
    handler(event.target.checked);
  });
}

function bindAction(id, handler) {
  document.getElementById(id)?.addEventListener('click', () => handler());
}
