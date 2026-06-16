/**
 * Pure functions for deriving usage insights from snapshot + history.
 * No DOM operations — call from app.js and pass results to render functions.
 */

const OPENCODE_COST_UNITS_PER_USD = 100000000;

/**
 * Compute usage insights from current state.
 * @param {object} snapshot - AppDataSnapshot
 * @param {Array} history - HistoryEntry array
 * @param {object} settings - AppSettings
 * @param {Date} [now] - Current date (injectable for testing)
 * @returns {object}
 */
export function deriveUsageInsights(snapshot, history, settings, now = new Date()) {
  const msgs = [];
  // monthlyBudget is already in cents (100 cents = $1), no need to divide again
  const budgetUsd = (settings.monthlyBudget || 0) / 100;
  const threshold = settings.usageThreshold || 0;

  // --- Cost analysis ---
  const dailyCosts = snapshot.daily_costs || [];
  const nowY = now.getFullYear();
  const nowM = now.getMonth() + 1;
  const todayStr = `${nowY}-${String(nowM).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;

  let monthCostUnits = 0;
  let todayCostUnits = 0;
  const monthDays = new Set();

  for (const c of dailyCosts) {
    const d = c.date || '';
    // Check month match: YYYY-MM prefix
    const parts = d.split('-');
    if (parts.length < 2) continue;
    if (parseInt(parts[0], 10) !== nowY || parseInt(parts[1], 10) !== nowM) continue;

    monthCostUnits += c.totalCost || 0;
    monthDays.add(d);
    if (d === todayStr) todayCostUnits += c.totalCost || 0;
  }

  const monthCostUsd = monthCostUnits / OPENCODE_COST_UNITS_PER_USD;
  const todayCostUsd = todayCostUnits / OPENCODE_COST_UNITS_PER_USD;

  // Debug logging
  console.log('[Insights] Budget calc:', {
    nowY, nowM, dayOfMonth: now.getDate(),
    dailyCostsCount: dailyCosts.length,
    monthDaysCount: monthDays.size,
    monthCostUnits,
    monthCostUsd: monthCostUsd.toFixed(4),
    budgetUsd: budgetUsd.toFixed(2),
  });

  // Projected monthly cost
  const dayOfMonth = now.getDate();
  const daysInMonth = new Date(nowY, nowM, 0).getDate();
  const monthElapsedPct = daysInMonth > 0 ? dayOfMonth / daysInMonth * 100 : 0;
  const projectedMonthlyCostUsd = dayOfMonth > 1
    ? monthCostUsd / dayOfMonth * daysInMonth
    : monthCostUsd;
  const projectedBudgetPct = budgetUsd > 0 ? projectedMonthlyCostUsd / budgetUsd * 100 : 0;

  // Daily average (exclude today)
  const historicalDayCount = Math.max(monthDays.size - (todayCostUnits > 0 ? 1 : 0), 1);
  const dailyAverageCostUsd = monthCostUsd / Math.max(monthDays.size || 1, 1);
  const todayVsAveragePct = dailyAverageCostUsd > 0 ? todayCostUsd / dailyAverageCostUsd * 100 : 0;

  // --- Budget projection message ---
  if (budgetUsd > 0 && projectedBudgetPct >= 100) {
    msgs.push({
      id: 'budget-projection',
      severity: 'danger',
      title: 'Projected over budget',
      detail: `Projected $${projectedMonthlyCostUsd.toFixed(2)} exceeds $${budgetUsd.toFixed(2)} budget.`,
      metric: Math.round(projectedBudgetPct) + '%',
    });
  } else if (budgetUsd > 0 && projectedBudgetPct >= 80) {
    msgs.push({
      id: 'budget-projection',
      severity: 'warning',
      title: 'Budget pace high',
      detail: `On track for ${Math.round(projectedBudgetPct)}% of monthly budget.`,
      metric: Math.round(projectedBudgetPct) + '%',
    });
  }

  // --- Cost spike today ---
  if (todayCostUsd >= dailyAverageCostUsd * 1.8 && todayCostUsd >= 0.25) {
    msgs.push({
      id: 'cost-spike',
      severity: 'warning',
      title: 'Cost spike today',
      detail: `Today $${todayCostUsd.toFixed(2)} vs avg $${dailyAverageCostUsd.toFixed(2)}.`,
      metric: Math.round(todayVsAveragePct) + '% of avg',
    });
  }

  // --- Riskiest quota ---
  const rolling = snapshot.usage?.rolling || {};
  const weekly = snapshot.usage?.weekly || {};
  const monthly = snapshot.usage?.monthly || {};

  const quotas = [
    { key: 'rolling', label: 'Rolling', percent: rolling.usage_percent || 0, resetInSec: rolling.reset_in_sec || 0 },
    { key: 'weekly', label: 'Weekly', percent: weekly.usage_percent || 0, resetInSec: weekly.reset_in_sec || 0 },
    { key: 'monthly', label: 'Monthly', percent: monthly.usage_percent || 0, resetInSec: monthly.reset_in_sec || 0 },
  ];
  quotas.sort((a, b) => b.percent - a.percent);
  const riskiestQuota = quotas[0];

  if (threshold >= 50 && riskiestQuota.percent >= threshold) {
    msgs.push({
      id: 'quota-threshold',
      severity: 'danger',
      title: riskiestQuota.label + ' quota critical',
      detail: `${riskiestQuota.label} usage at ${riskiestQuota.percent}% (threshold ${threshold}%).`,
      metric: riskiestQuota.percent + '%',
    });
  } else if (threshold >= 50 && riskiestQuota.percent >= threshold * 0.8) {
    msgs.push({
      id: 'quota-warning',
      severity: 'warning',
      title: riskiestQuota.label + ' quota high',
      detail: `${riskiestQuota.label} approaching threshold.`,
      metric: riskiestQuota.percent + '%',
    });
  } else if (riskiestQuota.percent > 0) {
    msgs.push({
      id: 'quota-info',
      severity: 'info',
      title: riskiestQuota.label + ' usage',
      detail: `${riskiestQuota.label} at ${riskiestQuota.percent}%.`,
      metric: riskiestQuota.percent + '%',
    });
  }

  // --- Trend spike (7-day rolling comparison) ---
  if (Array.isArray(history) && history.length >= 3) {
    const recent = history.slice(-7);
    const avgRolling = recent.reduce((s, e) => s + (e.rolling_pct || 0), 0) / recent.length;
    if ((rolling.usage_percent || 0) - avgRolling > 20) {
      msgs.push({
        id: 'trend-surge',
        severity: 'warning',
        title: 'Usage surging',
        detail: `Rolling ${rolling.usage_percent}% vs 7-day avg ${Math.round(avgRolling)}%.`,
        metric: '+' + Math.round(rolling.usage_percent - avgRolling) + ' pts',
      });
    }
  }

  return {
    projectedMonthlyCostUsd,
    projectedBudgetPct,
    monthElapsedPct,
    monthCostUsd,
    budgetUsd,
    dailyAverageCostUsd,
    todayCostUsd,
    todayVsAveragePct,
    riskiestQuota,
    messages: msgs,
  };
}

/**
 * Pick the most important insight message.
 * Order: danger > warning > info. Among same severity, budget > quota > spike > trend.
 */
export function pickPrimaryRisk(insights) {
  if (!insights || !insights.messages || insights.messages.length === 0) return null;
  const msgs = insights.messages;
  const prio = { danger: 3, warning: 2, info: 1 };
  msgs.sort((a, b) => (prio[b.severity] || 0) - (prio[a.severity] || 0));
  return msgs[0];
}

/**
 * Format a short insight string for tooltip display.
 */
export function formatInsightShort(insight) {
  if (!insight) return '';
  return insight.title + (insight.metric ? ' · ' + insight.metric : '');
}
