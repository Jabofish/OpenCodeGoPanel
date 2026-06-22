import { formatPct, escapeHtml, formatTimeAgo } from './format.js';

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

function filterRecordsByRange(records, range, now = new Date()) {
  if (!records || range === 'all') return records || [];
  const hours = range === '24h' ? 24 : 24 * 7;
  const cutoff = now.getTime() - hours * 3600 * 1000;
  return records.filter(r => {
    const t = Date.parse(r.timeCreated || r.time_created);
    return Number.isFinite(t) && t >= cutoff;
  });
}

/**
 * Aggregate model rows from both model_calls stats and usage_records.
 * Returns sorted array of { name, calls, percentage, input, output, cacheRead, cacheWrite5m, cacheWrite1h, cost, totalTokens, cacheRate, topProvider }
 */
export function aggregateModelRows(snapshot, range = 'all') {
  snapshot = snapshot || {};
  const records = filterRecordsByRange(snapshot.usage_records, range);
  const callStats = new Map();

  // For 'all' range, seed with model_calls data (which has call counts)
  // For 24h/7d, we'll count calls from filtered records only
  if (range === 'all' && snapshot.model_calls?.models) {
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

  // Aggregate from filtered records
  for (const r of records) {
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

    // For 24h/7d, count calls from records (not pre-seeded)
    if (range !== 'all') {
      row.calls += 1;
    }

    row.input += r.inputTokens || 0;
    row.output += r.outputTokens || 0;
    row.cacheRead += r.cacheReadTokens || 0;
    row.cacheWrite5m += r.cacheWrite5mTokens || 0;
    row.cacheWrite1h += r.cacheWrite1hTokens || 0;
    row.cost += r.cost || 0;
    const provider = r.provider || 'unknown';
    row.providers.set(provider, (row.providers.get(provider) || 0) + 1);
  }

  // Recalculate percentage for non-all ranges
  if (range !== 'all') {
    const totalCalls = [...callStats.values()].reduce((sum, r) => sum + r.calls, 0);
    for (const row of callStats.values()) {
      row.percentage = totalCalls > 0 ? (row.calls / totalCalls * 100) : 0;
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

export function renderModelsTab(snapshot, view, actions) {
  const container = document.getElementById('tab-models');
  if (!container) return;

  snapshot = snapshot || {};
  const v = view || { query: '', sortBy: 'calls', showAll: false, range: 'all', requestsOpen: false, requestsModelFilter: '' };
  const a = actions || {};
  const mc = snapshot.model_calls;
  if (!mc || !mc.models || mc.models.length === 0) {
    container.innerHTML = '<div class="loading">No model call data yet</div>';
    return;
  }

  const rows = aggregateModelRows(snapshot, v.range);

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

  // Controls — two rows to avoid cramping
  html += '<div class="model-controls">';

  // Row 1: filter, sort, range
  html += '<div class="model-controls-row">';
  html += '<input id="model-filter" class="model-filter-input" type="text" placeholder="Filter models" value="' + escapeHtml(v.query || '') + '">';
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
  html += '<div class="trend-range" style="margin-bottom:0">';
  ['24h','7d','all'].forEach(r => {
    html += '<button class="trend-range-btn' + (v.range === r ? ' active' : '') +
      '" data-model-range="' + r + '">' + (r === 'all' ? 'All' : r) + '</button>';
  });
  html += '</div>';
  html += '</div>';

  // Row 2: recent requests toggle + show all
  html += '<div class="model-controls-row">';
  html += '<button id="model-requests-toggle" class="model-requests-btn' + (v.requestsOpen ? ' active' : '') + '">' +
    (v.requestsOpen ? '&#x25BC; Recent requests' : '&#x25B6; Recent requests') +
  '</button>';
  html += '<button id="model-show-all" class="model-show-all-btn">' + (v.showAll ? 'Show Less' : 'Show All') + '</button>';
  html += '</div>';

  html += '</div>';

  // Active model filter chip (when a model row is selected for request filtering)
  if (v.requestsModelFilter) {
    html += '<div class="request-filter-chip">' +
      '<span>Filtering: ' + escapeHtml(v.requestsModelFilter) + '</span>' +
      '<button id="request-filter-clear" class="request-filter-clear" title="Clear filter">&times;</button>' +
    '</div>';
  }

  // Model list
  const totalCalls = filtered.reduce((sum, m) => sum + (m.calls || 0), 0);
  html += '<div class="model-panel">';
  html += '<div class="model-panel-head"><span>Total calls</span><strong>' + totalCalls.toLocaleString() + '</strong></div>';

  visible.forEach((m, i) => {
    const color = MODEL_COLORS[i % MODEL_COLORS.length];
    const hasTokens = m.input > 0 || m.output > 0 || m.cacheRead > 0;
    const isFilterActive = v.requestsModelFilter === m.name;

    html += '' +
      '<div class="model-item' + (isFilterActive ? ' model-item-active-filter' : '') + '" data-request-model="' + escapeHtml(m.name) + '">' +
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

  // Token summary (filtered by range like visible rows)
  let totalInput = 0, totalOutput = 0, totalCacheRead = 0;
  for (const r of filtered) {
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

  // Recent requests list (shown when requestsOpen is true)
  if (v.requestsOpen) {
    html += renderRequestList(snapshot, v);
  }

  // Preserve input state before replacing innerHTML
  const prevFilterEl = document.getElementById('model-filter');
  const savedFilterValue = prevFilterEl?.value || '';
  const savedFilterFocused = prevFilterEl === document.activeElement;
  const savedSelectionStart = savedFilterFocused ? prevFilterEl.selectionStart : null;
  const savedSelectionEnd = savedFilterFocused ? prevFilterEl.selectionEnd : null;

  // Preserve request list scroll position
  const prevRequestList = container.querySelector('.request-list');
  const savedRequestScrollTop = prevRequestList ? prevRequestList.scrollTop : 0;

  html += '</div>';
  container.innerHTML = html;

  // Bind events
  setTimeout(() => {
    bindModelControls(container, a);
    // Restore input focus and cursor position
    if (savedFilterFocused && savedFilterValue === v.query) {
      const newFilterEl = document.getElementById('model-filter');
      if (newFilterEl) {
        newFilterEl.focus();
        if (savedSelectionStart !== null && savedSelectionEnd !== null) {
          newFilterEl.setSelectionRange(savedSelectionStart, savedSelectionEnd);
        }
      }
    }
    // Restore request list scroll position
    if (savedRequestScrollTop > 0) {
      const newRequestList = container.querySelector('.request-list');
      if (newRequestList) newRequestList.scrollTop = savedRequestScrollTop;
    }
  }, 0);
}

/**
 * Render the scrollable list of individual API request records.
 * Records are filtered by the active range (24h/7d/all) and optionally
 * by the requestsModelFilter (set when a model row is clicked).
 */
function renderRequestList(snapshot, v) {
  const records = filterRecordsByRange(snapshot.usage_records, v.range);

  // Apply model filter if active
  let filtered = records;
  if (v.requestsModelFilter) {
    const mf = v.requestsModelFilter.toLowerCase();
    filtered = records.filter(r => (r.model || '').toLowerCase() === mf);
  }

  // Sort newest-first
  filtered.sort((a, b) => {
    const ta = Date.parse(a.timeCreated || a.time_created) || 0;
    const tb = Date.parse(b.timeCreated || b.time_created) || 0;
    return tb - ta;
  });

  // Cap display at 50 entries (the backend already limits usage_records)
  const display = filtered.slice(0, 50);

  let html = '<div class="request-list">';
  html += '<div class="request-list-header">' +
    '<span>' + filtered.length + ' request' + (filtered.length !== 1 ? 's' : '') + '</span>' +
    (v.requestsModelFilter ? ' <span class="request-list-filtered">in ' + escapeHtml(v.requestsModelFilter) + '</span>' : '') +
  '</div>';

  if (display.length === 0) {
    html += '<div class="request-list-empty">No requests found for this range</div>';
  } else {
    display.forEach(r => {
      const time = formatTimeAgo(r.timeCreated || r.time_created);
      const model = r.model || 'unknown';
      const provider = r.provider || '';
      const inTok = r.inputTokens || 0;
      const outTok = r.outputTokens || 0;
      const cacheTok = r.cacheReadTokens || 0;
      const cost = r.cost || 0;
      const plan = r.enrichment?.plan || '';

      html += '<div class="request-item">';
      html += '<div class="request-item-top">';
      html += '<span class="request-time">' + escapeHtml(time) + '</span>';
      html += '<span class="request-model" title="' + escapeHtml(model) + '">' + escapeHtml(model) + '</span>';
      if (plan) {
        html += '<span class="request-plan-badge">' + escapeHtml(plan) + '</span>';
      }
      html += '</div>';
      html += '<div class="request-item-meta">';
      if (provider) html += '<span class="request-provider">' + escapeHtml(provider) + '</span>';
      html += '<span class="request-tokens">IN ' + formatTokens(inTok) + ' / OUT ' + formatTokens(outTok);
      if (cacheTok > 0) html += ' / CACHE ' + formatTokens(cacheTok);
      html += '</span>';
      html += '<span class="request-cost">' + formatCost(cost) + '</span>';
      html += '</div>';
      html += '</div>';
    });
  }

  html += '</div>';
  return html;
}

function bindModelControls(container, a) {
  const filterEl = container.querySelector('#model-filter');
  const sortEl = container.querySelector('#model-sort');
  const showAllBtn = container.querySelector('#model-show-all');
  const requestsToggleBtn = container.querySelector('#model-requests-toggle');
  const filterClearBtn = container.querySelector('#request-filter-clear');

  if (filterEl) {
    filterEl.addEventListener('input', debounce(() => {
      if (a.setQuery) a.setQuery(filterEl.value);
    }, 120));
  }
  if (sortEl) sortEl.addEventListener('change', () => { if (a.setSortBy) a.setSortBy(sortEl.value); });
  if (showAllBtn) showAllBtn.addEventListener('click', () => { if (a.toggleShowAll) a.toggleShowAll(); });
  if (requestsToggleBtn) requestsToggleBtn.addEventListener('click', () => { if (a.toggleRequests) a.toggleRequests(); });
  if (filterClearBtn) filterClearBtn.addEventListener('click', () => { if (a.setRequestsModelFilter) a.setRequestsModelFilter(''); });

  container.querySelectorAll('[data-model-range]').forEach(btn => {
    btn.addEventListener('click', () => {
      if (a.setRange) a.setRange(btn.dataset.modelRange);
    });
  });

  // Model row click → toggle request filter by that model
  container.querySelectorAll('[data-request-model]').forEach(el => {
    el.addEventListener('click', () => {
      if (a.setRequestsModelFilter) a.setRequestsModelFilter(el.dataset.requestModel);
    });
  });
}

function debounce(fn, delay) {
  let timer = null;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), delay);
  };
}
