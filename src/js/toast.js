/**
 * Toast notification system - replaces alert() and provides non-blocking feedback.
 */

let toastContainer = null;
let toastIdCounter = 0;

function ensureContainer() {
  if (!toastContainer) {
    toastContainer = document.createElement('div');
    toastContainer.id = 'toast-container';
    document.body.appendChild(toastContainer);
  }
  return toastContainer;
}

/**
 * Show a toast message.
 * @param {string} message - The message to display
 * @param {object} options - { type: 'info'|'success'|'error'|'warning', duration: ms }
 */
export function showToast(message, options = {}) {
  const { type = 'info', duration = 3000 } = options;
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
