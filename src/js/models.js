import { formatPct, escapeHtml } from './format.js';

const MODEL_COLORS = ['#8a9eff', '#5cc08a', '#e0a050', '#7b7bbb', '#5aaac0', '#c080d0'];
const OPENCODE_COST_UNITS_PER_USD = 100000000;

function formatCost(units) {
  return '$' + ((units || 0) / OPENCODE_COST_UNITS_PER_USD).toFixed(4);
}

function formatTokens(n) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toLocaleString();
}

/**
 * Aggregate model rows from both model_calls stats and usage_records.
 * Returns sorted array of { name, calls, percentage, input, output, cacheRead, cacheWrite5m, cacheWrite1h, cost, totalTokens, cacheRate, topProvider }
 */
export function aggregateModelRows(snapshot) {
  const callStats = new Map();
  if (snapshot.model_calls?.models) {
    for (const m of snapshot.model_calls.models) {
      callStats.set(m.name, {
        name: m.name,
        calls: m.calls || 0,
        percentage: m.percentage || 0,
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite5m: 0,
        cacheWrite1h: 0,
        cost: 0,
        providers: new Map(),
      });
    }
  }

  if (snapshot.usage_records) {
    for (const r of snapshot.usage_records) {
      const name = r.model || 'unknown';
      if (!callStats.has(name)) {
        callStats.set(name, {
          name,
          calls: 0,
          percentage: 0,
          input: 0,
          output: 0,
          cacheRead: 0,
          cacheWrite5m: 0,
          cacheWrite1h: 0,
          cost: 0,
          providers: new Map(),
        });
      }
      const row = callStats.get(name);
      row.input += r.inputTokens || 0;
      row.output += r.outputTokens || 0;
      row.cacheRead += r.cacheReadTokens || 0;
      row.cacheWrite5m += r.cacheWrite5mTokens || 0;
      row.cacheWrite1h += r.cacheWrite1hTokens || 0;
      row.cost += r.cost || 0;
      const provider = r.provider || 'unknown';
      row.providers.set(provider, (row.providers.get(provider) || 0) + 1);
    }
  }

  return [...callStats.values()].map(row => {
    const totalTokens = row.input + row.output + row.cacheRead;
    return {
      ...row,
      totalTokens,
      cacheRate: totalTokens > 0 ? row.cacheRead / totalTokens * 100 : 0,
      topProvider: [...row.providers.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] || '',
    };
  });
}

const SORT_FNS = {
  calls: (a, b) => b.calls - a.calls || a.name.localeCompare(b.name),
  cost: (a, b) => b.cost - a.cost || a.name.localeCompare(b.name),
  input: (a, b) => b.input - a.input || a.name.localeCompare(b.name),
  output: (a, b) => b.output - a.output || a.name.localeCompare(b.name),
  cache: (a, b) => b.cacheRead - a.cacheRead || a.name.localeCompare(b.name),
  cacheRate: (a, b) => b.cacheRate - a.cacheRate || a.name.localeCompare(b.name),
};

export function renderModelsTab(snapshot, view) {
  const container = document.getElementById('tab-models');
  if (!container) return;

  const v = view || { query: '', sortBy: 'calls', showAll: false };
  const mc = snapshot.model_calls;
  if (!mc || !mc.models || mc.models.length === 0) {
    container.innerHTML = '<div class="loading">No model call data yet</div>';
    return;
  }

  const rows = aggregateModelRows(snapshot);

  // Filter
  let filtered = rows;
  if (v.query) {
    const q = v.query.toLowerCase();
    filtered = rows.filter(r => r.name.toLowerCase().includes(q));
  }

  // Sort
  const sortFn = SORT_FNS[v.sortBy] || SORT_FNS.calls;
  filtered.sort(sortFn);

  // Limit
  const visible = v.showAll ? filtered : filtered.slice(0, 6);

  let html = '';

  // Controls
  html += '<div class="model-controls">';
  html += '<input id="model-filter" class="model-filter-input" type="text" placeholder="Filter models" value="' + escapeHtml(v.query) + '">';
  html += '<select id="model-sort" class="model-sort-select">';
  [
    { value: 'calls', label: 'Calls' },
    { value: 'cost', label: 'Cost' },
    { value: 'input', label: 'Input' },
    { value: 'output', label: 'Output' },
    { value: 'cache', label: 'Cache' },
    { value: 'cacheRate', label: 'Cache rate' },
  ].forEach(opt => {
    html += '<option value="' + opt.value + '"' + (v.sortBy === opt.value ? ' selected' : '') + '>' + opt.label + '</option>';
  });
  html += '</select>';
  html += '<button id="model-show-all" class="model-show-all-btn">' + (v.showAll ? 'Top 6' : 'All') + '</button>';
  html += '</div>';

  // Model list
  html += '<div class="model-panel">';
  html += '<div class="model-panel-head"><span>Total calls</span><strong>' + mc.total_calls.toLocaleString() + '</strong></div>';

  visible.forEach((m, i) => {
    const color = MODEL_COLORS[i % MODEL_COLORS.length];
    const hasTokens = m.input > 0 || m.output > 0 || m.cacheRead > 0;

    html += '' +
      '<div class="model-item">' +
        '<div class="model-top">' +
          '<span class="model-name" title="' + escapeHtml(m.name) + '">' + escapeHtml(m.name) + '</span>' +
          '<span class="model-count" style="color:' + color + '">' + (m.calls || 0).toLocaleString() + '</span>' +
        '</div>' +
        '<div class="model-meta">' +
          '<span>' + formatCost(m.cost) + '</span>' +
          '<span>' + (m.percentage > 0 ? formatPct(m.percentage) + ' of total' : '') + '</span>' +
        '</div>' +
        '<div class="model-bar">' +
          '<div style="width:' + Math.min(m.percentage || 0, 100) + '%;height:100%;background:' + color + ';border-radius:2px;"></div>' +
        '</div>';

    if (hasTokens) {
      const cacheRate = m.cacheRate || 0;
      html += '' +
        '<div class="model-tokens">' +
          '<span class="tok-in" title="Input tokens">IN ' + formatTokens(m.input) + '</span>' +
          '<span class="tok-out" title="Output tokens">OUT ' + formatTokens(m.output) + '</span>' +
          '<span class="tok-cache" title="Cache read · ' + cacheRate.toFixed(1) + '% hit rate">CACHE ' + formatTokens(m.cacheRead) + ' · ' + cacheRate.toFixed(1) + '%</span>' +
        '</div>';
    }

    html += '</div>';
  });

  // Token summary
  let totalInput = 0, totalOutput = 0, totalCacheRead = 0;
  for (const r of rows) {
    totalInput += r.input;
    totalOutput += r.output;
    totalCacheRead += r.cacheRead;
  }
  const totalTokens = totalInput + totalOutput + totalCacheRead;
  const cacheRate = totalTokens > 0 ? (totalCacheRead / totalTokens * 100) : 0;

  if (totalTokens > 0) {
    html += '' +
      '<div class="token-summary">' +
        '<div class="token-summary-title">Token Summary</div>' +
        '<div class="token-row"><span>Input</span><strong>' + formatTokens(totalInput) + '</strong></div>' +
        '<div class="token-row"><span>Output</span><strong>' + formatTokens(totalOutput) + '</strong></div>' +
        '<div class="token-row"><span>Cache Read</span><strong>' + formatTokens(totalCacheRead) + '</strong></div>' +
        '<div class="token-row token-rate-row"><span>Cache Hit Rate</span><strong class="cache-rate">' + cacheRate.toFixed(1) + '%</strong></div>' +
      '</div>';
  }

  html += '<div class="model-summary">' +
    rows.length + ' models tracked' +
    (v.showAll ? '' : ' · showing ' + Math.min(6, rows.length)) +
  '</div>';

  html += '</div>';
  container.innerHTML = html;

  // Bind events
  setTimeout(() => bindModelControls(), 0);
}

function bindModelControls() {
  // We use a small module-scoped dispatch helper; the modelActions object
  // on window is set by app.js before the first render.
  const filterEl = document.getElementById('model-filter');
  const sortEl = document.getElementById('model-sort');
  const showAllBtn = document.getElementById('model-show-all');

  if (filterEl) {
    filterEl.addEventListener('input', () => {
      if (window._modelActions) {
        window._modelActions.setQuery(filterEl.value);
      }
    });
  }
  if (sortEl) {
    sortEl.addEventListener('change', () => {
      if (window._modelActions) {
        window._modelActions.setSortBy(sortEl.value);
      }
    });
  }
  if (showAllBtn) {
    showAllBtn.addEventListener('click', () => {
      if (window._modelActions) {
        window._modelActions.toggleShowAll();
      }
    });
  }
}
