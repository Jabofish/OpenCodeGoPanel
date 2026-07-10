/**
 * Account switcher helpers. Account data lives in the backend; this module
 * renders the titlebar selector and forwards switch requests.
 */

let accountSelectRenderKey = '';

export async function refreshAccounts() {
  if (!window.__TAURI__?.core?.invoke) return [];
  try {
    const accounts = await window.__TAURI__.core.invoke('list_accounts');
    return Array.isArray(accounts) ? accounts : [];
  } catch (e) {
    console.warn('[Accounts] list_accounts failed:', e);
    return [];
  }
}

export function renderAccountSelector(accounts, activeId) {
  const sel = document.getElementById('account-selector');
  if (!sel) return;

  const safeAccounts = Array.isArray(accounts) ? accounts : [];
  const key = safeAccounts
    .map(a => [a.id, a.displayName || '', a.lastUsedAt || ''].join(':'))
    .join('|') + '::' + (activeId || '');
  if (key === accountSelectRenderKey) return;
  accountSelectRenderKey = key;

  sel.innerHTML = '';
  sel.dataset.activeId = activeId || '';
  if (safeAccounts.length === 0) {
    sel.style.display = 'none';
    return;
  }

  sel.style.display = '';
  for (const account of safeAccounts) {
    const opt = document.createElement('option');
    opt.value = account.id;
    opt.textContent = account.displayName || account.id;
    opt.selected = account.id === activeId;
    sel.appendChild(opt);
  }
}

export function resetAccountSelectorRenderKey() {
  accountSelectRenderKey = '';
}

export function setupAccountSelector(onSwitched) {
  const sel = document.getElementById('account-selector');
  if (!sel) return;
  sel.addEventListener('change', async () => {
    const accountId = sel.value;
    if (!accountId || !window.__TAURI__?.core?.invoke) return;
    const previousId = sel.dataset.activeId || '';
    const footer = document.getElementById('footer-time');
    if (footer) footer.textContent = 'Switching account...';
    try {
      await window.__TAURI__.core.invoke('switch_account', { accountId });
      sel.dataset.activeId = accountId;
      if (onSwitched) await onSwitched();
    } catch (e) {
      console.error('[Accounts] switch failed:', e);
      if (previousId) sel.value = previousId;
      if (footer) footer.textContent = 'Account switch failed';
      if (onSwitched) await onSwitched({ failed: true });
    }
  });
}
