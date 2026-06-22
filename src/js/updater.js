// updater.js — Auto-update dialog UI and event handling
// Listens for 'update-status' events from the Rust backend and shows
// a custom overlay dialog with progress tracking.

import { showToast, dismissToast } from './toast.js';

const { invoke } = window.__TAURI__?.core || {};
const { listen } = window.__TAURI__?.event || {};

let updateListenerUnlisten = null;
let currentDialogVersion = '';
let pendingUpdateInfo = null;
let checkingToastId = null;
let updateOverlayActive = false;

// Download dialog state — tracks whether the progress dialog DOM has been
// built so subsequent progress events patch in-place instead of rebuilding.
let downloadDialogBuilt = false;

/**
 * Initialize the updater event listener. Call once at app startup.
 */
export function initUpdater() {
  if (!listen) {
    console.warn('[Updater] Tauri event API not available');
    return;
  }

  listen('update-status', (event) => {
    const payload = event.payload;
    if (!payload) return;

    switch (payload.status) {
      case 'checking':
        if (canShowInlineUI() && typeof showToast === 'function') {
          checkingToastId = showToast('Checking for updates...', { type: 'info', duration: 0 });
        }
        break;
      case 'available':
        if (checkingToastId) { dismissToast(checkingToastId); checkingToastId = null; }
        if (!canShowInlineUI()) {
          // Badge is collapsed (tiny window): defer dialog until user expands
          pendingUpdateInfo = payload.info;
        } else {
          showUpdateDialog(payload.info);
        }
        break;
      case 'downloading':
        if (canShowInlineUI()) showDownloadProgress(payload.progress, payload.total);
        break;
      case 'downloaded':
        // The downloaded event is the single source of truth for the
        // install-ready transition. startDownload() no longer calls
        // showInstallReady() directly, so there is no double-render.
        downloadDialogBuilt = false;
        if (canShowInlineUI()) showInstallReady();
        break;
      case 'up-to-date':
        if (checkingToastId) { dismissToast(checkingToastId); checkingToastId = null; }
        break;
      case 'installing':
        // App is about to restart
        break;
      case 'error':
        if (checkingToastId) { dismissToast(checkingToastId); checkingToastId = null; }
        downloadDialogBuilt = false;
        hideDialog();
        if (typeof showToast === 'function') {
          showToast(payload.message, { type: 'error' });
        }
        break;
    }
  }).then((unlisten) => {
    updateListenerUnlisten = unlisten;
  });

  console.log('[Updater] Event listener registered');
}

/**
 * Manually trigger an update check (from Settings UI).
 */
export async function checkForUpdateManually() {
  try {
    const info = await invoke('check_for_update');
    if (!info) {
      if (typeof showToast === 'function') {
        showToast('You are up to date', { type: 'success' });
      }
    }
    // If info is returned, the event listener handles showing the dialog
  } catch (_e) {
    // Error toast is already shown by the event listener — no duplicate here
  }
}

function showUpdateDialog(info) {
  currentDialogVersion = info?.version || '';
  let overlay = document.getElementById('update-overlay');
  if (!overlay) {
    overlay = document.createElement('div');
    overlay.id = 'update-overlay';
    document.body.appendChild(overlay);
  }

  const version = info?.version || 'unknown';
  const notes = info?.notes || 'A new version is available.';

  overlay.innerHTML = '' +
    '<div class="update-dialog">' +
      '<div class="update-dialog-header">' +
        '<span class="update-dialog-icon">&#x2B06;</span>' +
        '<span>Update Available</span>' +
      '</div>' +
      '<div class="update-dialog-body">' +
        '<div class="update-version">v' + escapeHtml(version) + '</div>' +
        '<div class="update-notes">' + escapeHtml(notes) + '</div>' +
      '</div>' +
      '<div class="update-dialog-actions">' +
        '<button id="update-btn-now" class="update-btn update-btn-primary">Update now</button>' +
        '<button id="update-btn-skip" class="update-btn update-btn-secondary">Skip this version</button>' +
        '<button id="update-btn-later" class="update-btn update-btn-tertiary">Remind later</button>' +
      '</div>' +
    '</div>';

  overlay.classList.add('visible');
  updateOverlayActive = true;

  document.getElementById('update-btn-now').addEventListener('click', startDownload);
  document.getElementById('update-btn-skip').addEventListener('click', () => skipVersion(version));
  document.getElementById('update-btn-later').addEventListener('click', hideDialog);
}

/**
 * Show an immediate "Connecting…" state before the download actually starts.
 * Called from startDownload() before invoking the backend command so the user
 * gets instant feedback instead of a frozen button during the network handshake.
 */
function showDownloadConnecting() {
  const overlay = document.getElementById('update-overlay');
  if (!overlay) return;

  // Reset the dialog-built flag so the first progress event rebuilds the DOM
  downloadDialogBuilt = false;

  overlay.innerHTML = '' +
    '<div class="update-dialog">' +
      '<div class="update-dialog-header">' +
        '<span class="update-dialog-icon">&#x2B07;</span>' +
        '<span>Downloading Update</span>' +
      '</div>' +
      '<div class="update-dialog-body">' +
        '<div class="update-connecting-text">Connecting to download server&hellip;</div>' +
      '</div>' +
    '</div>';

  overlay.classList.add('visible');
  updateOverlayActive = true;
}

function showDownloadProgress(progress, total) {
  const overlay = document.getElementById('update-overlay');
  if (!overlay) return;

  const pct = Math.round(progress);
  const sizeStr = total ? ' / ' + formatBytes(total) : '';

  // Build the dialog DOM only once; subsequent calls patch the fill width
  // and text in-place, eliminating the flicker caused by full innerHTML
  // rewrites firing dozens of times per second.
  if (!downloadDialogBuilt) {
    overlay.innerHTML = '' +
      '<div class="update-dialog">' +
        '<div class="update-dialog-header">' +
          '<span class="update-dialog-icon">&#x2B07;</span>' +
          '<span>Downloading Update</span>' +
        '</div>' +
        '<div class="update-dialog-body">' +
          '<div class="update-progress-bar">' +
              '<div class="update-progress-fill" style="width:' + pct + '%"></div>' +
          '</div>' +
          '<div class="update-progress-text">' + pct + '%' + sizeStr + '</div>' +
        '</div>' +
      '</div>';

    overlay.classList.add('visible');
    updateOverlayActive = true;
    downloadDialogBuilt = true;
  } else {
    // In-place patch — only touch the two dynamic elements
    const fill = overlay.querySelector('.update-progress-fill');
    const text = overlay.querySelector('.update-progress-text');
    if (fill) fill.style.width = pct + '%';
    if (text) text.textContent = pct + '%' + sizeStr;
  }
}

function showInstallReady() {
  const overlay = document.getElementById('update-overlay');
  if (!overlay) return;

  overlay.innerHTML = '' +
    '<div class="update-dialog">' +
      '<div class="update-dialog-header">' +
        '<span class="update-dialog-icon">&#x2705;</span>' +
        '<span>Update Ready</span>' +
      '</div>' +
      '<div class="update-dialog-body">' +
        '<div class="update-notes">Download complete. The app will restart to install the update.</div>' +
      '</div>' +
      '<div class="update-dialog-actions">' +
        '<button id="update-btn-install" class="update-btn update-btn-primary">Install &amp; Restart</button>' +
        '<button id="update-btn-later2" class="update-btn update-btn-tertiary">Later</button>' +
      '</div>' +
    '</div>';

  overlay.classList.add('visible');
  updateOverlayActive = true;

  document.getElementById('update-btn-install').addEventListener('click', installUpdate);
  document.getElementById('update-btn-later2').addEventListener('click', hideDialog);
}

function hideDialog() {
  const overlay = document.getElementById('update-overlay');
  if (overlay) {
    overlay.classList.remove('visible');
  }
  currentDialogVersion = '';
  updateOverlayActive = false;
}

async function startDownload() {
  // Set overlay-active and show connecting state immediately so the badge
  // cannot collapse during the network handshake and the user sees instant
  // feedback instead of a frozen button.
  updateOverlayActive = true;
  showDownloadConnecting();

  try {
    await invoke('download_update');
    // Do NOT call showInstallReady() here — the 'downloaded' event fired by
    // the backend is the single source of truth for that transition. This
    // removes the double-render that previously occurred when both the event
    // handler and this function called showInstallReady().
  } catch (e) {
    downloadDialogBuilt = false;
    hideDialog();
    if (typeof showToast === 'function') {
      showToast('Download failed: ' + e, { type: 'error' });
    }
  }
}

async function installUpdate() {
  try {
    await invoke('install_update');
    // App will restart — no further action needed
  } catch (e) {
    hideDialog();
    if (typeof showToast === 'function') {
      showToast('Install failed: ' + e, { type: 'error' });
    }
  }
}

async function skipVersion(version) {
  try {
    // Save skipped version to settings via backend
    if (invoke) {
      const settings = await invoke('get_settings');
      settings.skippedUpdateVersion = version;
      await invoke('save_settings', { next: settings });
    }
  } catch (e) {
    console.warn('[Updater] Failed to save skipped version:', e);
  }
  hideDialog();
}

/**
 * Whether in-app overlays (toasts, dialogs) are readable.
 * False only when the badge is collapsed (tiny window, content clipped).
 */
function canShowInlineUI() {
  return !document.body.classList.contains('mini-badge-mode') ||
    document.body.classList.contains('expanded');
}

/**
 * Called when the mini badge expands to panel mode.
 * Shows any pending update dialog that was deferred during badge mode.
 */
export function consumePendingUpdate() {
  if (pendingUpdateInfo) {
    showUpdateDialog(pendingUpdateInfo);
    pendingUpdateInfo = null;
  }
}

/**
 * Whether an update overlay (dialog, download progress, install prompt) is currently visible.
 * Used by app.js to prevent badge auto-collapse during update interactions.
 */
export function isUpdateOverlayActive() {
  return updateOverlayActive;
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

function formatBytes(bytes) {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
}
