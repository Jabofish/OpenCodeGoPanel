import { escapeHtml } from './format.js';
import { buildHealthCheckStatus, buildLocalDataStatus } from './settings-diagnostics.js';

export function renderSettingsTab(snapshot, settings, actions, isPinned, localDataStatus, healthCheck) {
  const container = document.getElementById('tab-settings');
  if (!container) return;

  const ws = (snapshot?.workspaces || []).find(w => w.id === snapshot?.workspace_id);
  const workspace = ws?.name || snapshot?.workspace_id || 'Not set';
  const profile = (settings.workspaceProfiles || {})[snapshot?.workspace_id] || {};
  const taskbarStatus = 'Hidden';
  const effectiveHealthCheck = healthCheck || snapshot?.healthCheck || localDataStatus?.healthCheck || null;

  container.innerHTML = '' +
    '<div class="settings-group">' +
      '<div class="settings-title">Window</div>' +
      buildToggle('setting-pin', 'Always on top', isPinned) +
      buildToggle('setting-mini-badge', 'Mini badge mode', settings.miniBadgeMode) +
      buildSelect('setting-mini-badge-source', 'Mini badge source', settings.miniBadgeSource, [
        { value: 'auto', label: 'Auto (max of all)' },
        { value: 'rolling', label: 'Rolling' },
        { value: 'weekly', label: 'Weekly' },
        { value: 'monthly', label: 'Monthly' }
      ]) +
      buildSelect('setting-mini-badge-display', 'Badge display', settings.miniBadgeDisplay || 'percent', [
        { value: 'percent', label: 'Percent' },
        { value: 'ring', label: 'Ring' },
        { value: 'dot', label: 'Dot' },
      ]) +
      buildStatus('Taskbar icon', taskbarStatus) +
      buildAction('setting-minimize', 'Minimize to taskbar') +
      buildAction('setting-hide-to-tray', 'Hide to tray') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Refresh</div>' +
      buildToggle('setting-auto-refresh', 'Auto refresh', settings.autoRefresh) +
      buildToggle('setting-compact', 'Compact layout', settings.compactMode) +
      buildSelect('setting-refresh-visible', 'Visible refresh', String(settings.refreshVisibleSecs || 30), [
        { value: '15', label: '15s' }, { value: '30', label: '30s' },
        { value: '60', label: '60s' }, { value: '300', label: '5m' },
      ]) +
      buildSelect('setting-refresh-hidden', 'Hidden refresh', String(settings.refreshHiddenSecs || 600), [
        { value: '300', label: '5m' }, { value: '600', label: '10m' },
        { value: '1800', label: '30m' }, { value: '0', label: 'Off' },
      ]) +
      buildAction('setting-refresh', 'Refresh now') +
      buildAction('setting-clear-cache', 'Clear cache') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Budget</div>' +
      buildInput('setting-budget', 'Monthly budget (USD)', settings.monthlyBudget ? (settings.monthlyBudget / 100).toFixed(2) : '', 'number', '0.00') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Alerts</div>' +
      buildInput('setting-threshold', 'Usage threshold (%, 0=off)', settings.usageThreshold || 0, 'number', '50-95 or 0') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Notifications</div>' +
      buildToggle('setting-notify-quota', 'Quota alerts', settings.notifyQuota !== false) +
      buildToggle('setting-notify-budget', 'Budget exceeded', settings.notifyBudgetProjection !== false) +
      buildToggle('setting-notify-refresh-failure', 'Refresh failures', settings.notifyRefreshFailure !== false) +
      buildToggle('setting-quiet-hours', 'Quiet hours (22:00-08:00)', settings.quietHoursEnabled) +
      buildAction('setting-test-notification', 'Test notification') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Hotkey</div>' +
      buildStatus('Current', escapeHtml(settings.hotkeyRecording ? 'Press shortcut...' : (settings.hotkey || 'Ctrl+Shift+U'))) +
      buildAction('setting-hotkey-record', settings.hotkeyRecording ? 'Recording...' : 'Record shortcut') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Account</div>' +
      buildStatus('Workspace', escapeHtml(workspace)) +
      buildStatus('Workspace ID', escapeHtml(snapshot?.workspace_id || 'Not set')) +
      buildAction('setting-rename-workspace', 'Rename workspace') +
      buildAction('setting-favorite-workspace', profile.favorite ? '★ Unfavorite' : '☆ Favorite') +
      buildAction('setting-login', 'Log in') +
      buildAction('setting-clear-auth', 'Clear login') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Local Data & Health</div>' +
      '<div class="settings-diagnostics">' +
        buildLocalDataStatus(localDataStatus) +
        buildHealthCheckStatus(effectiveHealthCheck, snapshot) +
        '<div class="settings-diagnostic-actions">' +
          buildAction('setting-refresh-local-data', 'Refresh data status', !!actions?.refreshLocalDataStatus, 'settings-diagnostic-action') +
          buildAction('setting-run-health-check', 'Run health check', !!actions?.runHealthCheck, 'settings-diagnostic-action') +
          buildAction('setting-open-exports', 'Open exports folder', true, 'settings-diagnostic-action') +
          buildAction('setting-backup', 'Backup settings/history', true, 'settings-diagnostic-action') +
          buildAction('setting-export-json', 'Export snapshot JSON', true, 'settings-diagnostic-action') +
          buildAction('setting-export-records', 'Export usage CSV', true, 'settings-diagnostic-action') +
          buildAction('setting-export-costs', 'Export costs CSV', true, 'settings-diagnostic-action') +
          buildAction('setting-clear-exports', 'Clear exports', true, 'settings-diagnostic-action danger') +
          buildAction('setting-clear-history', 'Clear history', true, 'settings-diagnostic-action danger') +
        '</div>' +
      '</div>' +
    '</div>';

  bindToggle('setting-pin', (value) => actions.setPinned(value));
  bindToggle('setting-auto-refresh', (value) => actions.setAutoRefresh(value));
  bindToggle('setting-compact', (value) => actions.setCompactMode(value));
  bindToggle('setting-mini-badge', (value) => actions.setMiniBadgeMode(value));
  bindToggle('setting-notify-quota', (value) => actions.setNotifyQuota(value));
  bindToggle('setting-notify-budget', (value) => actions.setNotifyBudgetProjection(value));
  bindToggle('setting-notify-refresh-failure', (value) => actions.setNotifyRefreshFailure(value));
  bindToggle('setting-quiet-hours', (value) => actions.setQuietHoursEnabled(value));
  bindSelect('setting-mini-badge-source', (value) => actions.setMiniBadgeSource(value));
  bindSelect('setting-mini-badge-display', (value) => { if (actions.setMiniBadgeDisplay) actions.setMiniBadgeDisplay(value); });
  bindSelect('setting-refresh-visible', (value) => actions.setRefreshVisibleSecs(parseInt(value, 10)));
  bindSelect('setting-refresh-hidden', (value) => actions.setRefreshHiddenSecs(parseInt(value, 10)));
  bindAction('setting-refresh', actions.refresh);
  bindAction('setting-clear-cache', actions.clearCache);
  bindAction('setting-login', actions.login);
  bindAction('setting-clear-auth', actions.clearAuth);
  bindAction('setting-minimize', actions.minimize);
  bindAction('setting-hide-to-tray', actions.hideToTray);
  bindAction('setting-hotkey-record', actions.recordHotkey);
  bindAction('setting-test-notification', actions.sendTestNotification);
  bindAction('setting-export-json', () => actions.exportData('snapshot-json'));
  bindAction('setting-export-records', () => actions.exportData('usage-records-csv'));
  bindAction('setting-export-costs', () => actions.exportData('daily-costs-csv'));
  bindAction('setting-refresh-local-data', actions?.refreshLocalDataStatus);
  bindAction('setting-run-health-check', actions?.runHealthCheck);
  bindAction('setting-open-exports', actions.openExportsFolder);
  bindAction('setting-backup', actions.backupLocalData);
  bindAction('setting-clear-exports', () => actions.clearLocalData('exports'));
  bindAction('setting-clear-history', () => actions.clearLocalData('history'));
  bindAction('setting-rename-workspace', actions.renameWorkspace);
  bindAction('setting-favorite-workspace', actions.toggleFavoriteWorkspace);
  bindInput('setting-budget', (value) => {
    const cents = Math.round(parseFloat(value || '0') * 100);
    actions.setBudget(cents >= 0 ? cents : 0);
  });
  bindInput('setting-threshold', (value) => {
    const v = parseInt(value || '0', 10);
    actions.setThreshold(v >= 50 && v <= 95 ? v : 0);
  });
}

function buildToggle(id, label, checked) {
  return '<label class="setting-row setting-toggle">' +
    '<span>' + escapeHtml(label) + '</span>' +
    '<input id="' + id + '" type="checkbox"' + (checked ? ' checked' : '') + '>' +
    '<span class="switch"></span></label>';
}
function buildStatus(label, value) {
  return '<div class="setting-row"><span>' + escapeHtml(label) + '</span><strong>' + value + '</strong></div>';
}
function buildAction(id, label, enabled = true, className = 'setting-action') {
  return '<button id="' + id + '" type="button" class="' + escapeHtml(className) + '"' +
    (enabled ? '' : ' disabled aria-disabled="true"') +
    '><span>' + escapeHtml(label) + '</span><span class="setting-arrow">></span></button>';
}
function buildInput(id, label, value, type, placeholder) {
  const rowClass = 'setting-row setting-input-row setting-input-row-' + id;
  if (type === 'number') {
    const step = id === 'setting-budget' ? '0.01' : '1';
    return '<label class="' + rowClass + '"><span>' + escapeHtml(label) + '</span>' +
      '<span class="number-control">' +
      '<input id="' + id + '" type="number" value="' + escapeHtml(value || '') + '" min="0" step="' + step + '"' +
      (placeholder ? ' placeholder="' + escapeHtml(placeholder) + '"' : '') + ' class="setting-input">' +
      '<span class="stepper-buttons">' +
      '<button type="button" class="stepper-btn" data-target="' + id + '" data-dir="up">+</button>' +
      '<button type="button" class="stepper-btn" data-target="' + id + '" data-dir="down">-</button>' +
      '</span></span></label>';
  }
  return '<label class="' + rowClass + '"><span>' + escapeHtml(label) + '</span>' +
    '<input id="' + id + '" type="' + (type || 'text') + '" value="' + escapeHtml(value || '') + '"' +
    (placeholder ? ' placeholder="' + escapeHtml(placeholder) + '"' : '') + ' class="setting-input"></label>';
}
function buildSelect(id, label, value, options) {
  const opts = options.map(o =>
    '<option value="' + escapeHtml(o.value) + '"' + (o.value === value ? ' selected' : '') + '>' + escapeHtml(o.label) + '</option>'
  ).join('');
  return '<label class="setting-row setting-select-row"><span>' + escapeHtml(label) + '</span>' +
    '<select id="' + id + '" class="setting-input">' + opts + '</select></label>';
}
function bindToggle(id, handler) {
  const el = document.getElementById(id);
  if (!el) return;
  el.addEventListener('change', (e) => {
    handler(e.target.checked);
    // Blur to prevent WebView2 focus-triggered viewport scroll
    el.blur();
  });
}
function bindAction(id, handler) {
  const el = document.getElementById(id);
  if (!el || typeof handler !== 'function') return;
  el.addEventListener('click', () => {
    handler();
    // Blur to prevent WebView2 focus-triggered viewport scroll
    el.blur();
  });
}
function bindInput(id, handler) {
  const input = document.getElementById(id);
  input?.addEventListener('change', (e) => handler(e.target.value));
  document.querySelectorAll('.stepper-btn[data-target="' + id + '"]').forEach((btn) => {
    btn.addEventListener('click', () => { if (!input) return; input.value = nextNumberValue(id, input.value, btn.dataset.dir); handler(input.value); });
  });
}
function bindSelect(id, handler) { document.getElementById(id)?.addEventListener('change', (e) => handler(e.target.value)); }
function nextNumberValue(id, raw, dir) {
  const up = dir === 'up';
  const current = parseFloat(raw || '0') || 0;
  if (id === 'setting-threshold') {
    if (up) return String(Math.min(current <= 0 ? 50 : current + 1, 95));
    return String(current <= 50 ? 0 : current - 1);
  }
  return Math.max(0, current + (up ? 1 : -1)).toFixed(2);
}
