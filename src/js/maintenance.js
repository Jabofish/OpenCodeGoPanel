import { showToast } from './toast.js';

let localDataStatus = null;
let localHealthCheck = null;

export function getMaintenanceStatus() {
  return {
    localDataStatus,
    localHealthCheck,
  };
}

export async function fetchLocalDataStatus(invoke) {
  try {
    if (!invoke) return;
    localDataStatus = await invoke('get_local_data_status');
  } catch (e) {
    console.warn('[Maintenance] Failed:', e);
  }
}

export async function fetchHealthCheck(invoke) {
  try {
    if (!invoke) return;
    localHealthCheck = await invoke('run_health_check');
  } catch (e) {
    console.warn('[Maintenance] Health check failed:', e);
    localHealthCheck = { lastRefreshError: String(e) };
  }
}

export async function refreshMaintenanceStatus(invoke, options = {}) {
  await Promise.all([fetchLocalDataStatus(invoke), fetchHealthCheck(invoke)]);
  if (options.forceHealthToast) {
    showHealthCheckToast(localHealthCheck);
  }
  return getMaintenanceStatus();
}

function showHealthCheckToast(healthCheck) {
  const ok = isHealthCheckPassing(healthCheck);
  showToast(ok ? 'Health check passed' : 'Health check completed', {
    type: ok ? 'success' : 'warning',
  });
}

function isHealthCheckPassing(healthCheck) {
  return !!(healthCheck?.hasAuth && healthCheck?.cacheOk &&
    healthCheck?.settingsOk && healthCheck?.historyOk);
}
