import { formatTimeAgo } from './format.js';
import { renderUsageTab } from './usage.js';
import { renderModelsTab } from './models.js';
import { renderSettingsTab } from './settings.js';

// Check if Tauri API is available
if (!window.__TAURI__) {
  console.error('[App] FATAL: window.__TAURI__ is not available!');
  console.error('[App] This usually means Tauri failed to inject its API');
  alert('Fatal Error: Tauri API not found. Please check if the app is running in Tauri environment.');
}

// Use Tauri v2 global API
const { invoke } = window.__TAURI__?.core || {};
const { getCurrentWindow } = window.__TAURI__?.window || {};
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
let settings = loadSettings();

const MINI_BADGE_SIZE = { width: 60, height: 60 };
const PANEL_MIN_SIZE = { width: 280, height: 320 };
const PANEL_SIZE = { width: 320, height: 480 };

const settingActions = {
  setPinned: async (value) => setPinned(value),
  setAutoRefresh: (value) => updateSettings({ autoRefresh: value }),
  setCompactMode: (value) => updateSettings({ compactMode: value }),
  setMiniBadgeMode: (value) => updateSettings({ miniBadgeMode: value }),
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
  refresh: async () => {
    await triggerRefresh();
    await renderAll();
  },
  login: async () => openLoginWindow(),
  clearAuth: async () => clearAuth(),
  clearCache: async () => clearCache(),
  hideToTray: async () => hideToTray(),
  minimize: async () => minimizeWindow(),
};

function loadSettings() {
  try {
    return {
      autoRefresh: true,
      compactMode: true,
      miniBadgeMode: false,
      monthlyBudget: 6000,
      hotkey: 'Ctrl+Shift+U',
      usageThreshold: 80,
      ...JSON.parse(localStorage.getItem('ocp-settings') || '{}'),
    };
  } catch (_) {
    return { autoRefresh: true, compactMode: true, miniBadgeMode: false, monthlyBudget: 6000, hotkey: 'Ctrl+Shift+U', usageThreshold: 80 };
  }
}

function saveSettings() {
  localStorage.setItem('ocp-settings', JSON.stringify(settings));
}

function updateSettings(next) {
  settings = { ...settings, ...next };
  saveSettings();
  applyUiSettings();
  renderSettingsTab(latestSnapshot, settings, settingActions, isPinned);
}

function applyUiSettings() {
  document.documentElement.classList.toggle('mini-badge-mode', settings.miniBadgeMode);
  document.body.classList.toggle('compact-mode', settings.compactMode);
  document.body.classList.toggle('mini-badge-mode', settings.miniBadgeMode);

  document.documentElement.classList.remove('expanded');
  document.body.classList.remove('expanded');

  resizeWindowForMiniBadge(false);
}

function setMiniBadgeExpanded(expanded) {
  document.documentElement.classList.toggle('expanded', expanded);
  document.body.classList.toggle('expanded', expanded);
}

async function resizeWindowForMiniBadge(expanded) {
  if (invoke) {
    try {
      await invoke('set_mini_badge_window', { expanded });
      return;
    } catch (e) {
      console.warn('[MiniBadge] Native resize command failed, falling back:', e);
    }
  }

  if (!getCurrentWindow) return;

  try {
    const win = getCurrentWindow();

    if (settings.miniBadgeMode && !expanded) {
      await win.setResizable?.(false);
      await win.setShadow?.(false);
      await win.setMinSize(MINI_BADGE_SIZE);
      await win.setSize(MINI_BADGE_SIZE);
      await win.setMaxSize(MINI_BADGE_SIZE);
      return;
    }

    await win.setMaxSize(null);
    await win.setMinSize(PANEL_MIN_SIZE);
    await win.setResizable?.(true);
    await win.setShadow?.(false);
    await win.setSize(PANEL_SIZE);
  } catch (e) {
    console.warn(e);
  }
}

// --- Mini Badge Mode ---
function setupMiniBadge() {
  const miniBadge = document.getElementById('mini-badge');
  const app = document.getElementById('app');

  if (!miniBadge || !app) return;

  let collapseTimer = null;

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
  }

  async function collapseMiniBadge() {
    if (!settings.miniBadgeMode) return;
    clearCollapseTimer();
    setMiniBadgeExpanded(false);
    await resizeWindowForMiniBadge(false);
  }

  function scheduleCollapse() {
    if (!settings.miniBadgeMode || !document.body.classList.contains('expanded')) return;
    clearCollapseTimer();
    collapseTimer = setTimeout(collapseMiniBadge, 300);
  }

  miniBadge.addEventListener('mouseenter', expandMiniBadge);
  app.addEventListener('mouseleave', scheduleCollapse);
  app.addEventListener('mouseenter', clearCollapseTimer);
  document.addEventListener('mouseleave', scheduleCollapse);
  window.addEventListener('blur', scheduleCollapse);
}

function updateMiniBadge(snapshot) {
  if (!snapshot || snapshot.error) return;

  const percentEl = document.getElementById('mini-badge-percent');
  const labelEl = document.getElementById('mini-badge-label');
  const indicatorEl = document.getElementById('mini-badge-indicator');

  if (!percentEl || !labelEl || !indicatorEl) return;

  // Calculate all three usage percentages
  const rollingPct = snapshot.usage?.rolling?.usage_percent ?? 0;
  const weeklyPct = snapshot.usage?.weekly?.usage_percent ?? 0;
  const monthlyPct = snapshot.usage?.monthly?.usage_percent ?? 0;

  // Use the maximum of the three percentages
  const percentage = Math.max(rollingPct, weeklyPct, monthlyPct);

  percentEl.textContent = percentage + '%';

  // Update label based on which period has the max usage
  if (percentage === monthlyPct && percentage > weeklyPct) {
    labelEl.textContent = 'Monthly';
  } else if (percentage === weeklyPct && percentage > rollingPct) {
    labelEl.textContent = 'Weekly';
  } else {
    labelEl.textContent = 'Rolling';
  }

  // Update indicator color based on usage
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
  console.log('[Refresh] Triggering refresh...');
  try {
    if (!invoke) return;
    await invoke('refresh_now');
    console.log('[Refresh] Refresh triggered');
  } catch (e) {
    console.error('[Refresh] Refresh failed:', e);
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
  updateFooter(snapshot);
  updateMiniBadge(snapshot);
  renderUsageTab(snapshot, settings);
  renderModelsTab(snapshot);
  renderSettingsTab(snapshot, settings, settingActions, isPinned);
  loadWorkspaces();
}

function updateFooter(snapshot) {
  const footer = document.getElementById('footer-time');
  if (snapshot.error && snapshot.error.includes('Not logged in')) {
    footer.textContent = 'Not logged in';
  } else if (snapshot.error) {
    footer.textContent = 'Update failed';
  } else {
    footer.textContent = 'Updated ' + formatTimeAgo(snapshot.last_updated);
  }
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
    try {
      console.log('[Workspace] Switching to:', wid);
      await invoke('switch_workspace', { workspaceId: wid });
      console.log('[Workspace] Switched OK, refreshing...');
      // Re-render to show updated data (snapshot is refreshed by backend)
      await new Promise(r => setTimeout(r, 500));
      await renderAll();
    } catch (e) {
      console.error('[Workspace] Switch failed:', e);
    }
  });
}

// --- Tab Switching ---
function switchTab(name) {
  currentTab = name;
  document.querySelectorAll('.tab').forEach(t => t.classList.toggle('active', t.dataset.tab === name));
  document.querySelectorAll('.tab-panel').forEach(p => p.classList.toggle('active', p.id === 'tab-' + name));
  if (name === 'settings') renderSettingsTab(latestSnapshot, settings, settingActions, isPinned);
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

    // Add F12 for dev tools in debug mode
    document.addEventListener('keydown', async (e) => {
      if (e.key === 'F12') {
        console.log('[DevTools] F12 pressed, attempting to toggle devtools...');
        try {
          if (getCurrentWebviewWindow) {
            await getCurrentWebviewWindow().toggleDevtools();
            console.log('[DevTools] Devtools toggled');
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
