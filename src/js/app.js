import { formatTimeAgo } from './format.js';
import { renderUsageTab } from './usage.js';
import { renderModelsTab } from './models.js';
import { renderSettingsTab } from './settings.js';
import { renderTrendsTab } from './trends.js';

// Check if Tauri API is available
if (!window.__TAURI__) {
  console.error('[App] FATAL: window.__TAURI__ is not available!');
  console.error('[App] This usually means Tauri failed to inject its API');
  alert('Fatal Error: Tauri API not found. Please check if the app is running in Tauri environment.');
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
};

const modelActions = {
  setQuery: (query) => {
    modelView = { ...modelView, query };
    renderModelsTab(latestSnapshot, modelView);
  },
  setSortBy: (sortBy) => {
    modelView = { ...modelView, sortBy };
    renderModelsTab(latestSnapshot, modelView);
  },
  toggleShowAll: () => {
    modelView = { ...modelView, showAll: !modelView.showAll };
    renderModelsTab(latestSnapshot, modelView);
  },
};
// Expose for models.js event binding
window._modelActions = modelActions;

let latestHistory = [];
let historyDays = 30;

const trendActions = {
  setHistoryDays: async (days) => {
    historyDays = days;
    latestHistory = await fetchHistory(days);
    renderTrendsTab(latestHistory, latestSnapshot, settings, trendActions, historyDays);
  },
};

const MINI_BADGE_SIZE = { width: 60, height: 60 };
const PANEL_MIN_SIZE = { width: 280, height: 320 };
const PANEL_SIZE = { width: 320, height: 480 };

const settingActions = {
  setPinned: async (value) => setPinned(value),
  setAutoRefresh: (value) => updateSettings({ autoRefresh: value }),
  setCompactMode: (value) => updateSettings({ compactMode: value }),
  setMiniBadgeMode: (value) => updateSettings({ miniBadgeMode: value }),
  setMiniBadgeSource: (value) => updateSettings({ miniBadgeSource: value }),
  setBudget: (value) => updateSettings({ monthlyBudget: value }),
  setHotkey: async (value) => {
    try {
      await invoke('set_hotkey', { hotkey: value });
      updateSettings({ hotkey: value });
    } catch (e) {
      console.error('[Hotkey] Failed to set hotkey:', e);
      alert('Failed to set hotkey: ' + e);
    }
  },
  setThreshold: async (value) => {
    try {
      await invoke('set_threshold', { threshold: value });
      updateSettings({ usageThreshold: value });
    } catch (e) {
      console.error('[Threshold] Failed:', e);
      alert('Failed to set threshold: ' + e);
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
      alert('Exported to: ' + path);
    } catch (e) {
      alert('Export failed: ' + e);
    }
  },
};

function defaultSettings() {
  return {
    autoRefresh: true,
    compactMode: true,
    miniBadgeMode: false,
    miniBadgeSource: 'auto',
    monthlyBudget: 6000,
    hotkey: 'Ctrl+Shift+U',
    usageThreshold: 80,
    refreshVisibleSecs: 30,
    refreshHiddenSecs: 600,
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
  renderSettingsTab(latestSnapshot, settings, settingActions, isPinned);

  const onKeyDown = async (event) => {
    event.preventDefault();
    event.stopPropagation();
    document.removeEventListener('keydown', onKeyDown, true);

    const key = event.key?.toUpperCase();
    if (!event.ctrlKey || !event.shiftKey || !key || key.length !== 1 || !/^[A-Z]$/.test(key)) {
      alert('Use Ctrl+Shift plus a letter A-Z.');
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
      alert('Failed to set hotkey: ' + e);
      delete settings.hotkeyRecording;
      updateSettings({ hotkey: previous });
    }
  };

  document.addEventListener('keydown', onKeyDown, true);
}

function updateSettings(next) {
  const miniBadgeModeChanged = Object.prototype.hasOwnProperty.call(next, 'miniBadgeMode') &&
    next.miniBadgeMode !== settings.miniBadgeMode;
  settings = { ...settings, ...next };
  saveSettings();
  applyUiSettings({ collapseMiniBadge: miniBadgeModeChanged });
  renderSettingsTab(latestSnapshot, settings, settingActions, isPinned);
}

function applyUiSettings(options = {}) {
  document.documentElement.classList.toggle('mini-badge-mode', settings.miniBadgeMode);
  document.body.classList.toggle('compact-mode', settings.compactMode);
  document.body.classList.toggle('mini-badge-mode', settings.miniBadgeMode);

  const collapseMiniBadge = options.collapseMiniBadge ?? true;

  if (!settings.miniBadgeMode || collapseMiniBadge) {
    setMiniBadgeExpanded(false);
  }

  resizeWindowForMiniBadge(settings.miniBadgeMode && miniBadgeExpanded);
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
      await setWindowMaxSize(win, null);
      await win.setShadow?.(false);
      await setWindowMinSize(win, MINI_BADGE_SIZE);
      await setWindowSize(win, MINI_BADGE_SIZE);
      await setWindowMaxSize(win, MINI_BADGE_SIZE);
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

  // Set title attribute on mini badge for accessibility
  if (miniBadge) {
    miniBadge.title = label + ' usage: ' + percentage + '%';
  }

  indicatorEl.classList.remove('warning', 'danger');
  if (percentage >= settings.usageThreshold) {
    indicatorEl.classList.add('danger');
  } else if (percentage >= settings.usageThreshold * 0.8) {
    indicatorEl.classList.add('warning');
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
  updateFooter(snapshot);
  updateRefreshButton(snapshot);
  updateMiniBadge(snapshot);
  renderUsageTab(snapshot, settings);
  renderModelsTab(snapshot, modelView);
  renderTrendsTab(latestHistory, snapshot, settings, trendActions, historyDays);
  renderSettingsTab(snapshot, settings, settingActions, isPinned);
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

// --- Workspace Switching ---
async function loadWorkspaces() {
  const sel = document.getElementById('workspace-selector');
  if (!sel || !latestSnapshot) return;
  try {
    const workspaces = latestSnapshot.workspaces || [];
    sel.innerHTML = '';
    if (workspaces.length <= 1) {
      sel.style.display = 'none';
      return;
    }
    sel.style.display = '';
    for (const ws of workspaces) {
      const opt = document.createElement('option');
      opt.value = ws.id;
      opt.textContent = ws.name || ws.id;
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
  if (name === 'settings') renderSettingsTab(latestSnapshot, settings, settingActions, isPinned);
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
      renderSettingsTab(latestSnapshot, settings, settingActions, isPinned);
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

// --- Init ---
async function init() {
  console.log('[Init] Starting application...');

  try {
    // Load settings from backend, falling back to localStorage
    settings = await loadSettingsFromBackend();

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

    // Add F12 for dev tools in debug mode
    document.addEventListener('keydown', async (e) => {
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

    await renderAll();
    console.log('[Init] Initial render complete');

    // Load workspace list
    await loadWorkspaces();

    // Trigger refresh and re-render with the updated data
    await triggerRefresh();
    console.log('[Init] Initial refresh triggered, fetching updated snapshot...');
    await renderAll();
    console.log('[Init] Re-rendered with refreshed data');

    startRefreshLoop();
    console.log('[Init] Refresh loop started');

    console.log('[Init] Application ready!');
  } catch (error) {
    console.error('[Init] Fatal error:', error);
    alert('Initialization failed: ' + error.message);
  }
}

document.addEventListener('DOMContentLoaded', init);
