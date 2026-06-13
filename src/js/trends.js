const MODEL_COLORS = ['#8a9eff', '#5cc08a', '#e0a050', '#7b7bbb', '#5aaac0', '#c080d0'];

export function renderTrendsTab(snapshot, chartInstances) {
  const container = document.getElementById('tab-trends');
  if (!container) return;

  // Always rebuild chart containers
  let html = '' +
    '<div class="card">' +
      '<div class="card-label" style="margin-bottom:8px;">Usage Trend - Recent Activity</div>' +
      '<div class="trend-box"><canvas id="chart-usage" height="140"></canvas></div>' +
    '</div>' +
    '<div class="card">' +
      '<div class="card-label" style="margin-bottom:8px;">Model Distribution</div>' +
      '<div class="trend-box"><canvas id="chart-models" height="140"></canvas></div>' +
    '</div>';

  container.innerHTML = html;

  // Destroy existing charts
  if (chartInstances) {
    Object.values(chartInstances).forEach(c => c && c.destroy && c.destroy());
    Object.keys(chartInstances).forEach(k => delete chartInstances[k]);
  }

  // Get snapshot if not provided
  if (!snapshot) {
    // Will be rendered when data is available
    return;
  }

  // Chart 1: Usage trend (placeholder - shows current percentages as bars)
  const ctx1 = document.getElementById('chart-usage');
  if (ctx1 && window.Chart && snapshot.usage) {
    chartInstances.usage = new Chart(ctx1, {
      type: 'bar',
      data: {
        labels: ['Rolling', 'Weekly', 'Monthly'],
        datasets: [{
          label: 'Usage %',
          data: [
            snapshot.usage.rolling?.usage_percent || 0,
            snapshot.usage.weekly?.usage_percent || 0,
            snapshot.usage.monthly?.usage_percent || 0
          ],
          backgroundColor: ['#8a9eff', '#5cc08a', '#e0a050'],
          borderRadius: 4,
          barPercentage: 0.6
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: { legend: { display: false } },
        scales: {
          x: { ticks: { color: '#5a5a78', font: { size: 10 } }, grid: { display: false } },
          y: {
            ticks: { color: '#5a5a78', font: { size: 9 }, callback: v => v + '%' },
            grid: { color: '#1a1a28' },
            max: 100
          }
        }
      }
    });
  }

  // Chart 2: Model distribution (doughnut)
  const ctx2 = document.getElementById('chart-models');
  if (ctx2 && window.Chart && snapshot.model_calls && snapshot.model_calls.models.length > 0) {
    const models = snapshot.model_calls.models.slice(0, 6); // Top 6

    chartInstances.models = new Chart(ctx2, {
      type: 'doughnut',
      data: {
        labels: models.map(m => m.name),
        datasets: [{
          data: models.map(m => m.percentage),
          backgroundColor: models.map((_, i) => MODEL_COLORS[i % MODEL_COLORS.length]),
          borderWidth: 0
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: {
          legend: {
            position: 'bottom',
            labels: {
              color: '#6e6e8a',
              font: { size: 9 },
              boxWidth: 10,
              padding: 8
            }
          }
        }
      }
    });
  }
}
