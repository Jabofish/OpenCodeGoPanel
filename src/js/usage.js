import { formatCountdown, escapeHtml } from './format.js';

const OPENCODE_COST_UNITS_PER_USD = 100000000;

export function renderUsageTab(snapshot, settings) {
  const container = document.getElementById('tab-usage');
  if (!container) return;

  const u = snapshot.usage;
  const rolling = u.rolling || {};
  const weekly = u.weekly || {};
  const monthly = u.monthly || {};

  const ws = (snapshot.workspaces || []).find(w => w.id === snapshot.workspace_id);
  const wsName = ws?.name || snapshot.workspace_id || 'Not set';

  let html = '';

  html += '<div class="mini-header">';
  html += '<span>' + escapeHtml(wsName) + '</span>';
  html += '<strong>OpenCode Go</strong>';
  html += '</div>';

  if (snapshot.error && !snapshot.error.includes('Not yet loaded')) {
    const err = snapshot.error;
    const isNoGoPlan = err.includes('Failed to match') || err.includes('HTTP error: 4')
        || err.includes('No Go');
    const isAuth = err.includes('expired') || err.includes('Not logged');

    if (isNoGoPlan && rolling.status === 'unknown') {
      // Workspace has no Go plan — show friendly message instead of error
      html += '<div class="info-banner">This workspace has no active Go plan.</div>';
    } else {
      html += '<div class="error-banner">' + escapeHtml(err);
      if (isAuth) {
        html += '<div class="error-cta"><a id="btn-login">Log in</a></div>';
      }
      html += '</div>';
    }
  }

  const threshold = settings?.usageThreshold || 0;
  const rollingPct = rolling.usage_percent || 0;
  const overThreshold = threshold >= 50 && rollingPct >= threshold;
  const barClass = overThreshold ? 'bar-rolling bar-over-threshold' : 'bar-rolling';

  html += '' +
    '<section class="usage-hero">' +
      '<div class="usage-hero-top">' +
        '<span>Rolling' + (overThreshold ? ' ⚠' : '') + '</span>' +
        '<strong>' + formatCountdown(rolling.reset_in_sec || 0) + '</strong>' +
      '</div>' +
      '<div class="usage-hero-value">' + rollingPct + '<span>%</span></div>' +
      '<div class="bar usage-main-bar">' +
        '<div class="bar-fill ' + barClass + '" style="width:' + Math.min(rollingPct, 100) + '%"></div>' +
      '</div>' +
    '</section>';

  html += '<div class="quota-strip">';
  html += buildQuotaPill('Weekly', weekly, 'bar-weekly');
  html += buildQuotaPill('Monthly', monthly, 'bar-monthly');
  html += '</div>';

  // Budget section
  const budget = settings?.monthlyBudget || 0;
  if (budget > 0) {
    const totalSpentUnits = (snapshot.daily_costs || []).reduce((sum, c) => sum + (c.totalCost || 0), 0);
    const totalSpentDollars = totalSpentUnits / OPENCODE_COST_UNITS_PER_USD;
    const budgetDollarsValue = budget / 100;
    const pct = Math.round((totalSpentDollars / budgetDollarsValue) * 100);
    const spentDollars = totalSpentDollars.toFixed(4);
    const budgetDollars = (budget / 100).toFixed(2);
    let barColor = 'var(--color-weekly)';
    if (pct > 100) barColor = '#e06170';
    else if (pct > 80) barColor = '#e9ae55';

    html += '' +
      '<div class="budget-section">' +
        '<div class="budget-header">' +
          '<span>Monthly Budget</span>' +
          '<strong>$' + spentDollars + ' / $' + budgetDollars + '</strong>' +
        '</div>' +
        '<div class="budget-bar">' +
          '<div class="budget-fill" style="width:' + Math.min(pct, 100) + '%;background:' + barColor + '"></div>' +
        '</div>' +
        '<div class="budget-pct" style="color:' + barColor + '">' + pct + '% used</div>' +
      '</div>';
  }

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
