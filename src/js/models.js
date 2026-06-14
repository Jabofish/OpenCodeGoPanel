import { formatPct, escapeHtml } from './format.js';

const MODEL_COLORS = ['#8a9eff', '#5cc08a', '#e0a050', '#7b7bbb', '#5aaac0', '#c080d0'];

export function renderModelsTab(snapshot) {
  const container = document.getElementById('tab-models');
  if (!container) return;

  const mc = snapshot.model_calls;
  if (!mc || !mc.models || mc.models.length === 0) {
    container.innerHTML = '<div class="loading">No model call data yet</div>';
    return;
  }

  const visibleModels = mc.models.slice(0, 6);
  let html = '<div class="model-panel">';
  html += '<div class="model-panel-head"><span>Total calls</span><strong>' + mc.total_calls.toLocaleString() + '</strong></div>';

  visibleModels.forEach((m, i) => {
    const color = MODEL_COLORS[i % MODEL_COLORS.length];
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
        '</div>' +
      '</div>';
  });

  html += '<div class="model-summary">' +
    mc.models.length + ' models tracked' +
  '</div>';

  html += '</div>';
  container.innerHTML = html;
}
