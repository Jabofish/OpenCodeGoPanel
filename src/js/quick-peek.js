import { escapeHtml } from './format.js';

const OPENCODE_COST_UNITS_PER_USD = 100000000;

let _quickPeekEl = null;
let _isOpen = false;

export function isQuickPeekOpen() {
  return _isOpen;
}

export function openQuickPeek() {
  _isOpen = true;
  ensureEl();
  if (_quickPeekEl) _quickPeekEl.classList.remove('hidden');
}

export function closeQuickPeek() {
  _isOpen = false;
  if (_quickPeekEl) _quickPeekEl.classList.add('hidden');
}

function ensureEl() {
  if (_quickPeekEl) return;
  _quickPeekEl = document.createElement('div');
  _quickPeekEl.id = 'quick-peek';
  _quickPeekEl.className = 'quick-peek hidden';
  const app = document.getElementById('app');
  if (app) app.appendChild(_quickPeekEl);
}

/**
 * Render and show the Quick Peek overlay.
 * @param {object} state - { snapshot, settings, insights, currentTab, recentWorkspaces }
 * @param {object} actions - { refresh, switchTab, switchWorkspace, close }
 */
export function renderQuickPeek(state, actions) {
  ensureEl();
  if (!_quickPeekEl) return;

  const s = state.snapshot;
  const rollingPct = s?.usage?.rolling?.usage_percent ?? 0;
  const ws = (s?.workspaces || []).find(w => w.id === s?.workspace_id);
  const wsName = ws?.name || s?.workspace_id || '?';
  const budgetUsd = (state.settings?.monthlyBudget || 0) / 100;

  // Month cost from insights or daily_costs
  let monthCostUsd = 0;
  if (state.insights) {
    monthCostUsd = state.insights.monthCostUsd;
  } else if (s?.daily_costs) {
    monthCostUsd = s.daily_costs.reduce((sum, c) => sum + (c.totalCost || 0), 0) / OPENCODE_COST_UNITS_PER_USD;
  }

  // Projection from insights
  let projection = '?';
  if (state.insights) {
    projection = '$' + state.insights.projectedMonthlyCostUsd.toFixed(2);
  }

  let html = '';

  // Head: workspace name + close hint
  html += '<div class="quick-peek-head">' +
    '<span>' + escapeHtml(wsName) + '</span>' +
    '<span style="color:var(--text-dim);font-size:9px;font-weight:400">Esc to close</span>' +
  '</div>';

  // Metrics row
  html += '<div class="quick-peek-metrics">' +
    buildMetricPill('Rolling', rollingPct + '%') +
    buildMetricPill('Month cost', '$' + monthCostUsd.toFixed(2)) +
    buildMetricPill('Projection', projection) +
  '</div>';

  // Actions row
  html += '<div class="quick-peek-actions">';
  html += '<button class="quick-peek-action" data-qp-action="refresh">Refresh</button>';
  html += '<button class="quick-peek-action" data-qp-action="tab-trends">Trends</button>';
  html += '<button class="quick-peek-action" data-qp-action="tab-settings">Settings</button>';
  html += '</div>';

  // Workspace quick-switch (max 3)
  const recents = state.recentWorkspaces || [];
  const otherWorkspaces = (s?.workspaces || []).filter(w => w.id !== s?.workspace_id && recents.includes(w.id));
  if (otherWorkspaces.length > 0) {
    html += '<div class="quick-peek-actions" style="margin-top:4px">';
    otherWorkspaces.slice(0, 3).forEach(w => {
      html += '<button class="quick-peek-action" data-qp-action="workspace" data-qp-workspace-id="' +
        escapeHtml(w.id) + '">' + escapeHtml(w.name || w.id) + '</button>';
    });
    html += '</div>';
  }

  _quickPeekEl.innerHTML = html;

  // Bind click events
  _quickPeekEl.querySelectorAll('[data-qp-action]').forEach(btn => {
    btn.addEventListener('click', () => {
      const action = btn.dataset.qpAction;
      if (action === 'refresh') actions.refresh();
      else if (action === 'tab-trends') actions.switchTab('trends');
      else if (action === 'tab-settings') actions.switchTab('settings');
      else if (action === 'workspace') actions.switchWorkspace(btn.dataset.qpWorkspaceId);
    });
  });

  openQuickPeek();
}

function buildMetricPill(label, value) {
  return '<div class="quick-peek-metric">' +
    '<span>' + escapeHtml(label) + '</span>' +
    '<strong>' + escapeHtml(value) + '</strong>' +
  '</div>';
}
