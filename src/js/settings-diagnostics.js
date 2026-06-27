import { escapeHtml } from './format.js';

export function buildLocalDataStatus(status) {
  if (!status) {
    return '<div class="settings-diagnostics-grid">' +
      buildDiagnosticStat('Local data', 'Not loaded') +
    '</div>';
  }
  const exportCount = status.exportCount || 0;
  const backupCount = status.backupCount || 0;
  return '<div class="settings-diagnostics-grid">' +
      buildDiagnosticStat('Total stored', formatBytes(sumLocalDataBytes(status))) +
      buildDiagnosticStat('Cache', formatBytes(status.cacheBytes || 0)) +
      buildDiagnosticStat('History', formatBytes(status.historyBytes || 0)) +
      buildDiagnosticStat('Settings', formatBytes(status.settingsBytes || 0)) +
      buildDiagnosticStat('Auth', formatBytes(status.authBytes || 0)) +
      buildDiagnosticStat('Exports', formatBytes(status.exportBytes || 0) + ' - ' + exportCount + ' file' + (exportCount === 1 ? '' : 's')) +
      buildDiagnosticStat('Backups', formatBytes(status.backupBytes || 0) + ' - ' + backupCount + ' file' + (backupCount === 1 ? '' : 's')) +
    '</div>' +
    (status.dataDir ? '<div class="settings-diagnostic-path">' + escapeHtml(status.dataDir) + '</div>' : '');
}

export function buildHealthCheckStatus(health, snapshot) {
  if (!health) {
    const lastError = snapshot?.refresh_state?.last_error || snapshot?.error || '';
    return buildHealthRow('Health check', lastError ? 'Not run - refresh has an error' : 'Not run', lastError ? 'warning' : 'unknown');
  }
  const checks = [
    health.hasAuth,
    health.cacheOk,
    health.settingsOk,
    health.historyOk,
    health.dataDirAvailable,
  ];
  const failed = checks.filter(value => value === false).length;
  const summary = failed === 0 ? 'OK' : failed + ' issue' + (failed === 1 ? '' : 's');
  return '<div class="settings-health-list">' +
      buildHealthRow('Health check', summary, failed === 0 ? 'ok' : 'warning') +
      buildHealthRow('Data folder', formatCheck(health.dataDirAvailable, 'Available', 'Problem'), health.dataDirAvailable === false ? 'error' : 'ok') +
      buildHealthRow('Auth', formatCheck(health.hasAuth, 'Present', 'Missing'), health.hasAuth === false ? 'warning' : 'ok') +
      buildHealthRow('Cache file', formatCheck(health.cacheOk, 'OK', 'Problem'), health.cacheOk === false ? 'error' : 'ok') +
      buildHealthRow('Settings file', formatCheck(health.settingsOk, 'OK', 'Problem'), health.settingsOk === false ? 'error' : 'ok') +
      buildHealthRow('History file', formatCheck(health.historyOk, 'OK', 'Problem'), health.historyOk === false ? 'error' : 'ok') +
      (health.lastRefreshError ? buildHealthRow('Last refresh error', health.lastRefreshError, 'warning') : '') +
    '</div>';
}

function formatBytes(bytes) {
  if (!bytes || bytes === 0) return '0 KB';
  if (bytes >= 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + ' MB';
  return Math.ceil(bytes / 1024) + ' KB';
}

function sumLocalDataBytes(status) {
  if (!status) return null;
  return (status.cacheBytes || 0) +
    (status.historyBytes || 0) +
    (status.settingsBytes || 0) +
    (status.authBytes || 0) +
    (status.exportBytes || 0) +
    (status.backupBytes || 0);
}

function formatCheck(value, okLabel, badLabel) {
  if (value === true) return okLabel;
  if (value === false) return badLabel;
  return 'Unknown';
}

function buildDiagnosticStat(label, value) {
  return '<div class="settings-diagnostic-stat"><span>' + escapeHtml(label) + '</span><strong>' + escapeHtml(value) + '</strong></div>';
}

function buildHealthRow(label, value, state) {
  const pillClass = state === 'ok' ? 'ok' : state === 'error' ? 'error' : state === 'warning' ? 'warning' : '';
  const pillText = state === 'ok' ? 'OK' : state === 'error' ? 'Issue' : state === 'warning' ? 'Check' : 'Idle';
  // When the descriptive value duplicates the pill text (e.g. both "OK"),
  // drop the middle column to avoid showing the same label twice on one row.
  // Keep an empty span so the 4-column grid layout stays intact.
  const metaValue = value === pillText ? '' : value;
  return '<div class="settings-health-row ' + pillClass + '"><span>' + escapeHtml(label) + '</span>' +
    '<span class="settings-diagnostic-meta">' + escapeHtml(metaValue) + '</span>' +
    '<strong class="settings-status-pill ' + pillClass + '">' + escapeHtml(pillText) + '</strong></div>';
}
