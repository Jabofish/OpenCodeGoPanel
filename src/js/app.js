import { formatTimeAgo } from './format.js';
import { renderUsageTab } from './usage.js';
import { renderModelsTab } from './models.js';
import { renderSettingsTab } from './settings.js';
import { renderTrendsTab } from './trends.js';
import { deriveUsageInsights, pickPrimaryRisk, formatInsightShort } from './insights.js';
import { renderQuickPeek, openQuickPeek, closeQuickPeek, isQuickPeekOpen } from './quick-peek.js';
import { getWorkspaceDisplayName, getWorkspaceProfile, sortWorkspaces } from './workspaces.js';
import { showToast, showConfirm } from './toast.js';
import { getMaintenanceStatus, refreshMaintenanceStatus as refreshMaintenanceStatusData } from './maintenance.js';
import { initUpdater, checkForUpdateManually, consumePendingUpdate, isUpdateOverlayActive } from './updater.js';

// Check if Tauri API is available
if (!window.__TAURI__) {
  console.error('[App] FATAL: window.__TAURI__ is not available!');
  console.error('[App] This usually means Tauri failed to inject its API');
  showToast('Fatal Error: Tauri API not found', { type: 'error', duration: 0 });
}

// Use Tauri v2 global API
const { invoke } = window.__TAURI__?.core || {};
const { getCurrentWindow, cursorPosition } = window.__TAURI__?.window || {};
const { getCurrentWebviewWindow } = window.__TAURI__?.webviewWindow || {};

console.log('[App] Tauri API check:', {
  hasTauri: !!window.__TAURI__,
  hasCore: !!window.__TAURI__?.core,
  hasWindow: !!window.__TAURI__?.window,
  hasWebviewWindow: !!window.__TAURI__?.webviewWindow,
  hasInvoke: !!invoke,
});

// --- State ---
let currentTab = 'usage';
let isPinned = true;
let refreshTimer = null;
let snapshotTimer = null;
let latestSnapshot = null;
let latestRefreshState = null;
let settings = loadSettings();
let miniBadgeExpanded = false;
let lastManualRefreshAt = 0;
let workspaceSwitchState = {
  switching: false,
  previousId: '',
  error: null,
};

let modelView = {
  query: '',
  sortBy: 'calls',
  showAll: false,
  range: 'all',
};

const modelActions = {
  setQuery: (query) => {
    modelView = { ...modelView, query };
    renderModelsTab(latestSnapshot, modelView, modelActions);
  },
  setSortBy: (sortBy) => {
    modelView = { ...modelView, sortBy };
    renderModelsTab(latestSnapshot, modelView, modelActions);
  },
  toggleShowAll: () => {
    modelView = { ...modelView, showAll: !modelView.showAll };
    renderModelsTab(latestSnapshot, modelView, modelActions);
  },
  setRange: (range) => {
    modelView = { ...modelView, range };
    renderModelsTab(latestSnapshot, modelView, modelActions);
  },
};

let latestHistory = [];
let latestInsights = null;
let historyDays = 30;
let workspaceSelectRenderKey = '';

const trendActions = {
  setHistoryDays: async (days) => {
    historyDays = days;
    latestHistory = await fetchHistory(days);
    renderTrendsTab(latestHistory, latestSnapshot, settings, trendActions, historyDays);
  },
};

const MINI_BADGE_SIZE = { width: 60, height: 60 };
const MINI_BADGE_RING_SIZE = { width: 76, height: 76 };
const MINI_BADGE_DOT_SIZE = { width: 28, height: 28 };
const PANEL_MIN_SIZE = { width: 280, height: 320 };
const PANEL_SIZE = { width: 320, height: 480 };

const settingActions = {
  setPinned: async (value) => setPinned(value),
  setAutoRefresh: (value) => updateSettings({ autoRefresh: value }),
  setCompactMode: (value) => updateSettings({ compactMode: value }),
  setMiniBadgeMode: (value) => updateSettings({ miniBadgeMode: value }),
  setMiniBadgeSource: (value) => updateSettings({ miniBadgeSource: value }),
  setBudget: (value) => updateSettings({ monthlyBudget: value }),
  setNotifyQuota: (value) => updateSettings({ notifyQuota: value }),
  setNotifyBudgetProjection: (value) => updateSettings({ notifyBudgetProjection: value }),
  setNotifyRefreshFailure: (value) => updateSettings({ notifyRefreshFailure: value }),
  setQuietHoursEnabled: (value) => updateSettings({ quietHoursEnabled: value }),
  setHotkey: async (value) => {
    try {
      await invoke('set_hotkey', { hotkey: value });
      updateSettings({ hotkey: value });
    } catch (e) {
      console.error('[Hotkey] Failed to set hotkey:', e);
      showToast('Failed to set hotkey: ' + e, { type: 'error' });
    }
  },
  setThreshold: async (value) => {
    try {
      await invoke('set_threshold', { threshold: value });
      updateSettings({ usageThreshold: value });
    } catch (e) {
      console.error('[Threshold] Failed:', e);
      showToast('Failed to set threshold: ' + e, { type: 'error' });
    }
  },
  setRefreshVisibleSecs: async (value) => {
    updateSettings({ refreshVisibleSecs: value });
    try {
      await invoke('set_refresh_intervals', {
        visibleSecs: settings.refreshVisibleSecs,
        hiddenSecs: settings.refreshHiddenSecs,
      });
    } catch (e) { console.warn('[Refresh] set_refresh_intervals failed:', e); }
  },
  setRefreshHiddenSecs: async (value) => {
    updateSettings({ refreshHiddenSecs: value });
    try {
      await invoke('set_refresh_intervals', {
        visibleSecs: settings.refreshVisibleSecs,
        hiddenSecs: settings.refreshHiddenSecs,
      });
    } catch (e) { console.warn('[Refresh] set_refresh_intervals failed:', e); }
  },
  recordHotkey: async () => startHotkeyRecording(),
  refresh: async () => {
    await triggerRefresh();
    await renderAll();
  },
  login: async () => openLoginWindow(),
  clearAuth: async () => clearAuth(),
  clearCache: async () => clearCache(),
  hideToTray: async () => hideToTray(),
  minimize: async () => minimizeWindow(),
  exportData: async (kind) => {
    try {
      const path = await invoke('export_data', { kind });
      showToast('Exported to: ' + path, { type: 'success' });
    } catch (e) {
      showToast('Export failed: ' + e, { type: 'error' });
    }
  },
  setMiniBadgeDisplay: (value) => updateSettings({ miniBadgeDisplay: value }),
  updateSettings: (patch) => updateSettings(patch),
  sendTestNotification: async () => {
    try {
      await invoke('send_test_notification');
      showToast('Test notification sent', { type: 'success' });
    }
    catch (e) { showToast('Failed: ' + e, { type: 'error' }); }
  },
  openExportsFolder: async () => {
    try { await invoke('open_exports_folder'); }
    catch (e) {
      console.error('Failed to open exports folder:', e);
      showToast('Failed to open exports folder: ' + e, { type: 'error' });
    }
  },
  backupLocalData: async () => {
    try {
      const path = await invoke('backup_local_data');
      await refreshMaintenanceStatus();
      showToast('Backup saved: ' + path, { type: 'success' });
    }
    catch (e) {
      console.error('Backup failed:', e);
      showToast('Backup failed: ' + e, { type: 'error' });
    }
  },
  refreshMaintenanceStatus: async () => refreshMaintenanceStatus(),
  refreshLocalDataStatus: async () => refreshMaintenanceStatus(),
  runHealthCheck: async () => refreshMaintenanceStatus({ forceHealthToast: true }),
  clearCacheData: async () => settingActions.clearLocalData('cache'),
  clearAuthData: async () => settingActions.clearLocalData('auth'),
  clearSettingsData: async () => settingActions.clearLocalData('settings'),
  clearLocalData: async (scope) => {
    const confirmed = await showConfirm(
      `Clear ${scope} data? This cannot be undone.`,
      { title: 'Confirm Clear', confirmText: 'Clear', cancelText: 'Cancel' }
    );
    if (!confirmed) return;
    try {
      await invoke('clear_local_data', { scope });
      if (scope === 'cache') {
        // Trigger immediate refresh after cache clear
        await triggerRefresh();
        await renderAll();
      } else {
        await refreshMaintenanceStatus({ render: false });
        await renderAll();
      }
      showToast('Cleared ' + scope + ' data', { type: 'success' });
    }
    catch (e) {
      console.error('Failed to clear ' + scope + ':', e);
      showToast('Failed to clear ' + scope + ': ' + e, { type: 'error' });
    }
  },
  renameWorkspace: () => renameCurrentWorkspace(),
  toggleFavoriteWorkspace: () => toggleFavoriteCurrentWorkspace(),
  setAutoBackup: (value) => updateSettings({ autoBackup: value }),
  setAutoUpdate: (value) => updateSettings({ autoUpdate: value }),
  checkForUpdate: () => checkForUpdateManually(),
};

function defaultSettings() {
  return {
    autoRefresh: true,
    compactMode: true,
    miniBadgeMode: false,
    miniBadgeSource: 'auto',
    miniBadgeDisplay: 'percent',
    monthlyBudget: 6000,
    hotkey: 'Ctrl+Shift+U',
    usageThreshold: 80,
    refreshVisibleSecs: 30,
    refreshHiddenSecs: 600,
    recentWorkspaces: [],
    workspaceProfiles: {},
    notifyQuota: true,
    notifyBudgetProjection: true,
    notifyCostSpike: false,
    notifyRefreshFailure: true,
    quietHoursEnabled: false,
    quietHoursStart: '22:00',
    quietHoursEnd: '08:00',
    notificationCooldownMins: 60,
    autoUpdate: true,
    skippedUpdateVersion: '',
  };
}

function loadSettings() {
  try {
    return {
      ...defaultSettings(),
      ...JSON.parse(localStorage.getItem('ocp-settings') || '{}'),
    };
  } catch (_) {
    return defaultSettings();
  }
}

async function loadSettingsFromBackend() {
  const fallback = {
    ...defaultSettings(),
    ...JSON.parse(localStorage.getItem('ocp-settings') || '{}'),
  };
  if (!invoke) return fallback;
  try {
    const backendSettings = await invoke('get_settings');
    const migrated = { ...fallback, ...backendSettings };
    // Persist migrated settings to backend and mark migration done
    await invoke('save_settings', { next: migrated });
    localStorage.setItem('ocp-settings-migrated', '1');
    // Sync refresh intervals to scheduler
    await invoke('set_refresh_intervals', {
      visibleSecs: migrated.refreshVisibleSecs,
      hiddenSecs: migrated.refreshHiddenSecs,
    }).catch(() => {});
    return migrated;
  } catch (e) {
    console.warn('[Settings] Backend settings unavailable, using localStorage:', e);
    return fallback;
  }
}

function saveSettings() {
  localStorage.setItem('ocp-settings', JSON.stringify(settings));
  if (invoke) {
    invoke('save_settings', { next: settings })
      .catch(e => console.warn('[Settings] Failed to persist backend settings:', e));
  }
}

function startHotkeyRecording() {
  const previous = settings.hotkey || 'Ctrl+Shift+U';
  settings.hotkeyRecording = true;
  // Update settings UI to show recording state
  renderSettingsWithMaintenance(latestSnapshot);

  const onKeyDown = async (event) => {
    event.preventDefault();
    event.stopPropagation();
    document.removeEventListener('keydown', onKeyDown, true);

    const key = event.key?.toUpperCase();
    if (!event.ctrlKey || !event.shiftKey || !key || key.length !== 1 || !/^[A-Z]$/.test(key)) {
      showToast('Use Ctrl+Shift plus a letter A-Z', { type: 'warning' });
      delete settings.hotkeyRecording;
      updateSettings({ hotkey: previous });
      return;
    }

    const hotkey = 'Ctrl+Shift+' + key;
    try {
      await invoke('set_hotkey', { hotkey });
      delete settings.hotkeyRecording;
      updateSettings({ hotkey });
    } catch (e) {
      showToast('Failed to set hotkey: ' + e, { type: 'error' });
      delete settings.hotkeyRecording;
      updateSettings({ hotkey: previous });
    }
  };

  document.addEventListener('keydown', onKeyDown, true);
}

async function updateSettings(next) {
  const miniBadgeModeChanged = Object.prototype.hasOwnProperty.call(next, 'miniBadgeMode') &&
    next.miniBadgeMode !== settings.miniBadgeMode;
  settings = { ...settings, ...next };
  saveSettings();
  await applyUiSettings({ collapseMiniBadge: miniBadgeModeChanged });
}

async function applyUiSettings(options = {}) {
  document.documentElement.classList.toggle('mini-badge-mode', settings.miniBadgeMode);
  document.body.classList.toggle('compact-mode', settings.compactMode);
  document.body.classList.toggle('mini-badge-mode', settings.miniBadgeMode);
  document.body.classList.toggle('badge-ring', settings.miniBadgeDisplay === 'ring');
  document.body.classList.toggle('badge-dot', settings.miniBadgeDisplay === 'dot');

  const collapseMiniBadge = options.collapseMiniBadge ?? true;

  if (!settings.miniBadgeMode || collapseMiniBadge) {
    setMiniBadgeExpanded(false);
  }

  await resizeWindowForMiniBadge(settings.miniBadgeMode && miniBadgeExpanded);
}

function setMiniBadgeExpanded(expanded) {
  miniBadgeExpanded = expanded && settings.miniBadgeMode;
  document.documentElement.classList.toggle('expanded', miniBadgeExpanded);
  document.body.classList.toggle('expanded', miniBadgeExpanded);
}

async function resizeWindowForMiniBadge(expanded) {
  if (!getCurrentWindow) return;

  try {
    const win = getCurrentWindow();

    if (settings.miniBadgeMode && !expanded) {
      const badgeSize = settings.miniBadgeDisplay === 'dot' ? MINI_BADGE_DOT_SIZE
        : settings.miniBadgeDisplay === 'ring' ? MINI_BADGE_RING_SIZE
        : MINI_BADGE_SIZE;
      await setWindowMaxSize(win, null);
      await win.setShadow?.(false);
      await setWindowMinSize(win, badgeSize);
      await setWindowSize(win, badgeSize);
      await setWindowMaxSize(win, badgeSize);
      await win.setResizable?.(false);
      return;
    }

    await setWindowMaxSize(win, null);
    await setWindowMinSize(win, PANEL_MIN_SIZE);
    await win.setResizable?.(true);
    await win.setShadow?.(false);
    await setWindowSize(win, PANEL_SIZE);
  } catch (e) {
    console.warn(e);
  }
}

async function setWindowSize(win, size) {
  return invokeWindowCommand(win, 'set_size', logicalSize(size));
}

async function setWindowMinSize(win, size) {
  return invokeWindowCommand(win, 'set_min_size', size ? logicalSize(size) : null);
}

async function setWindowMaxSize(win, size) {
  return invokeWindowCommand(win, 'set_max_size', size ? logicalSize(size) : null);
}

function logicalSize(size) {
  return { Logical: { width: size.width, height: size.height } };
}

async function invokeWindowCommand(win, command, value) {
  if (invoke) {
    await invoke('plugin:window|' + command, {
      label: win.label || 'main',
      value,
    });
    return;
  }

  const method = {
    set_size: 'setSize',
    set_min_size: 'setMinSize',
    set_max_size: 'setMaxSize',
  }[command];
  await win[method]?.(value);
}

// --- Mini Badge Mode ---
function setupMiniBadge() {
  const miniBadge = document.getElementById('mini-badge');
  const app = document.getElementById('app');

  if (!miniBadge || !app) return;

  let collapseTimer = null;
  let pointerWatchTimer = null;
  let pointerWatchInFlight = false;
  let pointerOutsideSince = null;

  function clearCollapseTimer() {
    if (collapseTimer) {
      clearTimeout(collapseTimer);
      collapseTimer = null;
    }
  }

  async function expandMiniBadge() {
    if (!settings.miniBadgeMode) return;
    clearCollapseTimer();
    setMiniBadgeExpanded(true);
    await resizeWindowForMiniBadge(true);
    startPointerWatch();
    consumePendingUpdate();
  }

  async function collapseMiniBadge() {
    if (!settings.miniBadgeMode) return;
    clearCollapseTimer();
    stopPointerWatch();
    setMiniBadgeExpanded(false);
    await resizeWindowForMiniBadge(false);
  }

  function startMiniBadgeDrag(event) {
    if (!settings.miniBadgeMode || miniBadgeExpanded || event.button !== 0) return;
    if (event.detail >= 2) {
      event.preventDefault();
      expandMiniBadge();
      return;
    }

    try {
      const win = getCurrentWindow?.();
      win?.startDragging?.().catch(e => console.warn('[MiniBadge] Failed to start dragging:', e));
    } catch (e) {
      console.warn('[MiniBadge] Failed to start dragging:', e);
    }
  }

  function scheduleCollapse() {
    if (!settings.miniBadgeMode || !miniBadgeExpanded) return;
    if (isUpdateOverlayActive()) return;
    clearCollapseTimer();
    collapseTimer = setTimeout(collapseMiniBadge, 300);
  }

  function startPointerWatch() {
    stopPointerWatch();
    pointerOutsideSince = null;
    pointerWatchTimer = setInterval(checkPointerInsideExpandedWindow, 120);
  }

  function stopPointerWatch() {
    if (pointerWatchTimer) {
      clearInterval(pointerWatchTimer);
      pointerWatchTimer = null;
    }
    pointerWatchInFlight = false;
    pointerOutsideSince = null;
  }

  async function checkPointerInsideExpandedWindow() {
    if (pointerWatchInFlight || !settings.miniBadgeMode || !miniBadgeExpanded) return;
    if (!cursorPosition || !getCurrentWindow) return;
    if (isUpdateOverlayActive()) {
      pointerOutsideSince = null;
      clearCollapseTimer();
      return;
    }

    pointerWatchInFlight = true;
    try {
      const win = getCurrentWindow();
      const [cursor, position, size, scaleFactor] = await Promise.all([
        cursorPosition(),
        win.innerPosition(),
        win.innerSize(),
        win.scaleFactor?.() || Promise.resolve(1),
      ]);
      const expectedWidth = PANEL_SIZE.width * scaleFactor;
      const expectedHeight = PANEL_SIZE.height * scaleFactor;
      const width = Math.min(size.width, expectedWidth);
      const height = Math.min(size.height, expectedHeight);

      const margin = 2;
      const inside =
        cursor.x >= position.x - margin &&
        cursor.y >= position.y - margin &&
        cursor.x <= position.x + width + margin &&
        cursor.y <= position.y + height + margin;

      if (inside) {
        pointerOutsideSince = null;
        clearCollapseTimer();
      } else {
        pointerOutsideSince = pointerOutsideSince || Date.now();
        if (Date.now() - pointerOutsideSince >= 180) {
          await collapseMiniBadge();
        }
      }
    } catch (e) {
      console.warn('[MiniBadge] Pointer watch failed:', e);
      stopPointerWatch();
    } finally {
      pointerWatchInFlight = false;
    }
  }

  miniBadge.addEventListener('mousedown', startMiniBadgeDrag);
  miniBadge.addEventListener('dblclick', (event) => {
    event.preventDefault();
    expandMiniBadge();
  });
  app.addEventListener('mouseleave', scheduleCollapse);
  app.addEventListener('mouseenter', clearCollapseTimer);
  document.addEventListener('mouseleave', scheduleCollapse);
}

function updateMiniBadge(snapshot) {
  if (!snapshot || snapshot.error) return;

  const percentEl = document.getElementById('mini-badge-percent');
  const labelEl = document.getElementById('mini-badge-label');
  const indicatorEl = document.getElementById('mini-badge-indicator');
  const miniBadge = document.getElementById('mini-badge');
  if (!percentEl || !labelEl || !indicatorEl) return;

  const rollingPct = snapshot.usage?.rolling?.usage_percent ?? 0;
  const weeklyPct  = snapshot.usage?.weekly?.usage_percent  ?? 0;
  const monthlyPct = snapshot.usage?.monthly?.usage_percent ?? 0;

  const source = settings.miniBadgeSource || 'auto';
  let percentage, label;

  if (source === 'auto') {
    percentage = Math.max(rollingPct, weeklyPct, monthlyPct);
    if (percentage === monthlyPct && percentage > weeklyPct) {
      label = 'Monthly';
    } else if (percentage === weeklyPct && percentage > rollingPct) {
      label = 'Weekly';
    } else {
      label = 'Rolling';
    }
  } else if (source === 'rolling') {
    percentage = rollingPct;
    label = 'Rolling';
  } else if (source === 'weekly') {
    percentage = weeklyPct;
    label = 'Weekly';
  } else if (source === 'monthly') {
    percentage = monthlyPct;
    label = 'Monthly';
  } else {
    percentage = Math.max(rollingPct, weeklyPct, monthlyPct);
    label = 'Rolling';
  }

  percentEl.textContent = percentage + '%';
  percentEl.classList.toggle('three-digit', percentage >= 100);
  labelEl.textContent = label;

  // Set title attribute on mini badge with insight if available
  if (miniBadge) {
    const primary = pickPrimaryRisk(latestInsights);
    if (primary) {
      miniBadge.title = label + ' usage: ' + percentage + '% · ' + formatInsightShort(primary);
    } else {
      miniBadge.title = label + ' usage: ' + percentage + '%';
    }
  }

  indicatorEl.classList.remove('warning', 'danger');
  if (percentage >= settings.usageThreshold) {
    indicatorEl.classList.add('danger');
  } else if (percentage >= settings.usageThreshold * 0.8) {
    indicatorEl.classList.add('warning');
  }

  // Ring mode SVG variables
  if (miniBadge) {
    const ringColor = percentage >= settings.usageThreshold ? '#e06170'
      : percentage >= settings.usageThreshold * 0.8 ? '#e9ae55'
      : '#5fcf97';
    miniBadge.style.setProperty('--badge-ring-color', ringColor);

    if (document.body.classList.contains('badge-ring')) {
      updateRingSvg(miniBadge, Math.min(percentage, 100));
    } else {
      // Clean up SVG when not in ring mode
      const existing = miniBadge.querySelector('.mini-badge-ring-svg');
      if (existing) existing.remove();
    }
  }
}

const RING_RADIUS = 30;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

function updateRingSvg(badge, pct) {
  let svg = badge.querySelector('.mini-badge-ring-svg');
  if (!svg) {
    const ns = 'http://www.w3.org/2000/svg';
    svg = document.createElementNS(ns, 'svg');
    svg.setAttribute('class', 'mini-badge-ring-svg');
    svg.setAttribute('viewBox', '0 0 72 72');

    const track = document.createElementNS(ns, 'circle');
    track.setAttribute('class', 'mini-badge-ring-track');
    track.setAttribute('cx', '36');
    track.setAttribute('cy', '36');
    track.setAttribute('r', String(RING_RADIUS));
    svg.appendChild(track);

    const fill = document.createElementNS(ns, 'circle');
    fill.setAttribute('class', 'mini-badge-ring-fill');
    fill.setAttribute('cx', '36');
    fill.setAttribute('cy', '36');
    fill.setAttribute('r', String(RING_RADIUS));
    fill.setAttribute('stroke-dasharray', String(RING_CIRCUMFERENCE));
    svg.appendChild(fill);

    badge.insertBefore(svg, badge.firstChild);
  }

  const fillCircle = svg.querySelector('.mini-badge-ring-fill');
  if (fillCircle) {
    const offset = RING_CIRCUMFERENCE * (1 - pct / 100);
    fillCircle.setAttribute('stroke-dashoffset', String(offset));
  }
}

// --- Data fetching ---
async function fetchSnapshot() {
  console.log('[Fetch] Fetching snapshot...');
  try {
    if (!invoke) {
      console.error('[Fetch] invoke function not available');
      return { error: 'Tauri API not loaded' };
    }
    const data = await invoke('get_snapshot');
    console.log('[Fetch] Snapshot received:', data);
    return data;
  } catch (e) {
    console.error('[Fetch] Error:', e);
    return { error: 'Failed to connect: ' + e };
  }
}

async function checkAuthStatus() {
  console.log('[Auth] Checking auth status...');
  try {
    if (!invoke) return false;
    const hasAuth = await invoke('get_auth_status');
    console.log('[Auth] Has auth:', hasAuth);
    return hasAuth;
  } catch (e) {
    console.error('[Auth] Check failed:', e);
    return false;
  }
}

async function openLoginWindow() {
  console.log('[Login] Opening login window...');
  try {
    if (!invoke) {
      console.error('[Login] invoke not available');
      return;
    }
    await invoke('open_login_window');
    console.log('[Login] Login window opened');
  } catch (e) {
    console.error('[Login] Failed to open login window:', e);
  }
}

async function triggerRefresh() {
  const now = Date.now();
  const cooldownMs = 10000; // 10s cooldown for manual refresh
  if (now - lastManualRefreshAt < cooldownMs) {
    console.log('[Refresh] Cooldown active, skipping manual refresh');
    return;
  }
  lastManualRefreshAt = now;

  console.log('[Refresh] Triggering refresh...');
  try {
    if (!invoke) return;
    // Immediately show refreshing state
    const btn = document.getElementById('btn-refresh');
    if (btn) {
      btn.disabled = true;
      btn.textContent = 'Refreshing';
    }
    await invoke('refresh_now');
    console.log('[Refresh] Refresh triggered');
  } catch (e) {
    console.error('[Refresh] Refresh failed:', e);
  }
}

async function fetchHistory(days = 30) {
  try {
    if (!invoke) return [];
    return await invoke('get_history', { days });
  } catch (e) {
    console.warn('[History] Failed:', e);
    return [];
  }
}

async function clearAuth() {
  console.log('[Auth] Clearing auth...');
  try {
    if (!invoke) return;
    await invoke('clear_auth');
    await triggerRefresh();
    await renderAll();
  } catch (e) {
    console.error('[Auth] Clear failed:', e);
  }
}

async function clearCache() {
  console.log('[Cache] Clearing cache...');
  try {
    if (!invoke) return;
    await invoke('clear_cache');
    await renderAll();
    await triggerRefresh();
  } catch (e) {
    console.error('[Cache] Clear failed:', e);
  }
}

async function setVisibility(visible) {
  console.log('[Visibility] Setting visibility:', visible);
  try {
    if (!invoke) return;
    await invoke('set_visibility', { visible });
    console.log('[Visibility] Visibility set');
  } catch (e) {
    console.error('[Visibility] Toggle failed:', e);
  }
}

// --- Rendering ---
async function renderAll() {
  console.log('[Render] Starting render cycle...');

  try {
    const snapshot = await fetchSnapshot();
    console.log('[Render] Snapshot received:', snapshot);
    applySnapshot(snapshot);

    // Check if need login
    if (snapshot.error && (snapshot.error.includes('Not logged in') || snapshot.error.includes('Not yet loaded'))) {
      console.log('[Render] Not logged in, checking auth status...');
      const hasAuth = await checkAuthStatus();
      console.log('[Render] Has auth:', hasAuth);

      if (!hasAuth) {
        console.log('[Render] Will open login window in 1s...');
        // Auto-open login window on first load
        setTimeout(() => {
          console.log('[Render] Opening login window...');
          openLoginWindow().catch(err => console.error('[Render] Failed to open login:', err));
        }, 1000);
      }
    }
  } catch (error) {
    console.error('[Render] Error during render:', error);
  }
}

function applySnapshot(snapshot) {
  latestSnapshot = snapshot;
  latestRefreshState = snapshot.refresh_state || null;
  latestInsights = deriveUsageInsights(snapshot, latestHistory, settings);
  updateFooter(snapshot);
  updateRefreshButton(snapshot);
  updateMiniBadge(snapshot);
  renderUsageTab(snapshot, settings, latestInsights);
  renderModelsTab(snapshot, modelView, modelActions);
  renderTrendsTab(latestHistory, snapshot, settings, trendActions, historyDays);
  renderSettingsWithMaintenance(snapshot);
  loadWorkspaces();
}

function updateFooter(snapshot) {
  const footer = document.getElementById('footer-time');
  if (!footer) return;

  // Workspace switch takes priority
  if (workspaceSwitchState.switching) {
    footer.textContent = 'Switching workspace...';
    return;
  }
  if (workspaceSwitchState.error) {
    footer.textContent = 'Workspace switch failed';
    return;
  }

  // Refresh state messages
  const rs = snapshot.refresh_state;
  if (rs && rs.is_refreshing) {
    const phaseLabels = {
      auth: 'Refreshing auth...',
      usage: 'Refreshing usage...',
      records: 'Refreshing records...',
      costs: 'Refreshing costs...',
    };
    footer.textContent = phaseLabels[rs.phase] || 'Refreshing...';
    return;
  }

  // Auth error
  if (snapshot.error && (snapshot.error.includes('Not logged in') || snapshot.error.includes('Session expired'))) {
    footer.textContent = 'Login required';
  } else if (snapshot.error) {
    footer.textContent = 'Update failed · using cached data';
  } else {
    footer.textContent = 'Updated ' + formatTimeAgo(snapshot.last_updated);
  }
}

function updateRefreshButton(snapshot) {
  const btn = document.getElementById('btn-refresh');
  if (!btn) return;
  const refreshing = !!(snapshot.refresh_state && snapshot.refresh_state.is_refreshing);
  btn.disabled = refreshing;
  btn.textContent = refreshing ? 'Refreshing' : 'Refresh';
}

function isTypingTarget(target) {
  return target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
}

// --- Workspace Switching ---
function rememberWorkspace(workspaceId) {
  const next = [workspaceId, ...(settings.recentWorkspaces || []).filter(id => id !== workspaceId)].slice(0, 5);
  updateSettings({ recentWorkspaces: next });
}

function renameCurrentWorkspace() {
  const wid = latestSnapshot?.workspace_id;
  if (!wid) return;
  const profiles = { ...(settings.workspaceProfiles || {}) };
  const current = profiles[wid]?.alias || '';
  const alias = prompt('Workspace alias (empty to clear):', current);
  if (alias === null) return; // cancelled
  if (alias.trim()) {
    profiles[wid] = { ...(profiles[wid] || {}), alias: alias.trim() };
  } else {
    if (profiles[wid]) delete profiles[wid].alias;
  }
  updateSettings({ workspaceProfiles: profiles });
}

function toggleFavoriteCurrentWorkspace() {
  const wid = latestSnapshot?.workspace_id;
  if (!wid) return;
  const profiles = { ...(settings.workspaceProfiles || {}) };
  const current = profiles[wid] || {};
  profiles[wid] = { ...current, favorite: !current.favorite };
  updateSettings({ workspaceProfiles: profiles });
}

async function refreshMaintenanceStatus(options = {}) {
  await refreshMaintenanceStatusData(invoke, options);
  if (options.render !== false) {
    renderSettingsWithMaintenance(latestSnapshot);
  }
}

function renderSettingsWithMaintenance(snapshot) {
  const { localDataStatus, localHealthCheck } = getMaintenanceStatus();
  renderSettingsTab(snapshot, settings, settingActions, isPinned, localDataStatus, localHealthCheck);
}

async function switchWorkspaceById(workspaceId) {
  const sel = document.getElementById('workspace-selector');
  if (sel) sel.value = workspaceId;
  // Trigger the selector change logic
  workspaceSwitchState.previousId = latestSnapshot?.workspace_id || '';
  workspaceSwitchState.switching = true;
  workspaceSwitchState.error = null;
  const footer = document.getElementById('footer-time');
  if (footer) footer.textContent = 'Switching workspace...';
  try {
    await invoke('switch_workspace', { workspaceId });
    workspaceSwitchState.switching = false;
    rememberWorkspace(workspaceId);
    await renderAll();
    setTimeout(() => renderAll().catch(e => console.warn('[Workspace] Follow-up render failed:', e)), 700);
  } catch (e) {
    console.error('[Workspace] Switch failed:', e);
    workspaceSwitchState.switching = false;
    workspaceSwitchState.error = String(e);
    if (sel) sel.value = workspaceSwitchState.previousId;
  }
}

async function loadWorkspaces() {
  const sel = document.getElementById('workspace-selector');
  if (!sel || !latestSnapshot) return;
  try {
    const workspaces = sortWorkspaces(latestSnapshot.workspaces || [], settings);
    const key = workspaces.map(ws => {
      const profile = getWorkspaceProfile(ws.id, settings);
      return [
        ws.id,
        ws.name || '',
        profile.favorite ? '1' : '0',
        profile.alias || '',
      ].join(':');
    }).join('|') + '::' + (latestSnapshot.workspace_id || '');

    if (key === workspaceSelectRenderKey) return;
    workspaceSelectRenderKey = key;

    sel.innerHTML = '';
    if (workspaces.length <= 1) {
      sel.style.display = 'none';
      return;
    }
    sel.style.display = '';
    for (const ws of workspaces) {
      const opt = document.createElement('option');
      opt.value = ws.id;
      const profile = getWorkspaceProfile(ws.id, settings);
      opt.textContent = (profile.favorite ? '* ' : '') + getWorkspaceDisplayName(ws, settings);
      opt.selected = ws.id === latestSnapshot.workspace_id;
      sel.appendChild(opt);
    }
  } catch (e) {
    console.warn('[Workspace] Failed to load:', e);
    sel.style.display = 'none';
  }
}

function setupWorkspaceSelector() {
  const sel = document.getElementById('workspace-selector');
  if (!sel) return;
  sel.addEventListener('change', async () => {
    const wid = sel.value;
    if (!wid) return;

    workspaceSwitchState.previousId = latestSnapshot?.workspace_id || '';
    workspaceSwitchState.switching = true;
    workspaceSwitchState.error = null;

    // Show immediate feedback
    const footer = document.getElementById('footer-time');
    if (footer) footer.textContent = 'Switching workspace...';

    try {
      console.log('[Workspace] Switching to:', wid);
      await invoke('switch_workspace', { workspaceId: wid });
      console.log('[Workspace] Switched OK, showing cached data and refreshing...');
      workspaceSwitchState.switching = false;
      rememberWorkspace(wid);
      await renderAll();
      setTimeout(() => renderAll().catch(e => console.warn('[Workspace] Follow-up render failed:', e)), 700);
    } catch (e) {
      console.error('[Workspace] Switch failed:', e);
      workspaceSwitchState.switching = false;
      workspaceSwitchState.error = String(e);
      // Restore previous selection
      sel.value = workspaceSwitchState.previousId;
      // Clear error after a few seconds
      setTimeout(() => {
        workspaceSwitchState.error = null;
        applySnapshot(latestSnapshot);
      }, 5000);
    }
  });
}

// --- Tab Switching ---
function switchTab(name) {
  currentTab = name;
  document.querySelectorAll('.tab').forEach(t => t.classList.toggle('active', t.dataset.tab === name));
  document.querySelectorAll('.tab-panel').forEach(p => p.classList.toggle('active', p.id === 'tab-' + name));
  if (name === 'settings') {
    renderSettingsWithMaintenance(latestSnapshot);
    refreshMaintenanceStatus().catch(e => console.warn('[Maintenance] Refresh failed:', e));
  }
  if (name === 'trends') {
    fetchHistory(historyDays).then(history => {
      latestHistory = history;
      renderTrendsTab(latestHistory, latestSnapshot, settings, trendActions, historyDays);
    });
  }
}

// --- Window Controls ---
function setupWindowControls() {
  document.getElementById('btn-pin').addEventListener('click', togglePin);
  document.getElementById('btn-min').addEventListener('click', minimizeWindow);
  document.getElementById('btn-close').addEventListener('click', hideToTray);
  document.getElementById('btn-refresh').addEventListener('click', async () => {
    await triggerRefresh();
    await renderAll();
  });
}

async function setPinned(value) {
  try {
    if (getCurrentWindow) {
      isPinned = value;
      await getCurrentWindow().setAlwaysOnTop(isPinned);
      document.getElementById('btn-pin').classList.toggle('active', isPinned);
      renderSettingsWithMaintenance(latestSnapshot);
    }
  } catch (error) {
    console.error('[Controls] Failed to set pin:', error);
  }
}

async function togglePin() {
  console.log('[Controls] Pin clicked');
  await setPinned(!isPinned);
  console.log('[Controls] Always-on-top:', isPinned);
}

async function minimizeWindow() {
  console.log('[Controls] Minimize clicked');
  try {
    if (getCurrentWindow) {
      await getCurrentWindow().minimize();
      console.log('[Controls] Window minimized');
    } else {
      console.error('[Controls] getCurrentWindow not available');
    }
  } catch (error) {
    console.error('[Controls] Failed to minimize:', error);
  }
}

async function hideToTray() {
  console.log('[Controls] Hide to tray');
  try {
    if (invoke) {
      // Backend hides the window and flips the scheduler visibility flag so
      // background refresh drops to the 10-minute cadence. The Rust layer
      // separately intercepts CloseRequested events (Alt+F4, etc.) for the
      // same effect, so this call is also the explicit in-app path.
      await invoke('hide_to_tray');
      console.log('[Controls] Window hidden to tray');
    } else {
      console.error('[Controls] invoke not available');
    }
  } catch (error) {
    console.error('[Controls] Failed to hide to tray:', error);
  }
}

// --- Tab click listeners ---
function setupTabs() {
  document.querySelectorAll('.tab').forEach(tab => {
    tab.addEventListener('click', () => switchTab(tab.dataset.tab));
  });
}

// --- Refresh loop ---
function startRefreshLoop() {
  snapshotTimer = setInterval(async () => {
    const snapshot = await fetchSnapshot();
    applySnapshot(snapshot);
  }, 3000);

  refreshTimer = setInterval(async () => {
    if (settings.autoRefresh) {
      await triggerRefresh();
    }
  }, 30000); // 30s visible refresh
}

function buildQuickPeekState() {
  return {
    snapshot: latestSnapshot,
    settings,
    insights: latestInsights,
    currentTab,
    recentWorkspaces: settings.recentWorkspaces || [],
  };
}

const quickPeekActions = {
  refresh: async () => {
    await triggerRefresh();
    closeQuickPeek();
  },
  switchTab: (tab) => {
    switchTab(tab);
    closeQuickPeek();
  },
  switchWorkspace: async (workspaceId) => {
    await switchWorkspaceById(workspaceId);
    closeQuickPeek();
  },
  close: () => closeQuickPeek(),
};

// --- Init ---
async function init() {
  console.log('[Init] Starting application...');

  try {
    // Load settings from backend, falling back to localStorage
    settings = await loadSettingsFromBackend();

    // Initialize auto-update event listener
    initUpdater();

    setupTabs();
    console.log('[Init] Tabs setup complete');

    setupWindowControls();
    setupWorkspaceSelector();
    setupMiniBadge();
    console.log('[Init] Window controls setup complete');
    document.getElementById('btn-pin')?.classList.toggle('active', isPinned);
    applyUiSettings();

    // Sync threshold to backend on startup
    if (settings.usageThreshold > 0) {
      try {
        await invoke('set_threshold', { threshold: settings.usageThreshold });
      } catch (e) {
        console.warn('[Init] Failed to sync threshold:', e);
      }
    }

    // Sync refresh intervals to scheduler
    try {
      await invoke('set_refresh_intervals', {
        visibleSecs: settings.refreshVisibleSecs || 30,
        hiddenSecs: settings.refreshHiddenSecs || 600,
      });
    } catch (e) {
      console.warn('[Init] set_refresh_intervals not available:', e);
    }

    // Keyboard shortcuts: Quick Peek (Space/K), Esc to close, F12 devtools
    document.addEventListener('keydown', async (e) => {
      // Esc closes Quick Peek
      if (e.key === 'Escape' && isQuickPeekOpen()) {
        closeQuickPeek();
        return;
      }
      // Space or K opens Quick Peek (skip when typing in inputs)
      if ((e.key === ' ' || e.key.toLowerCase() === 'k') && !isQuickPeekOpen() && !isTypingTarget(e.target)) {
        e.preventDefault();
        renderQuickPeek(buildQuickPeekState(), quickPeekActions);
        return;
      }
      // F12 DevTools
      if (e.key === 'F12') {
        console.log('[DevTools] F12 pressed, attempting to toggle devtools...');
        try {
          if (getCurrentWebviewWindow) {
            const webview = getCurrentWebviewWindow();
            if (typeof webview.toggleDevtools === 'function') {
              await webview.toggleDevtools();
              console.log('[DevTools] Devtools toggled');
            } else {
              console.warn('[DevTools] toggleDevtools is not available in this runtime');
            }
          } else {
            console.error('[DevTools] getCurrentWebviewWindow not available');
          }
        } catch (error) {
          console.error('[DevTools] Failed to toggle:', error);
        }
      }
    });

    // Titlebar click to open Quick Peek
    document.getElementById('app-title')?.addEventListener('click', (e) => {
      e.stopPropagation();
      renderQuickPeek(buildQuickPeekState(), quickPeekActions);
    });

    await renderAll();
    console.log('[Init] Initial render complete');

    // Load workspace list
    await loadWorkspaces();

    // Trigger refresh and re-render with the updated data
    await triggerRefresh();
    console.log('[Init] Initial refresh triggered, fetching updated snapshot...');
    await refreshMaintenanceStatus({ render: false });
    await renderAll();
    console.log('[Init] Re-rendered with refreshed data');

    startRefreshLoop();
    console.log('[Init] Refresh loop started');

    console.log('[Init] Application ready!');
  } catch (error) {
    console.error('[Init] Fatal error:', error);
    showToast('Initialization failed: ' + error.message, { type: 'error', duration: 0 });
  }
}

document.addEventListener('DOMContentLoaded', init);
