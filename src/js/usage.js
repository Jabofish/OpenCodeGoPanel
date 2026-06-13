import { formatCountdown, escapeHtml } from './format.js';

export function renderUsageTab(snapshot) {
  const container = document.getElementById('tab-usage');
  if (!container) return;

  const u = snapshot.usage;

  let html = '';

  // Workspace banner
  html += '<div class="workspace-banner">';
  html += '<span>Workspace <strong>' + escapeHtml(snapshot.workspace_id || 'Not set') + '</strong></span>';
  html += '<span>OpenCode Go</span>';
  html += '</div>';

  // Error banner
  if (snapshot.error && !snapshot.error.includes('Not yet loaded')) {
    html += '<div class="error-banner">' + escapeHtml(snapshot.error);
    if (snapshot.error.includes('expired') || snapshot.error.includes('Not logged')) {
      html += '<div class="error-cta"><a id="btn-login">Click here to log in</a></div>';
    }
    html += '</div>';
  }

  // Rolling usage
  html += buildPeriodCard('Rolling Usage', u.rolling, 'bar-rolling', 'color-rolling');
  // Weekly
  html += buildPeriodCard('Weekly Usage', u.weekly, 'bar-weekly', 'color-weekly');
  // Monthly
  html += buildPeriodCard('Monthly Usage', u.monthly, 'bar-monthly', 'color-monthly');

  container.innerHTML = html;

  // Attach login button event
  const loginBtn = document.getElementById('btn-login');
  if (loginBtn) {
    loginBtn.addEventListener('click', async () => {
      const { invoke } = window.__TAURI__.core;
      await invoke('open_login_window');
    });
  }
}

function buildPeriodCard(label, period, barClass, colorClass) {
  if (!period) return '';

  const pct = period.usage_percent;
  const resetIn = formatCountdown(period.reset_in_sec);

  return '' +
    '<div class="card">' +
      '<div class="card-header">' +
        '<span class="card-label">' + label + '</span>' +
        '<span class="card-reset">Resets in <strong>' + resetIn + '</strong></span>' +
      '</div>' +
      '<div class="card-body">' +
        '<span class="card-value" style="color: var(--' + colorClass + ')">' + pct + '%</span>' +
        '<span class="card-unit">of quota used</span>' +
      '</div>' +
      '<div class="bar">' +
        '<div class="bar-fill ' + barClass + '" style="width:' + Math.min(pct, 100) + '%"></div>' +
      '</div>' +
    '</div>';
}
