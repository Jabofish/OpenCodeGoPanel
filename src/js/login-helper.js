/**
 * Login Helper - Handles manual login flow
 */

async function setupLoginHelper() {
  const { invoke } = window.__TAURI__?.core || {};

  if (!invoke) {
    console.error('[LoginHelper] Tauri API not available');
    return;
  }

  // Add global button to extract cookies
  addExtractButton();

  // Monitor URL changes to detect workspace page
  monitorWorkspaceNavigation();
}

function addExtractButton() {
  // Create a floating button for manual extraction
  const button = document.createElement('button');
  button.id = 'extract-cookies-btn';
  button.textContent = 'Save Login';
  button.style.cssText = `
    position: fixed;
    bottom: 20px;
    right: 20px;
    padding: 12px 24px;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: white;
    border: none;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 4px 12px rgba(102, 126, 234, 0.4);
    z-index: 999999;
    transition: all 0.3s ease;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  `;

  button.addEventListener('mouseenter', () => {
    button.style.transform = 'translateY(-2px)';
    button.style.boxShadow = '0 6px 16px rgba(102, 126, 234, 0.5)';
  });

  button.addEventListener('mouseleave', () => {
    button.style.transform = 'translateY(0)';
    button.style.boxShadow = '0 4px 12px rgba(102, 126, 234, 0.4)';
  });

  button.addEventListener('click', async () => {
    await extractAndSave(button);
  });

  document.body.appendChild(button);
  console.log('[LoginHelper] Extract button added');
}

async function extractAndSave(button) {
  const { invoke } = window.__TAURI__?.core || {};

  if (!invoke) {
    alert('Tauri API not available');
    return;
  }

  // Show loading state
  const originalText = button.textContent;
  button.textContent = 'Saving...';
  button.disabled = true;
  button.style.opacity = '0.7';

  try {
    console.log('[LoginHelper] Extracting workspace_id and visible cookies...');

    // 1. Extract cookies
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

    // Try URL path: /workspace/xxx
    const pathMatch = window.location.pathname.match(/workspace\/([a-zA-Z0-9_-]+)/);
    if (pathMatch) {
      workspaceId = pathMatch[1];
      console.log('[LoginHelper] Found workspace_id in URL:', workspaceId);
    } else {
      // Try localStorage
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
      throw new Error('Workspace ID not found. Please navigate to your workspace page (e.g., /workspace/xxx/go).');
    }

    // Convert visible cookies to JSON. The backend will read the full WebView
    // cookie store, including HttpOnly cookies that document.cookie cannot see.
    const cookiesJson = JSON.stringify(cookies);

    console.log('[LoginHelper] Calling extract_cookies_from_webview...');
    const success = await invoke('extract_cookies_from_webview', {
      cookiesJson: cookiesJson,
      workspaceId: workspaceId
    });

    if (success) {
      // Show success state
      button.textContent = 'Saved!';
      button.style.background = 'linear-gradient(135deg, #11998e 0%, #38ef7d 100%)';

      console.log('[LoginHelper] Login saved successfully, window will close in 2s');

      // Window will be closed by the backend
      setTimeout(() => {
        button.textContent = 'Closing...';
      }, 1500);
    } else {
      throw new Error('Extraction returned false');
    }
  } catch (error) {
    console.error('[LoginHelper] Failed to save login:', error);

    // Show error state
    button.textContent = 'Failed';
    button.style.background = 'linear-gradient(135deg, #eb3349 0%, #f45c43 100%)';

    // Show user-friendly error message
    alert('Failed to save login: ' + error.toString());

    // Reset button after 3s
    setTimeout(() => {
      button.textContent = originalText;
      button.style.background = 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)';
      button.disabled = false;
      button.style.opacity = '1';
    }, 3000);
  }
}

function monitorWorkspaceNavigation() {
  // Check if we're on a workspace page
  function checkWorkspacePage() {
    const isWorkspacePage = window.location.pathname.includes('/workspace/');
    const button = document.getElementById('extract-cookies-btn');

    if (button) {
      if (isWorkspacePage) {
        // Highlight button on workspace page
        button.style.animation = 'pulse 2s infinite';
        button.title = 'Click to save your login credentials';

        // Add pulse animation if not exists
        if (!document.getElementById('pulse-style')) {
          const style = document.createElement('style');
          style.id = 'pulse-style';
          style.textContent = `
            @keyframes pulse {
              0%, 100% { box-shadow: 0 4px 12px rgba(102, 126, 234, 0.4); }
              50% { box-shadow: 0 6px 20px rgba(102, 126, 234, 0.8); }
            }
          `;
          document.head.appendChild(style);
        }
      } else {
        button.style.animation = 'none';
        button.title = 'Navigate to your workspace page first';
      }
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

console.log('[LoginHelper] Login helper module loaded');
