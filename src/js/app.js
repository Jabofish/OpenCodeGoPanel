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

const settingActions = {
  setPinned: async (value) => setPinned(value),
  setAutoRefresh: (value) => updateSettings({ autoRefresh: value }),
  setCompactMode: (value) => updateSettings({ compactMode: value }),
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
      ...JSON.parse(localStorage.getItem('ocp-settings') || '{}'),
    };
  } catch (_) {
    return { autoRefresh: true, compactMode: true };
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
  document.body.classList.toggle('compact-mode', settings.compactMode);
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
  renderUsageTab(snapshot);
  renderModelsTab(snapshot);
  renderSettingsTab(snapshot, settings, settingActions, isPinned);
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
    console.log('[Init] Window controls setup complete');
    document.getElementById('btn-pin')?.classList.toggle('active', isPinned);
    applyUiSettings();

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
