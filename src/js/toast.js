/**
 * Toast notification system - replaces alert() and provides non-blocking feedback.
 * In collapsed badge mode, falls back to OS system notifications.
 */

const { getCurrentWindow } = window.__TAURI__?.window || {};

let toastContainer = null;
let toastIdCounter = 0;
let badgeBubbleActive = false;
let badgeBubbleTimer = null;
let badgeBubbleSavedPos = null;

function ensureContainer() {
  if (!toastContainer) {
    toastContainer = document.createElement('div');
    toastContainer.id = 'toast-container';
    document.body.appendChild(toastContainer);
  }
  return toastContainer;
}

/**
 * Check if the window is in collapsed badge mode (inline UI not visible).
 */
function isInBadgeMode() {
  return document.body.classList.contains('mini-badge-mode') &&
    !document.body.classList.contains('expanded');
}

/**
 * Check if the badge bubble is currently being shown.
 */
export function isBadgeBubbleActive() {
  return badgeBubbleActive;
}

/**
 * Hide the badge bubble and restore the badge window to its original size/position.
 */
export function hideBadgeBubble() {
  if (!badgeBubbleActive) return;
  badgeBubbleActive = false;
  if (badgeBubbleTimer) {
    clearTimeout(badgeBubbleTimer);
    badgeBubbleTimer = null;
  }
  const bubble = document.getElementById('badge-bubble');
  if (bubble) bubble.remove();
  const win = getCurrentWindow?.();
  if (win) {
    const dpi = window.__TAURI__?.dpi;
    const badgeSize = document.body.classList.contains('badge-dot')
      ? { width: 28, height: 28 }
      : document.body.classList.contains('badge-ring')
        ? { width: 76, height: 76 }
        : { width: 60, height: 60 };
    const sizeArg = dpi?.LogicalSize
      ? new dpi.LogicalSize(badgeSize.width, badgeSize.height)
      : { type: 'Logical', data: badgeSize };
    win.setMaxSize?.(null);
    win.setMinSize?.(sizeArg);
    win.setSize?.(sizeArg);
    win.setMaxSize?.(sizeArg);
    if (badgeBubbleSavedPos) {
      const posArg = dpi?.LogicalPosition
        ? new dpi.LogicalPosition(badgeBubbleSavedPos.x, badgeBubbleSavedPos.y)
        : { type: 'Logical', data: badgeBubbleSavedPos };
      win.setPosition?.(posArg);
    }
  }
  document.documentElement.classList.remove('badge-bubble-active');
  document.body.classList.remove('badge-bubble-active');
}

/**
 * Show a bubble notification by temporarily expanding the badge window.
 * The badge stays visible; a chat-like bubble pops out to its right.
 */
async function showBadgeBubble(message, type) {
  if (!getCurrentWindow) return;

  // Remove any existing bubble first
  hideBadgeBubble();

  badgeBubbleActive = true;
  const win = getCurrentWindow();
  const dpi = window.__TAURI__?.dpi;
  const scaleFactor = await win.scaleFactor().catch(() => 1);

  // Determine bubble left offset based on badge type (matches CSS left values)
  const isDot = document.body.classList.contains('badge-dot');
  const isRing = document.body.classList.contains('badge-ring');
  const bubbleLeft = isDot ? 40 : isRing ? 92 : 72;
  const BUBBLE_MAX_W = 180;
  const WIN_W = bubbleLeft + BUBBLE_MAX_W + 12;

  try {
    // Save current position (logical)
    const physPos = await win.innerPosition();
    badgeBubbleSavedPos = {
      x: Math.round(physPos.x / scaleFactor),
      y: Math.round(physPos.y / scaleFactor),
    };

    // Get monitor bounds for edge detection
    let monW = 1920, monH = 1080; // fallback
    if (win.currentMonitor) {
      try {
        const mon = await win.currentMonitor();
        if (mon && mon.size) {
          monW = mon.size.width;
          monH = mon.size.height;
        }
      } catch (e) {
        console.warn('[BadgeBubble] currentMonitor failed:', e);
      }
    }

    // Temporarily override CSS size constraints on both html and body
    document.documentElement.classList.add('badge-bubble-active');
    document.body.classList.add('badge-bubble-active');

    // Create the bubble first (hidden) so we can measure its natural height
    // and size the window to fit the wrapped text. A fixed window height
    // clipped long messages; this lets multi-line errors show in full.
    const bubble = document.createElement('div');
    bubble.id = 'badge-bubble';
    bubble.className = 'badge-bubble badge-bubble-' + type;
    bubble.textContent = message;
    document.body.appendChild(bubble);

    // offsetHeight reflects layout height (transform/opacity don't affect it)
    const bubbleH = bubble.offsetHeight || 40;
    // Window height = bubble height + vertical padding/margin, clamped so a
    // runaway error string can't blow the window up too tall.
    const WIN_H = Math.max(70, Math.min(bubbleH + 20, 200));

    // Resize window: badge on left, bubble on right
    // Follow the same pattern as resizeWindowForMiniBadge (no setResizable)
    win.setMaxSize?.(null);
    const sizeArg = dpi?.LogicalSize
      ? new dpi.LogicalSize(WIN_W, WIN_H)
      : { type: 'Logical', data: { width: WIN_W, height: WIN_H } };
    await win.setSize(sizeArg);

    // Adjust position if near screen edge (all in physical pixels)
    const physSize = await win.innerSize();
    let newX = badgeBubbleSavedPos.x;
    let newY = badgeBubbleSavedPos.y;

    if (newX * scaleFactor + physSize.width > monW) {
      newX = Math.round((monW - physSize.width) / scaleFactor);
    }
    if (newY * scaleFactor + physSize.height > monH) {
      newY = Math.round((monH - physSize.height) / scaleFactor);
    }
    if (newX !== badgeBubbleSavedPos.x || newY !== badgeBubbleSavedPos.y) {
      if (dpi?.LogicalPosition) {
        await win.setPosition(new dpi.LogicalPosition(newX, newY));
      } else {
        await win.setPosition({ type: 'Logical', data: { x: newX, y: newY } });
      }
    }

    // Trigger animation
    setTimeout(() => bubble.classList.add('badge-bubble-show'), 10);

    // Auto-dismiss
    badgeBubbleTimer = setTimeout(() => {
      bubble.classList.remove('badge-bubble-show');
      bubble.classList.add('badge-bubble-hide');
      setTimeout(() => hideBadgeBubble(), 300);
    }, 3500);
  } catch (e) {
    console.warn('[BadgeBubble] Failed:', e);
    hideBadgeBubble();
  }
}

/**
 * Show a toast message. In badge mode, shows a bubble notification instead.
 * @param {string} message - The message to display
 * @param {object} options - { type: 'info'|'success'|'error'|'warning', duration: ms }
 */
export function showToast(message, options = {}) {
  const { type = 'info', duration = 3000 } = options;

  // Badge mode: show bubble notification instead of inline toast
  if (isInBadgeMode()) {
    showBadgeBubble(message, type);
    return null;
  }

  const container = ensureContainer();

  const id = 'toast-' + (++toastIdCounter);
  const toast = document.createElement('div');
  toast.id = id;
  toast.className = 'toast toast-' + type;
  toast.textContent = message;

  container.appendChild(toast);

  // Trigger animation
  setTimeout(() => toast.classList.add('toast-show'), 10);

  // Auto-dismiss
  if (duration > 0) {
    setTimeout(() => dismissToast(id), duration);
  }

  return id;
}

/**
 * Dismiss a toast by ID.
 */
export function dismissToast(id) {
  const toast = document.getElementById(id);
  if (!toast) return;

  toast.classList.remove('toast-show');
  toast.classList.add('toast-hide');

  setTimeout(() => {
    if (toast.parentNode) {
      toast.parentNode.removeChild(toast);
    }
  }, 300);
}

/**
 * Dismiss all active toasts immediately (no animation). Call when collapsing badge.
 */
export function dismissAllToasts() {
  const container = document.getElementById('toast-container');
  if (!container) return;
  while (container.firstChild) {
    container.removeChild(container.firstChild);
  }
}

/**
 * Show a confirmation dialog with custom UI (replaces window.confirm).
 * @param {string} message - The confirmation message
 * @param {object} options - { title, confirmText, cancelText }
 * @returns {Promise<boolean>} - true if confirmed, false if cancelled
 */
export function showConfirm(message, options = {}) {
  const {
    title = 'Confirm',
    confirmText = 'OK',
    cancelText = 'Cancel'
  } = options;

  return new Promise((resolve) => {
    const overlay = document.createElement('div');
    overlay.className = 'confirm-overlay';

    const dialog = document.createElement('div');
    dialog.className = 'confirm-dialog';

    dialog.innerHTML = `
      <div class="confirm-title">${escapeHtml(title)}</div>
      <div class="confirm-message">${escapeHtml(message)}</div>
      <div class="confirm-buttons">
        <button class="confirm-btn confirm-cancel">${escapeHtml(cancelText)}</button>
        <button class="confirm-btn confirm-ok">${escapeHtml(confirmText)}</button>
      </div>
    `;

    overlay.appendChild(dialog);
    document.body.appendChild(overlay);

    const cleanup = () => {
      overlay.classList.add('confirm-hide');
      setTimeout(() => {
        if (overlay.parentNode) {
          overlay.parentNode.removeChild(overlay);
        }
      }, 200);
    };

    const okBtn = dialog.querySelector('.confirm-ok');
    const cancelBtn = dialog.querySelector('.confirm-cancel');

    okBtn.addEventListener('click', () => {
      cleanup();
      resolve(true);
    });

    cancelBtn.addEventListener('click', () => {
      cleanup();
      resolve(false);
    });

    overlay.addEventListener('click', (e) => {
      if (e.target === overlay) {
        cleanup();
        resolve(false);
      }
    });

    // Show animation
    setTimeout(() => overlay.classList.add('confirm-show'), 10);

    // Focus OK button
    setTimeout(() => okBtn.focus(), 100);
  });
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
