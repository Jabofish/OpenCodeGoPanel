/**
 * Login Helper - Handles login flow with auto-detect and manual fallback.
 *
 * When the user navigates to a /workspace/{id}/... page (the post-login
 * redirect target), the module waits a short debounce (800 ms) for the page
 * to settle and cookies to be written, then automatically calls
 * extract_cookies_from_webview. The backend closes the window on success.
 *
 * The manual "Save login manually" button remains as a fallback in case
 * auto-detect misses. A double-save guard prevents re-triggering within the
 * same session.
 */

let autoSaveTriggered = false; // Double-save guard: once auto-save succeeds,
                               // subsequent navigations won't re-trigger.

async function setupLoginHelper() {
  const { invoke } = window.__TAURI__?.core || {};

  if (!invoke) {
    console.error('[LoginHelper] Tauri API not available');
    return;
  }

  // Add global button for manual extraction (fallback)
  addExtractButton();

  // Add a status line element for auto-save feedback
  addStatusLine();

  // Monitor URL changes to detect workspace page and trigger auto-save
  monitorWorkspaceNavigation();
}

function addExtractButton() {
  const button = document.createElement('button');
  button.id = 'extract-cookies-btn';
  button.textContent = 'Save login manually';
  button.style.cssText = `
    position: fixed;
    bottom: 20px;
    right: 20px;
    padding: 10px 18px;
    background: rgba(102, 126, 234, 0.45);
    color: rgba(255, 255, 255, 0.7);
    border: none;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
    z-index: 999999;
    transition: all 0.3s ease;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  `;

  button.addEventListener('mouseenter', () => {
    button.style.background = 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)';
    button.style.color = 'white';
    button.style.transform = 'translateY(-2px)';
    button.style.boxShadow = '0 6px 16px rgba(102, 126, 234, 0.5)';
  });

  button.addEventListener('mouseleave', () => {
    if (!button.dataset.saving) {
      button.style.background = 'rgba(102, 126, 234, 0.45)';
      button.style.color = 'rgba(255, 255, 255, 0.7)';
      button.style.transform = 'translateY(0)';
      button.style.boxShadow = '0 2px 8px rgba(0, 0, 0, 0.2)';
    }
  });

  button.addEventListener('click', async () => {
    await extractAndSave(button);
  });

  document.body.appendChild(button);
  console.log('[LoginHelper] Manual save button added');
}

function addStatusLine() {
  const status = document.createElement('div');
  status.id = 'login-status-line';
  status.style.cssText = `
    position: fixed;
    bottom: 60px;
    right: 20px;
    padding: 4px 10px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.6);
    background: rgba(20, 21, 28, 0.85);
    border-radius: 4px;
    z-index: 999998;
    opacity: 0;
    transition: opacity 0.3s ease;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    pointer-events: none;
  `;
  document.body.appendChild(status);
}

function setStatusLine(text, color) {
  const status = document.getElementById('login-status-line');
  if (!status) return;
  status.textContent = text;
  status.style.color = color || 'rgba(255, 255, 255, 0.6)';
  status.style.opacity = text ? '1' : '0';
}

/**
 * Core cookie extraction logic shared by both auto-save and manual save.
 * Returns { success: boolean, error?: string }.
 */
async function doExtractCookies() {
  const { invoke } = window.__TAURI__?.core || {};

  // 1. Extract visible cookies
  const cookies = document.cookie.split('; ')
    .filter(c => c.includes('='))
    .map(c => {
      const [name, ...valueParts] = c.split('=');
      return {
        name: name.trim(),
        value: valueParts.join('=').trim()
      };
    })
    .filter(c => c.name && c.value);

  console.log('[LoginHelper] Extracted visible cookies:', cookies.length);

  // 2. Extract workspace_id from URL or localStorage
  let workspaceId = '';

  const pathMatch = window.location.pathname.match(/workspace\/([a-zA-Z0-9_-]+)/);
  if (pathMatch) {
    workspaceId = pathMatch[1];
    console.log('[LoginHelper] Found workspace_id in URL:', workspaceId);
  } else {
    workspaceId = localStorage.getItem('workspace_id') ||
                  localStorage.getItem('workspaceId') || '';
    console.log('[LoginHelper] Found workspace_id in localStorage:', workspaceId);
  }

  // 3. Try to find workspace_id in page content if still empty
  if (!workspaceId) {
    const bodyText = document.body.innerText;
    const contentMatch = bodyText.match(/workspace[:\s]+([a-zA-Z0-9_-]+)/i);
    if (contentMatch) {
      workspaceId = contentMatch[1];
      console.log('[LoginHelper] Found workspace_id in page content:', workspaceId);
    }
  }

  if (!workspaceId) {
    return { success: false, error: 'Workspace ID not found. Navigate to your workspace page first.' };
  }

  const cookiesJson = JSON.stringify(cookies);

  console.log('[LoginHelper] Calling extract_cookies_from_webview...');
  const success = await invoke('extract_cookies_from_webview', {
    cookiesJson: cookiesJson,
    workspaceId: workspaceId
  });

  if (success) {
    console.log('[LoginHelper] Login saved successfully for workspace:', workspaceId);
    return { success: true };
  } else {
    return { success: false, error: 'Extraction returned false' };
  }
}

async function extractAndSave(button) {
  const originalText = button.textContent;
  button.textContent = 'Saving...';
  button.disabled = true;
  button.style.opacity = '0.7';
  button.dataset.saving = 'true';

  try {
    const result = await doExtractCookies();

    if (result.success) {
      button.textContent = 'Saved!';
      button.style.background = 'linear-gradient(135deg, #11998e 0%, #38ef7d 100%)';
      button.style.color = 'white';
      autoSaveTriggered = true; // Mark as saved so auto-detect won't re-trigger

      console.log('[LoginHelper] Login saved, window will close shortly');

      setTimeout(() => {
        button.textContent = 'Closing...';
      }, 1500);
    } else {
      throw new Error(result.error);
    }
  } catch (error) {
    console.error('[LoginHelper] Failed to save login:', error);

    button.textContent = 'Failed';
    button.style.background = 'linear-gradient(135deg, #eb3349 0%, #f45c43 100%)';
    button.style.color = 'white';
    console.error('[LoginHelper] Error:', error);

    setTimeout(() => {
      button.textContent = originalText;
      button.style.background = 'rgba(102, 126, 234, 0.45)';
      button.style.color = 'rgba(255, 255, 255, 0.7)';
      button.disabled = false;
      button.style.opacity = '1';
      delete button.dataset.saving;
    }, 3000);
  }
}

/**
 * Attempt an automatic save when workspace navigation is detected.
 * Debounced (800 ms) to let the page settle and cookies to be written.
 * Guarded by autoSaveTriggered to prevent double-save.
 */
async function attemptAutoSave() {
  if (autoSaveTriggered) return;
  autoSaveTriggered = true; // Set early to prevent re-entry during debounce

  console.log('[LoginHelper] Auto-save triggered — waiting 800ms for page to settle...');
  setStatusLine('Login detected \u2014 saving\u2026', 'rgba(160, 176, 224, 0.9)');

  // Debounce: wait for cookies and page state to stabilize
  await new Promise(resolve => setTimeout(resolve, 800));

  try {
    const result = await doExtractCookies();

    if (result.success) {
      setStatusLine('Saved \u2713', 'rgba(95, 207, 151, 0.9)');

      // Update the manual button to reflect success
      const btn = document.getElementById('extract-cookies-btn');
      if (btn) {
        btn.textContent = 'Saved!';
        btn.style.background = 'linear-gradient(135deg, #11998e 0%, #38ef7d 100%)';
        btn.style.color = 'white';
        btn.disabled = true;
      }

      console.log('[LoginHelper] Auto-save complete, window will close');
      // The backend closes the window via auth-state-changed event.
    } else {
      // Auto-save failed — reset the guard so the user can try manually
      autoSaveTriggered = false;
      setStatusLine('Auto-save failed \u2014 use the button below', 'rgba(224, 97, 112, 0.9)');
      console.warn('[LoginHelper] Auto-save failed:', result.error);

      // Clear status after 4s
      setTimeout(() => setStatusLine(''), 4000);
    }
  } catch (error) {
    autoSaveTriggered = false;
    setStatusLine('Auto-save failed \u2014 use the button below', 'rgba(224, 97, 112, 0.9)');
    console.error('[LoginHelper] Auto-save error:', error);

    setTimeout(() => setStatusLine(''), 4000);
  }
}

function monitorWorkspaceNavigation() {
  let autoSaveTimer = null;

  function checkWorkspacePage() {
    const isWorkspacePage = window.location.pathname.includes('/workspace/');
    const button = document.getElementById('extract-cookies-btn');

    if (isWorkspacePage && !autoSaveTriggered) {
      // Highlight the manual button (workspace page detected)
      if (button) {
        button.style.animation = 'pulse 2s infinite';
        button.title = 'Click to save your login credentials manually';

        if (!document.getElementById('pulse-style')) {
          const style = document.createElement('style');
          style.id = 'pulse-style';
          style.textContent = `
            @keyframes pulse {
              0%, 100% { box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2); }
              50% { box-shadow: 0 4px 16px rgba(102, 126, 234, 0.6); }
            }
          `;
          document.head.appendChild(style);
        }
      }

      // Schedule auto-save with debounce. Clear any pending timer first
      // (SPA might fire multiple navigation events during redirect).
      clearTimeout(autoSaveTimer);
      autoSaveTimer = setTimeout(() => {
        attemptAutoSave();
      }, 800);
    } else if (!isWorkspacePage) {
      // Not on workspace page — reset button animation
      if (button) {
        button.style.animation = 'none';
        button.title = 'Navigate to your workspace page first';
      }
      clearTimeout(autoSaveTimer);
    }
  }

  // Initial check
  checkWorkspacePage();

  // Monitor URL changes (SPA navigation)
  let lastUrl = location.href;
  new MutationObserver(() => {
    const url = location.href;
    if (url !== lastUrl) {
      lastUrl = url;
      checkWorkspacePage();
      console.log('[LoginHelper] URL changed:', url);
    }
  }).observe(document, { subtree: true, childList: true });

  // Also listen to popstate for history navigation
  window.addEventListener('popstate', checkWorkspacePage);
}

// Auto-initialize when script loads
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', setupLoginHelper);
} else {
  setupLoginHelper();
}

console.log('[LoginHelper] Login helper module loaded (auto-detect enabled)');
