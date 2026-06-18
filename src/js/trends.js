import { escapeHtml } from './format.js';

const OPENCODE_COST_UNITS_PER_USD = 100000000;
let _trendChart = null;
let _trendChartKey = '';
let _trendDomKey = '';

export function renderTrendsTab(history, snapshot, settings, actions, days) {
  const container = document.getElementById('tab-trends');
  if (!container) return;

  const entries = Array.isArray(history) ? history : [];
  const domKey = buildTrendKey(entries, days);
  if (_trendDomKey !== domKey || (entries.length > 0 && !container.querySelector('#trend-chart-canvas'))) {
    container.innerHTML = buildTrendHtml(entries, settings, days);
    _trendDomKey = domKey;
    bindTrendControls(container, actions, days);
  }
  renderTrendChart(entries);
}

function buildTrendHtml(entries, settings, days) {
  const daysLabel = days || 30;
  const latest = entries.length > 0 ? entries[entries.length - 1] : null;

  let html = '';

  // Period selector
  html += '<div class="trend-range">';
  [7, 30, 90].forEach(d => {
    html += '<button class="trend-range-btn' + (daysLabel === d ? ' active' : '') +
      '" data-days="' + d + '">' + d + 'd</button>';
  });
  html += '</div>';

  // Summary
  if (latest) {
    html += '<div class="trend-summary">';
    html += '<div class="trend-summary-row"><span>Latest rolling</span><strong>' +
      safePct(latest.rolling_pct) + '%</strong></div>';
    html += '<div class="trend-summary-row"><span>Latest weekly</span><strong>' +
      safePct(latest.weekly_pct) + '%</strong></div>';
    html += '<div class="trend-summary-row"><span>Latest monthly</span><strong>' +
      safePct(latest.monthly_pct) + '%</strong></div>';
    const totalCost = entries.reduce((sum, e) => sum + (e.total_cost || 0), 0);
    html += '<div class="trend-summary-row"><span>Cost in period</span><strong>$' +
      (totalCost / OPENCODE_COST_UNITS_PER_USD).toFixed(4) + '</strong></div>';
    html += '</div>';
  }

  // Chart area
  html += '<div class="trend-panel">';
  if (entries.length === 0) {
    html += '<div class="loading">No history yet. Keep the panel running to collect trend data.</div>';
  } else {
    html += '<div class="trend-chart-wrap"><canvas id="trend-chart-canvas" class="trend-chart-canvas"></canvas></div>';
  }
  html += '</div>';

  return html;
}

function bindTrendControls(container, actions, days) {
  container.querySelectorAll('.trend-range-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const d = parseInt(btn.dataset.days, 10);
      if (d !== days && actions?.setHistoryDays) {
        actions.setHistoryDays(d);
      }
    });
  });
}

function renderTrendChart(entries) {
  const canvas = document.getElementById('trend-chart-canvas');
  if (!canvas || entries.length === 0) {
    destroyTrendChart();
    return;
  }

  const chartKey = buildTrendKey(entries, null);

  if (_trendChart && _trendChart.canvas === canvas && _trendChartKey === chartKey) return;

  // Destroy previous chart instance
  destroyTrendChart();

  const labels = entries.map(e => e.date);
  const rolling = entries.map(e => safePct(e.rolling_pct));
  const weekly = entries.map(e => safePct(e.weekly_pct));
  const monthly = entries.map(e => safePct(e.monthly_pct));

  // Check if Chart.js is available (loaded as vendor script)
  if (typeof Chart === 'undefined') {
    console.warn('[Trends] Chart.js not available');
    return;
  }

  _trendChart = new Chart(canvas, {
    type: 'line',
    data: {
      labels,
      datasets: [
        {
          label: 'Rolling',
          data: rolling,
          borderColor: '#82a2ff',
          backgroundColor: 'rgba(130, 162, 255, 0.1)',
          fill: false,
          tension: 0.3,
          pointRadius: 0,
          borderWidth: 1.5,
        },
        {
          label: 'Weekly',
          data: weekly,
          borderColor: '#5fcf97',
          backgroundColor: 'rgba(95, 207, 151, 0.1)',
          fill: false,
          tension: 0.3,
          pointRadius: 0,
          borderWidth: 1.5,
        },
        {
          label: 'Monthly',
          data: monthly,
          borderColor: '#e9ae55',
          backgroundColor: 'rgba(233, 174, 85, 0.1)',
          fill: false,
          tension: 0.3,
          pointRadius: 0,
          borderWidth: 1.5,
        },
      ],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: {
          display: true,
          position: 'bottom',
          labels: {
            color: '#858b99',
            font: { size: 9 },
            boxWidth: 10,
            padding: 8,
          },
        },
      },
      scales: {
        x: {
          ticks: { color: '#626875', font: { size: 8 }, maxTicksLimit: 8 },
          grid: { color: 'rgba(255,255,255,0.04)' },
        },
        y: {
          min: 0,
          max: 100,
          ticks: { color: '#626875', font: { size: 8 }, stepSize: 25 },
          grid: { color: 'rgba(255,255,255,0.04)' },
        },
      },
    },
  });
  _trendChartKey = chartKey;
}

function destroyTrendChart() {
  if (!_trendChart) return;
  _trendChart.destroy();
  _trendChart = null;
  _trendChartKey = '';
}

function buildTrendKey(entries, days) {
  return (days ?? '') + '::' + entries.map(e => [
    e.date,
    safePct(e.rolling_pct),
    safePct(e.weekly_pct),
    safePct(e.monthly_pct),
    e.total_cost || 0,
  ].join(':')).join('|');
}

function safePct(value) {
  const num = Number(value);
  return Number.isFinite(num) ? num : 0;
}
