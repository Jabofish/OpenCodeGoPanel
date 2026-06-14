import { formatCountdown, escapeHtml } from './format.js';

export function renderUsageTab(snapshot) {
  const container = document.getElementById('tab-usage');
  if (!container) return;

  const u = snapshot.usage;
  const rolling = u.rolling || {};
  const weekly = u.weekly || {};
  const monthly = u.monthly || {};

  let html = '';

  html += '<div class="mini-header">';
  html += '<span>' + escapeHtml(snapshot.workspace_id || 'Not set') + '</span>';
  html += '<strong>OpenCode Go</strong>';
  html += '</div>';

  if (snapshot.error && !snapshot.error.includes('Not yet loaded')) {
    html += '<div class="error-banner">' + escapeHtml(snapshot.error);
    if (snapshot.error.includes('expired') || snapshot.error.includes('Not logged')) {
      html += '<div class="error-cta"><a id="btn-login">Log in</a></div>';
    }
    html += '</div>';
  }

  html += '' +
    '<section class="usage-hero">' +
      '<div class="usage-hero-top">' +
        '<span>Rolling</span>' +
        '<strong>' + formatCountdown(rolling.reset_in_sec || 0) + '</strong>' +
      '</div>' +
      '<div class="usage-hero-value">' + (rolling.usage_percent || 0) + '<span>%</span></div>' +
      '<div class="bar usage-main-bar">' +
        '<div class="bar-fill bar-rolling" style="width:' + Math.min(rolling.usage_percent || 0, 100) + '%"></div>' +
      '</div>' +
    '</section>';

  html += '<div class="quota-strip">';
  html += buildQuotaPill('Weekly', weekly, 'bar-weekly');
  html += buildQuotaPill('Monthly', monthly, 'bar-monthly');
  html += '</div>';

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

function buildQuotaPill(label, period, barClass) {
  if (!period) return '';

  const pct = period.usage_percent || 0;
  const resetIn = formatCountdown(period.reset_in_sec || 0);

  return '' +
    '<div class="quota-pill">' +
      '<div class="quota-pill-top"><span>' + label + '</span><strong>' + pct + '%</strong></div>' +
      '<div class="quota-reset">' + resetIn + '</div>' +
      '<div class="bar"><div class="bar-fill ' + barClass + '" style="width:' + Math.min(pct, 100) + '%"></div></div>' +
    '</div>';
}
