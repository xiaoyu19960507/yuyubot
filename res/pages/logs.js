const LogsPage = {
  data() {
    return {
      logs: [],
      eventSource: null,
      autoRefresh: true,
    };
  },
  template: `
    <div class="logs-page">
      <div class="page-header">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline></svg>
        <h1>日志查看</h1>
      </div>
      <div class="card">
        <div class="card-title">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="8" y1="6" x2="21" y2="6"></line><line x1="8" y1="12" x2="21" y2="12"></line><line x1="8" y1="18" x2="21" y2="18"></line></svg>
          运行日志
        </div>
        <div class="log-controls">
          <label class="toggle-switch">
            <input type="checkbox" v-model="autoRefresh" @change="onAutoRefreshChange">
            <span class="toggle-slider"></span>
            <span class="toggle-label">自动刷新</span>
          </label>
          <button class="btn-clear" @click="clearLogs">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path><line x1="10" y1="11" x2="10" y2="17"></line><line x1="14" y1="11" x2="14" y2="17"></line></svg>
            清空日志
          </button>
        </div>
        <div class="log-container">
          <div v-if="logs.length === 0" style="text-align: center; color: var(--text-secondary); padding: 20px;">
            暂无日志
          </div>
          <div v-for="(log, i) in logs" :key="i" class="log-item">
            <span class="log-time">{{ log.time }}</span>
            <span class="log-source">{{ log.source }}</span>
            <span :class="'log-' + log.level">{{ log.message }}</span>
          </div>
        </div>
      </div>
    </div>
  `,
  mounted() {
    this.loadInitialLogs();
    this.connectSSE();
  },
  beforeUnmount() {
    if (this.eventSource) {
      this.eventSource.close();
    }
  },
  methods: {
    loadInitialLogs() {
      fetch('/api/logs')
        .then(res => res.json())
        .then(data => {
          if (data.retcode === 0) {
            this.logs = data.data.logs;
          }
        })
        .catch(err => console.error('Failed to load logs:', err));
    },
    connectSSE() {
      if (!this.autoRefresh) return;
      
      const sseUrl = `/api/logs/stream`;
      
      this.eventSource = new EventSource(sseUrl);
      
      this.eventSource.onmessage = (event) => {
        try {
          const log = JSON.parse(event.data);
          this.logs.push(log);
          // 保持最多 1000 条日志
          if (this.logs.length > 1000) {
            this.logs.shift();
          }
          // 自动滚动到底部
          this.$nextTick(() => {
            const container = document.querySelector('.log-container');
            if (container) {
              container.scrollTop = container.scrollHeight;
            }
          });
        } catch (err) {
          console.error('Failed to parse log:', err);
        }
      };
      
      this.eventSource.onerror = (error) => {
        console.error('SSE error:', error);
        console.log('SSE closed, reconnecting in 3s...');
        if (this.autoRefresh) {
          setTimeout(() => this.connectSSE(), 3000);
        }
      };
    },
    onAutoRefreshChange() {
      if (this.autoRefresh) {
        this.connectSSE();
      } else if (this.eventSource) {
        this.eventSource.close();
        this.eventSource = null;
      }
    },
    clearLogs() {
      fetch('/api/logs/clear', { method: 'POST' })
        .then(res => res.json())
        .then(data => {
          if (data.retcode === 0) {
            this.logs = [];
          }
        })
        .catch(err => console.error('Failed to clear logs:', err));
    }
  }
};
