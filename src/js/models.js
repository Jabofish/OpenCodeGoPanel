import { formatPct, escapeHtml } from './format.js';

const MODEL_COLORS = ['#8a9eff', '#5cc08a', '#e0a050', '#7b7bbb', '#5aaac0', '#c080d0'];

/**
 * Aggregate token usage by model from usage records.
 * Returns a map: modelName -> { input, output, cacheRead, cacheWrite5m, cacheWrite1h }
 */
function aggregateTokensByModel(records) {
  const map = {};
  if (!records) return map;
  for (const r of records) {
    const key = r.model || 'unknown';
    if (!map[key]) {
      map[key] = { input: 0, output: 0, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 };
    }
    map[key].input += r.inputTokens || 0;
    map[key].output += r.outputTokens || 0;
    map[key].cacheRead += r.cacheReadTokens || 0;
    map[key].cacheWrite5m += r.cacheWrite5mTokens || 0;
    map[key].cacheWrite1h += r.cacheWrite1hTokens || 0;
  }
  return map;
}

function formatTokens(n) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toLocaleString();
}

export function renderModelsTab(snapshot) {
  const container = document.getElementById('tab-models');
  if (!container) return;

  const mc = snapshot.model_calls;
  if (!mc || !mc.models || mc.models.length === 0) {
    container.innerHTML = '<div class="loading">No model call data yet</div>';
    return;
  }

  const tokenMap = aggregateTokensByModel(snapshot.usage_records);
  const visibleModels = mc.models.slice(0, 6);

  let html = '<div class="model-panel">';
  html += '<div class="model-panel-head"><span>Total calls</span><strong>' + mc.total_calls.toLocaleString() + '</strong></div>';

  visibleModels.forEach((m, i) => {
    const color = MODEL_COLORS[i % MODEL_COLORS.length];
    const tokens = tokenMap[m.name] || { input: 0, output: 0, cacheRead: 0 };
    const hasTokens = tokens.input > 0 || tokens.output > 0 || tokens.cacheRead > 0;

    html += '' +
      '<div class="model-item">' +
        '<div class="model-top">' +
          '<span class="model-name">' + escapeHtml(m.name) + '</span>' +
          '<span class="model-count" style="color:' + color + '">' + m.calls.toLocaleString() + '</span>' +
        '</div>' +
        '<div class="model-meta">' +
          '<span>requests</span>' +
          '<span>' + formatPct(m.percentage) + ' of total</span>' +
        '</div>' +
        '<div class="model-bar">' +
          '<div style="width:' + Math.min(m.percentage, 100) + '%;height:100%;background:' + color + ';border-radius:2px;"></div>' +
        '</div>';

    if (hasTokens) {
      html += '' +
        '<div class="model-tokens">' +
          '<span class="tok-in" title="Input tokens">IN ' + formatTokens(tokens.input) + '</span>' +
          '<span class="tok-out" title="Output tokens">OUT ' + formatTokens(tokens.output) + '</span>' +
          '<span class="tok-cache" title="Cache read tokens">CACHE ' + formatTokens(tokens.cacheRead) + '</span>' +
        '</div>';
    }

    html += '</div>';
  });

  // Token summary
  let totalInput = 0, totalOutput = 0, totalCacheRead = 0;
  for (const t of Object.values(tokenMap)) {
    totalInput += t.input;
    totalOutput += t.output;
    totalCacheRead += t.cacheRead;
  }
  const totalTokens = totalInput + totalOutput + totalCacheRead;
  const cacheRate = totalTokens > 0 ? (totalCacheRead / (totalInput + totalOutput + totalCacheRead) * 100) : 0;

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
    mc.models.length + ' models tracked' +
  '</div>';

  html += '</div>';
  container.innerHTML = html;
}
