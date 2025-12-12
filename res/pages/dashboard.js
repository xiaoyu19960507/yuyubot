const DashboardPage = {
  props: ['stats'],
  template: `
    <div>
      <div class="page-header">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"></rect><rect x="14" y="3" width="7" height="7"></rect><rect x="14" y="14" width="7" height="7"></rect><rect x="3" y="14" width="7" height="7"></rect></svg>
        <h1>控制面板</h1>
      </div>
      <div class="card">
        <div class="card-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 12h-4l-3 9L9 3l-3 9H2"></path></svg>运行状态</div>
        <div class="stat-grid">
          <div class="stat-item"><div class="stat-value">{{ stats.messages }}</div><div class="stat-label">今日消息</div></div>
          <div class="stat-item"><div class="stat-value">{{ stats.groups }}</div><div class="stat-label">群组数量</div></div>
          <div class="stat-item"><div class="stat-value">{{ stats.friends }}</div><div class="stat-label">好友数量</div></div>
          <div class="stat-item"><div class="stat-value">{{ stats.uptime }}</div><div class="stat-label">运行时间</div></div>
        </div>
      </div>
      <div class="card">
        <div class="card-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"></circle><polyline points="12 6 12 12 16 14"></polyline></svg>快捷操作</div>
        <div style="display: flex; gap: 10px; flex-wrap: wrap;">
          <button class="btn-primary"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"></polyline><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"></path></svg>重启Bot</button>
          <button class="btn-primary"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg>清除缓存</button>
        </div>
      </div>
    </div>
  `
};
