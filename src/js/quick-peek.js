import { escapeHtml, formatTimeAgo } from './format.js';
import { pickPrimaryRisk, formatInsightShort } from './insights.js';

let _quickPeekEl = null;
let _isOpen = false;
let _outsideClickHandler = null;

export function isQuickPeekOpen() {
  return _isOpen;
}

export function openQuickPeek() {
  _isOpen = true;
  ensureEl();
  if (_quickPeekEl) _quickPeekEl.classList.remove('hidden');

  // Close when the user clicks anywhere outside the overlay. Uses mousedown
  // (capture) so the close happens before any underlying control's click;
  // clicks inside the overlay are left alone.
  if (!_outsideClickHandler) {
    _outsideClickHandler = (e) => {
      if (!_isOpen || !_quickPeekEl) return;
      if (_quickPeekEl.contains(e.target)) return;
      // The title label reopens Quick Peek on click — let that click through
      // instead of closing+reopen flicker.
      if (e.target && e.target.id === 'app-title') return;
      closeQuickPeek();
    };
    document.addEventListener('mousedown', _outsideClickHandler, true);
  }
}

export function closeQuickPeek() {
  _isOpen = false;
  if (_quickPeekEl) _quickPeekEl.classList.add('hidden');
  if (_outsideClickHandler) {
    document.removeEventListener('mousedown', _outsideClickHandler, true);
    _outsideClickHandler = null;
  }
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
 * Threshold-based status class for a usage percentage.
 */
function pctClass(pct, threshold) {
  if (pct >= threshold) return 'qp-danger';
  if (pct >= threshold * 0.8) return 'qp-warning';
  return '';
}

/**
 * Render and show the Quick Peek overlay.
 * @param {object} state - { snapshot, settings, insights, currentTab, recentWorkspaces }
 * @param {object} actions - { refresh, switchTab, switchWorkspace, close }
 */
export function renderQuickPeek(state, actions) {
  ensureEl();
  if (!_quickPeekEl) return;

  const s = state.snapshot || {};
  const settings = state.settings || {};
  const insights = state.insights;
  const threshold = settings.usageThreshold || 80;

  const rollingPct = s.usage?.rolling?.usage_percent ?? 0;
  const weeklyPct  = s.usage?.weekly?.usage_percent  ?? 0;
  const monthlyPct = s.usage?.monthly?.usage_percent ?? 0;

  const ws = (s.workspaces || []).find(w => w.id === s.workspace_id);
  const wsName = ws?.name || s.workspace_id || '?';

  const budgetUsd = (settings.monthlyBudget || 0) / 100;
  const monthCostUsd = insights?.monthCostUsd ?? 0;
  const todayCostUsd = insights?.todayCostUsd ?? 0;
  const projected = insights?.projectedMonthlyCostUsd ?? 0;
  const budgetPace = insights?.projectedBudgetPct ?? 0;

  const primary = pickPrimaryRisk(insights);

  let html = '';

  // Head: workspace name + close button
  html += '<div class="quick-peek-head">' +
    '<span class="quick-peek-title">' + escapeHtml(wsName) + '</span>' +
    '<button class="quick-peek-close" data-qp-action="close" title="Close">&times;</button>' +
  '</div>';

  // Quota metrics — rolling / weekly / monthly
  html += '<div class="quick-peek-metrics">' +
    buildMetricPill('Rolling', rollingPct + '%', pctClass(rollingPct, threshold)) +
    buildMetricPill('Weekly', weeklyPct + '%', pctClass(weeklyPct, threshold)) +
    buildMetricPill('Monthly', monthlyPct + '%', pctClass(monthlyPct, threshold)) +
  '</div>';

  // Cost metrics — month / projection / today
  html += '<div class="quick-peek-metrics">' +
    buildMetricPill('Month cost', '$' + monthCostUsd.toFixed(2)) +
    buildMetricPill('Projection', '$' + projected.toFixed(2)) +
    buildMetricPill('Today', '$' + todayCostUsd.toFixed(2)) +
  '</div>';

  // Budget pace bar
  if (budgetUsd > 0) {
    const paceCapped = Math.min(budgetPace, 100);
    const barClass = budgetPace >= 100 ? 'qp-danger' : budgetPace >= 80 ? 'qp-warning' : '';
    html += '<div class="quick-peek-budget">' +
      '<div class="quick-peek-budget-label">' +
        '<span>Budget pace · $' + budgetUsd.toFixed(0) + '</span>' +
        '<strong class="' + barClass + '">' + Math.round(budgetPace) + '%</strong>' +
      '</div>' +
      '<div class="quick-peek-budget-bar">' +
        '<div class="quick-peek-budget-fill ' + barClass + '" style="width:' + paceCapped + '%"></div>' +
      '</div>' +
    '</div>';
  }

  // Primary insight (risk) line
  if (primary) {
    html += '<div class="quick-peek-insight ' + (primary.severity === 'danger' ? 'qp-danger' : primary.severity === 'warning' ? 'qp-warning' : 'qp-info') + '">' +
      escapeHtml(formatInsightShort(primary)) +
    '</div>';
  }

  // Last updated
  if (s.last_updated) {
    html += '<div class="quick-peek-updated">Updated ' + escapeHtml(formatTimeAgo(s.last_updated)) + '</div>';
  }

  // Actions row
  html += '<div class="quick-peek-actions">';
  html += '<button class="quick-peek-action" data-qp-action="refresh">Refresh</button>';
  html += '<button class="quick-peek-action" data-qp-action="tab-trends">Trends</button>';
  html += '<button class="quick-peek-action" data-qp-action="tab-settings">Settings</button>';
  html += '</div>';

  // Workspace quick-switch (max 3)
  const recents = state.recentWorkspaces || [];
  const otherWorkspaces = (s.workspaces || []).filter(w => w.id !== s.workspace_id && recents.includes(w.id));
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
      if (action === 'close') { actions.close(); return; }
      if (action === 'refresh') actions.refresh();
      else if (action === 'tab-trends') actions.switchTab('trends');
      else if (action === 'tab-settings') actions.switchTab('settings');
      else if (action === 'workspace') actions.switchWorkspace(btn.dataset.qpWorkspaceId);
    });
  });

  openQuickPeek();
}

function buildMetricPill(label, value, stateClass = '') {
  return '<div class="quick-peek-metric ' + stateClass + '">' +
    '<span>' + escapeHtml(label) + '</span>' +
    '<strong>' + escapeHtml(value) + '</strong>' +
  '</div>';
}
