/**
 * Pure functions for the OpenCode Usage Panel.
 * These have no side-effects and can be verified in DevTools console.
 */

export function pickMiniBadgeUsage(snapshot, source) {
  const rollingPct = snapshot.usage?.rolling?.usage_percent ?? 0;
  const weeklyPct = snapshot.usage?.weekly?.usage_percent ?? 0;
  const monthlyPct = snapshot.usage?.monthly?.usage_percent ?? 0;

  if (source === 'rolling') {
    return { percentage: rollingPct, label: 'Rolling' };
  }
  if (source === 'weekly') {
    return { percentage: weeklyPct, label: 'Weekly' };
  }
  if (source === 'monthly') {
    return { percentage: monthlyPct, label: 'Monthly' };
  }

  // auto
  const percentage = Math.max(rollingPct, weeklyPct, monthlyPct);
  let label;
  if (percentage === monthlyPct && percentage > weeklyPct) {
    label = 'Monthly';
  } else if (percentage === weeklyPct && percentage > rollingPct) {
    label = 'Weekly';
  } else {
    label = 'Rolling';
  }
  return { percentage, label };
}

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

export function buildTrendSeries(history) {
  if (!Array.isArray(history) || history.length === 0) return null;
  return {
    labels: history.map(e => e.date),
    rolling: history.map(e => e.rolling_pct),
    weekly: history.map(e => e.weekly_pct),
    monthly: history.map(e => e.monthly_pct),
  };
}
