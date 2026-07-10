import { formatCountdown, escapeHtml } from './format.js';

const OPENCODE_COST_UNITS_PER_USD = 100000000;

let _usageCostChart = null;

export function renderUsageTab(snapshot, settings, insights) {
  const container = document.getElementById('tab-usage');
  if (!container) return;

  snapshot = snapshot || {};
  const u = snapshot.usage || {};
  const rolling = u.rolling || {};
  const weekly = u.weekly || {};
  const monthly = u.monthly || {};
  const deltas = insights?.deltas || {};

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
      '<div class="usage-hero-value">' + rollingPct + '<span>%</span>' + deltaBadge(deltas.rolling, 'vs yesterday') + '</div>' +
      '<div class="bar usage-main-bar">' +
        '<div class="bar-fill ' + barClass + '" style="width:' + Math.min(rollingPct, 100) + '%"></div>' +
      '</div>' +
    '</section>';

  // Insight strip (max 1 primary insight)
  if (insights && insights.messages && insights.messages.length > 0) {
    const primary = insights.messages.reduce((best, m) => {
      const prio = { danger: 3, warning: 2, info: 1 };
      return (prio[m.severity] || 0) > (prio[best.severity] || 0) ? m : best;
    }, insights.messages[0]);
    html += '<div class="insight-strip insight-' + primary.severity + '">' +
      '<span>' + escapeHtml(primary.title) + '</span>' +
      (primary.metric ? '<strong>' + escapeHtml(primary.metric) + '</strong>' : '') +
    '</div>';
  }

  html += '<div class="quota-strip">';
  html += buildQuotaPill('Weekly', weekly, 'bar-weekly', deltas.weekly, 'vs last week');
  html += buildQuotaPill('Monthly', monthly, 'bar-monthly', deltas.monthly, 'vs last month');
  html += '</div>';

  const budget = settings?.monthlyBudget || 0;
  if (budget > 0) {
    // Only sum costs for current month
    const now = new Date();
    const nowY = now.getFullYear();
    const nowM = String(now.getMonth() + 1).padStart(2, '0');
    const monthPrefix = `${nowY}-${nowM}`;
    const dailyCosts = snapshot.daily_costs || [];
    const monthCosts = dailyCosts.filter(c => (c.date || '').startsWith(monthPrefix));
    const monthCostUnits = monthCosts
      .reduce((sum, c) => sum + (c.totalCost || 0), 0);
    const totalSpentDollars = monthCostUnits / OPENCODE_COST_UNITS_PER_USD;

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

  html += '' +
    '<div class="cost-chart-section">' +
      '<div class="cost-chart-header">' +
        '<span>Daily Cost (this month)</span>' +
        '<strong id="cost-chart-total"></strong>' +
      '</div>' +
      '<div class="cost-chart-canvas-wrap">' +
        '<canvas id="usage-cost-chart" class="cost-chart-canvas"></canvas>' +
      '</div>' +
    '</div>';

  container.innerHTML = html;

  renderCostChart(snapshot);

  const loginBtn = document.getElementById('btn-login');
  if (loginBtn) {
    loginBtn.addEventListener('click', async () => {
      const { invoke } = window.__TAURI__.core;
      await invoke('open_login_window');
    });
  }
}

function deltaBadge(delta, label) {
  if (!delta) return '';
  const arrow = delta.direction === 'up' ? '↗' : delta.direction === 'down' ? '↘' : '→';
  const val = Math.round(delta.value);
  const text = (val > 0 ? '+' : '') + val;
  return '<span class="usage-delta ' + delta.direction + '" title="' + escapeHtml(label) + '">' + arrow + ' ' + text + '%</span>';
}

function buildQuotaPill(label, period, barClass, delta, deltaLabel) {
  if (!period) return '';

  const pct = period.usage_percent || 0;
  const resetIn = formatCountdown(period.reset_in_sec || 0);

  return '' +
    '<div class="quota-pill">' +
      '<div class="quota-pill-top"><span>' + label + '</span><strong>' + pct + '%</strong>' + deltaBadge(delta, deltaLabel) + '</div>' +
      '<div class="quota-reset">' + resetIn + '</div>' +
      '<div class="bar"><div class="bar-fill ' + barClass + '" style="width:' + Math.min(pct, 100) + '%"></div></div>' +
    '</div>';
}

function renderCostChart(snapshot) {
  if (_usageCostChart) {
    _usageCostChart.destroy();
    _usageCostChart = null;
  }

  const costs = snapshot?.daily_costs || [];
  const totalEl = document.getElementById('cost-chart-total');
  const canvas = document.getElementById('usage-cost-chart');

  if (costs.length === 0) {
    if (totalEl) totalEl.textContent = '';
    if (canvas) canvas.style.display = 'none';
    const section = canvas && canvas.parentElement;
    if (section && !section.querySelector('.cost-chart-empty')) {
      const empty = document.createElement('div');
      empty.className = 'cost-chart-empty';
      empty.textContent = 'No data available.';
      section.appendChild(empty);
    }
    return;
  }

  if (typeof Chart === 'undefined') {
    console.warn('[Usage] Chart.js not available');
    if (totalEl) totalEl.textContent = '';
    if (canvas) canvas.style.display = 'none';
    return;
  }

  if (!canvas) return;
  if (canvas) canvas.style.display = 'block';
  const leftover = canvas.parentElement?.querySelector('.cost-chart-empty');
  if (leftover) leftover.remove();

  const byDate = new Map();
  for (const entry of costs) {
    const d = entry.date;
    const v = entry.totalCost || 0;
    byDate.set(d, (byDate.get(d) || 0) + v);
  }
  const labels = [...byDate.keys()].sort();
  const data = labels.map(d => byDate.get(d) / OPENCODE_COST_UNITS_PER_USD);

  let runningTotal = 0;
  const cumulative = data.map(v => { runningTotal += v; return runningTotal; });

  const total = data.reduce((s, v) => s + v, 0);
  if (totalEl) totalEl.textContent = '$' + total.toFixed(2);

  _usageCostChart = new Chart(canvas, {
    type: 'bar',
    data: {
      labels: labels,
      datasets: [
        {
          type: 'bar',
          label: 'Daily',
          data: data,
          yAxisID: 'yDaily',
          backgroundColor: 'rgba(130, 162, 255, 0.55)',
          borderColor: 'rgba(130, 162, 255, 0.55)',
          borderWidth: 0,
          borderRadius: 2,
          barPercentage: 0.7,
          categoryPercentage: 0.85,
          order: 1
        },
        {
          type: 'line',
          label: 'Cumulative',
          data: cumulative,
          yAxisID: 'yCumulative',
          borderColor: '#e9ae55',
          backgroundColor: 'transparent',
          borderWidth: 1.5,
          pointRadius: 0,
          pointHoverRadius: 3,
          tension: 0,
          fill: false,
          order: 2
        }
      ]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: false,
      plugins: {
        legend: { display: false },
        tooltip: {
          callbacks: {
            title: (items) => items[0].label,
            label: (item) => {
              if (item.dataset.label === 'Cumulative') {
                return 'Total: $' + item.parsed.y.toFixed(2);
              }
              return 'Day: $' + item.parsed.y.toFixed(4);
            }
          }
        }
      },
      scales: {
        x: {
          type: 'category',
          grid: { color: 'rgba(255,255,255,0.04)' },
          ticks: { color: '#6e6e8a', font: { size: 9 }, maxRotation: 0, autoSkip: true, maxTicksLimit: 8 }
        },
        yDaily: {
          type: 'linear',
          position: 'left',
          beginAtZero: true,
          grid: { color: 'rgba(255,255,255,0.04)' },
          ticks: { color: '#6e6e8a', font: { size: 9 }, maxTicksLimit: 4, callback: (v) => '$' + v.toFixed(2) }
        },
        yCumulative: {
          type: 'linear',
          position: 'right',
          beginAtZero: true,
          grid: { drawOnChartArea: false },
          ticks: { color: '#e9ae55', font: { size: 9 }, maxTicksLimit: 4, callback: (v) => '$' + v.toFixed(0) }
        }
      }
    }
  });
}
