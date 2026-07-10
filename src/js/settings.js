import { escapeHtml } from './format.js';
import { buildHealthCheckStatus, buildLocalDataStatus } from './settings-diagnostics.js';

export function renderSettingsTab(snapshot, settings, actions, isPinned, localDataStatus, healthCheck, advancedOpen = false, accounts = [], inlineAction = null) {
  const container = document.getElementById('tab-settings');
  if (!container) return;

  const ws = (snapshot?.workspaces || []).find(w => w.id === snapshot?.workspace_id);
  const workspace = ws?.name || snapshot?.workspace_id || 'Not set';
  const profile = (settings.workspaceProfiles || {})[snapshot?.workspace_id] || {};
  const taskbarStatus = 'Hidden';
  const effectiveHealthCheck = healthCheck || snapshot?.healthCheck || localDataStatus?.healthCheck || null;
  const healthCheckOpen = !!actions?.isHealthCheckOpen?.();
  const localBackups = typeof actions?.getLocalBackups === 'function' ? actions.getLocalBackups() : [];
  const localBackupsLoaded = !!actions?.hasLocalBackupsLoaded?.();
  const localBackupsOpen = !!actions?.isLocalBackupsOpen?.();
  const updateActionLabel = actions?.hasDownloadedUpdate?.() ? 'Install downloaded update' : 'Check for updates now';

  // Less-frequently used groups are tucked into an "Advanced settings"
  // collapsible, closed by default.
  const advancedHtml =
    '<div class="settings-group">' +
      '<div class="settings-title">Window</div>' +
      buildSelect('setting-theme', 'Theme', settings.theme || 'system', [
        { value: 'dark', label: 'Dark' },
        { value: 'light', label: 'Light' },
        { value: 'system', label: 'Follow system' },
      ]) +
      buildToggle('setting-pin', 'Always on top', isPinned) +
      buildToggle('setting-autostart', 'Launch on startup', settings.launchOnStartup) +
      buildStatus('Taskbar icon', taskbarStatus) +
      buildAction('setting-minimize', 'Minimize to taskbar') +
      buildAction('setting-hide-to-tray', 'Hide to tray') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Mini Badge</div>' +
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
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Refresh Tuning</div>' +
      buildToggle('setting-compact', 'Compact layout', settings.compactMode) +
      buildSelect('setting-refresh-visible', 'Visible refresh', String(settings.refreshVisibleSecs || 30), [
        { value: '15', label: '15s' }, { value: '30', label: '30s' },
        { value: '60', label: '60s' }, { value: '300', label: '5m' },
      ]) +
      buildSelect('setting-refresh-hidden', 'Hidden refresh', String(settings.refreshHiddenSecs || 600), [
        { value: '300', label: '5m' }, { value: '600', label: '10m' },
        { value: '1800', label: '30m' }, { value: '0', label: 'Off' },
      ]) +
      buildAction('setting-clear-cache', 'Clear cache') +
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
      '<div class="settings-title">Accounts</div>' +
      buildAccountsListHtml(accounts, settings) +
      buildAction('setting-add-account', 'Add account') +
      (isAccountInlineAction(inlineAction) ? buildInlineAccountAction(inlineAction) : '') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Hotkey</div>' +
      buildStatus('Current', escapeHtml(settings.hotkeyRecording ? 'Press shortcut...' : (settings.hotkey || 'Ctrl+Shift+U'))) +
      buildAction('setting-hotkey-record', settings.hotkeyRecording ? 'Recording...' : 'Record shortcut') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Reports</div>' +
      buildSelect('setting-report-frequency', 'Report frequency', settings.reportFrequency || 'off', [
        { value: 'off', label: 'Off' },
        { value: 'daily', label: 'Daily' },
        { value: 'weekly', label: 'Weekly' },
        { value: 'monthly', label: 'Monthly' },
      ]) +
      buildToggle('setting-report-auto', 'Auto generate', settings.reportAutoGenerate) +
      buildAction('setting-generate-report', 'Generate report now') +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Updates</div>' +
      buildToggle('setting-auto-update', 'Auto check updates', settings.autoUpdate !== false) +
      buildAction('setting-check-update', updateActionLabel) +
    '</div>' +
    '<div class="settings-group">' +
      '<div class="settings-title">Local Data & Health</div>' +
      buildToggle('setting-auto-backup', 'Auto backup (daily, keep 7)', settings.autoBackup !== false) +
      '<div class="settings-diagnostics">' +
        buildLocalDataStatus(localDataStatus) +
        buildHealthCheckBrowser(effectiveHealthCheck, snapshot, healthCheckOpen) +
        buildBackupBrowser(localBackups, localBackupsOpen, localDataStatus?.backupCount, localBackupsLoaded) +
        '<div class="settings-diagnostic-actions">' +
          buildAction('setting-refresh-local-data', 'Refresh data status', !!actions?.refreshLocalDataStatus, 'settings-diagnostic-action') +
          buildAction('setting-run-health-check', 'Run health check', !!actions?.runHealthCheck, 'settings-diagnostic-action') +
          buildAction('setting-refresh-backups', 'Refresh backups', !!actions?.refreshLocalBackups, 'settings-diagnostic-action') +
          buildAction('setting-open-exports', 'Open exports folder', true, 'settings-diagnostic-action') +
          buildAction('setting-backup', 'Backup settings/history', true, 'settings-diagnostic-action') +
          buildAction('setting-restore', 'Restore JSON file', true, 'settings-diagnostic-action danger') +
          buildAction('setting-export-json', 'Export snapshot JSON', true, 'settings-diagnostic-action') +
          buildAction('setting-export-records', 'Export usage CSV', true, 'settings-diagnostic-action') +
          buildAction('setting-export-costs', 'Export costs CSV', true, 'settings-diagnostic-action') +
          buildAction('setting-clear-cache-data', 'Clear cache', true, 'settings-diagnostic-action danger') +
          buildAction('setting-clear-auth-data', 'Clear login', true, 'settings-diagnostic-action danger') +
          buildAction('setting-clear-exports', 'Clear exports', true, 'settings-diagnostic-action danger') +
          buildAction('setting-clear-history', 'Clear history', true, 'settings-diagnostic-action danger') +
        '</div>' +
        (inlineAction?.kind === 'clear-local-data' ? buildInlineConfirm(inlineAction, 'Clear ' + inlineAction.scope + ' data?') : '') +
        (inlineAction?.kind === 'restore-local-data' ? buildRestoreConfirm(inlineAction) : '') +
      '</div>' +
    '</div>';

  container.innerHTML =
    '<div class="settings-group">' +
      '<div class="settings-title">Refresh</div>' +
      buildToggle('setting-auto-refresh', 'Auto refresh', settings.autoRefresh) +
      buildAction('setting-refresh', 'Refresh now') +
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
      '<div class="settings-title">Account</div>' +
      buildStatus('Workspace', escapeHtml(workspace)) +
      buildStatus('Workspace ID', escapeHtml(snapshot?.workspace_id || 'Not set')) +
      buildAction('setting-rename-workspace', 'Rename workspace') +
      (inlineAction?.kind === 'rename-workspace' ? buildInlineInput(inlineAction, 'Workspace alias', 'Empty clears alias') : '') +
      buildAction('setting-favorite-workspace', profile.favorite ? '★ Unfavorite' : '☆ Favorite') +
      buildAction('setting-login', 'Log in') +
      buildAction('setting-clear-auth', 'Clear login') +
    '</div>' +
    buildCollapsibleSection('setting-advanced', 'Advanced settings', advancedOpen, advancedHtml);

  bindAction('setting-advanced-toggle', actions.toggleAdvanced);
  bindToggle('setting-pin', (value) => actions.setPinned(value));
  bindToggle('setting-autostart', (value) => actions.setLaunchOnStartup(value));
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
  bindAction('setting-health-check-toggle', actions?.toggleHealthCheck);
  bindAction('setting-backups-toggle', actions?.toggleLocalBackups);
  bindAction('setting-refresh-backups', actions?.refreshLocalBackups);
  bindAction('setting-open-exports', actions.openExportsFolder);
  bindAction('setting-backup', actions.backupLocalData);
  bindAction('setting-restore', actions.restoreLocalData);
  bindAction('setting-clear-cache-data', () => actions.clearLocalData('cache'));
  bindAction('setting-clear-auth-data', () => actions.clearLocalData('login'));
  bindAction('setting-clear-exports', () => actions.clearLocalData('exports'));
  bindAction('setting-clear-history', () => actions.clearLocalData('history'));
  bindAction('setting-rename-workspace', actions.renameWorkspace);
  bindAction('setting-favorite-workspace', actions.toggleFavoriteWorkspace);
  bindSelect('setting-theme', (value) => actions.setTheme(value));
  bindSelect('setting-report-frequency', (value) => actions.setReportFrequency(value));
  bindToggle('setting-report-auto', (value) => actions.setReportAutoGenerate(value));
  bindAction('setting-generate-report', () => actions.generateReport(
    settings.reportFrequency === 'off' ? 'daily' : settings.reportFrequency
  ));
  bindToggle('setting-auto-backup', (value) => actions.setAutoBackup(value));
  bindToggle('setting-auto-update', (value) => actions.setAutoUpdate(value));
  bindAction('setting-check-update', actions.checkForUpdate);
  localBackups.forEach((backup, index) => {
    bindAction('setting-restore-backup-' + index, () => actions.restoreListedBackup(backup.id));
  });
  bindAction('setting-add-account', actions.addAccount);
  for (const account of accounts || []) {
    bindAction('setting-rename-account-' + account.id, () => actions.renameAccount(account.id, account.displayName));
    if ((accounts || []).length > 1) {
      bindAction('setting-remove-account-' + account.id, () => actions.removeAccount(account.id, account.displayName));
    }
  }
  bindInlineAction(inlineAction, actions);
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

/**
 * Collapsible section (like the Models "Recent requests" pattern).
 * The toggle button is always rendered; the body is only rendered when open,
 * so closed sections avoid creating bindings for their inner controls.
 */
function buildCollapsibleSection(id, title, open, bodyHtml, className = '') {
  const extraClass = className ? ' ' + className : '';
  return '<div class="settings-collapsible' + extraClass + (open ? ' open' : '') + '">' +
    '<button id="' + id + '-toggle" type="button" class="settings-collapsible-toggle' + extraClass + (open ? ' active' : '') + '">' +
      '<span class="settings-collapsible-arrow">' + (open ? '&#x25BC;' : '&#x25B6;') + '</span>' +
      '<span>' + escapeHtml(title) + '</span>' +
    '</button>' +
    (open ? '<div class="settings-collapsible-body">' + bodyHtml + '</div>' : '') +
  '</div>';
}
function buildStatus(label, value) {
  return '<div class="setting-row"><span>' + escapeHtml(label) + '</span><strong>' + value + '</strong></div>';
}
function buildHealthCheckBrowser(health, snapshot, open) {
  const state = healthCheckState(health, snapshot);
  return buildCollapsibleSection(
    'setting-health-check',
    'Health check (' + healthCheckSummary(health, snapshot) + ')',
    open,
    buildHealthCheckStatus(health, snapshot),
    'health-' + state
  );
}
function buildBackupBrowser(backups, open, statusBackupCount, loaded) {
  const list = Array.isArray(backups) ? backups : [];
  const count = loaded
    ? list.length
    : (Number.isFinite(statusBackupCount) ? statusBackupCount : list.length);
  if (list.length === 0) {
    return buildCollapsibleSection(
      'setting-backups',
      'Backups (' + count + ')',
      open,
      '<div class="settings-backup-list empty">No backups found</div>'
    );
  }
  const rows = list.map((backup, index) => {
    const created = formatBackupDate(backup.createdAt || backup.modifiedAt);
    const meta = [
      backup.source || 'Backup',
      created,
      (backup.historyEntries || 0) + ' history',
      backup.workspaceId || 'no workspace',
    ].filter(Boolean).join(' - ');
    return '<div class="settings-backup-row">' +
      '<div class="settings-backup-main">' +
        '<span class="settings-backup-name">' + escapeHtml(backup.fileName || backup.id || 'backup.json') + '</span>' +
        '<span class="settings-backup-meta">' + escapeHtml(meta) + '</span>' +
      '</div>' +
      buildAction('setting-restore-backup-' + index, 'Restore', true, 'settings-backup-action danger') +
    '</div>';
  }).join('');
  return buildCollapsibleSection(
    'setting-backups',
    'Backups (' + count + ')',
    open,
    '<div class="settings-backup-list">' + rows + '</div>'
  );
}
function healthCheckSummary(health, snapshot) {
  if (!health) {
    const lastError = snapshot?.refresh_state?.last_error || snapshot?.error || '';
    return lastError ? 'Check needed' : 'Not run';
  }
  const checks = [
    health.hasAuth,
    health.cacheOk,
    health.settingsOk,
    health.historyOk,
    health.dataDirAvailable,
  ];
  const failed = checks.filter(value => value === false).length;
  return failed === 0 ? 'OK' : failed + ' issue' + (failed === 1 ? '' : 's');
}
function healthCheckState(health, snapshot) {
  if (!health) {
    const lastError = snapshot?.refresh_state?.last_error || snapshot?.error || '';
    return lastError ? 'warning' : 'unknown';
  }
  const checks = [
    health.hasAuth,
    health.cacheOk,
    health.settingsOk,
    health.historyOk,
    health.dataDirAvailable,
  ];
  return checks.some(value => value === false) ? 'warning' : 'ok';
}
function buildAccountsListHtml(accounts, settings) {
  const list = accounts || [];
  if (list.length === 0) {
    return buildStatus('Accounts', 'None');
  }
  const activeId = settings.activeAccountId || '';
  const rows = list.map(account => {
    const isActive = account.id === activeId;
    const name = (isActive ? '* ' : '') + (account.displayName || account.id);
    return '<div class="settings-account-row" data-account-id="' + escapeHtml(account.id) + '">' +
      '<span class="settings-account-name">' + escapeHtml(name) + '</span>' +
      '<span class="settings-account-actions">' +
        buildAction('setting-rename-account-' + account.id, 'Rename', true, 'settings-account-action') +
        buildAction('setting-remove-account-' + account.id, 'Remove', list.length > 1, 'settings-account-action danger') +
      '</span>' +
    '</div>';
  }).join('');
  return '<div class="settings-account-list">' + rows + '</div>';
}
function isAccountInlineAction(action) {
  return ['add-account', 'rename-account', 'remove-account'].includes(action?.kind);
}
function buildInlineAccountAction(action) {
  if (action.kind === 'remove-account') {
    return buildInlineConfirm(action, 'Remove "' + (action.displayName || action.accountId) + '" and its saved data?');
  }
  return buildInlineInput(action, 'Account name', action.kind === 'add-account' ? 'Optional' : '');
}
function buildInlineInput(action, label, placeholder) {
  return '<div class="settings-inline-action" data-kind="' + escapeHtml(action?.kind || '') + '">' +
    '<label class="settings-inline-label">' +
      '<span>' + escapeHtml(label) + '</span>' +
      '<input id="settings-inline-input" type="text" class="setting-input" value="' + escapeHtml(action?.value || '') + '"' +
        (placeholder ? ' placeholder="' + escapeHtml(placeholder) + '"' : '') + '>' +
    '</label>' +
    '<div class="settings-inline-buttons">' +
      '<button id="settings-inline-submit" type="button" class="settings-inline-btn primary">Save</button>' +
      '<button id="settings-inline-cancel" type="button" class="settings-inline-btn">Cancel</button>' +
    '</div>' +
  '</div>';
}
function buildInlineConfirm(action, message) {
  const confirmText = action.kind === 'remove-account' ? 'Remove' : action.kind === 'restore-local-data' ? 'Restore' : 'Clear';
  return '<div class="settings-inline-action danger" data-kind="' + escapeHtml(action?.kind || '') + '">' +
    '<div class="settings-inline-message">' + escapeHtml(message) + '</div>' +
    '<div class="settings-inline-buttons">' +
      '<button id="settings-inline-submit" type="button" class="settings-inline-btn danger">' + escapeHtml(confirmText) + '</button>' +
      '<button id="settings-inline-cancel" type="button" class="settings-inline-btn">Cancel</button>' +
    '</div>' +
  '</div>';
}
function buildRestoreConfirm(action) {
  const preview = action.preview || {};
  const details = [
    'Version ' + (preview.version || '?'),
    (preview.historyEntries || 0) + ' history entries',
    'workspace ' + (preview.workspaceId || 'not set'),
    'account ' + (preview.activeAccountId || 'default'),
  ];
  if (preview.createdAt) details.unshift(formatBackupDate(preview.createdAt));
  return '<div class="settings-inline-action danger" data-kind="restore-local-data">' +
    '<div class="settings-inline-message">Restore this backup and replace current settings, history, and cache?</div>' +
    '<div class="settings-restore-preview">' + escapeHtml(details.join(' - ')) + '</div>' +
    '<div class="settings-inline-buttons">' +
      '<button id="settings-inline-submit" type="button" class="settings-inline-btn danger">Restore</button>' +
      '<button id="settings-inline-cancel" type="button" class="settings-inline-btn">Cancel</button>' +
    '</div>' +
  '</div>';
}
function bindInlineAction(action, actions) {
  if (!action) return;
  const submit = document.getElementById('settings-inline-submit');
  const cancel = document.getElementById('settings-inline-cancel');
  const input = document.getElementById('settings-inline-input');
  submit?.addEventListener('click', () => actions.submitInlineAction(input ? input.value : ''));
  cancel?.addEventListener('click', () => actions.cancelInlineAction());
  input?.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') actions.submitInlineAction(input.value);
    if (event.key === 'Escape') actions.cancelInlineAction();
  });
  input?.focus();
  input?.select();
}
function buildAction(id, label, enabled = true, className = 'setting-action') {
  return '<button id="' + id + '" type="button" class="' + escapeHtml(className) + '"' +
    (enabled ? '' : ' disabled aria-disabled="true" disabled') +
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
function formatBackupDate(value) {
  if (!value) return 'unknown date';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}
