const SystemPage = {
  data() {
    return {
      port: '-',
      dataDir: '-',
      pluginsDir: '-',
      autoStart: false,
      loading: true,
      savingAutoStart: false,
      confirmDialog: {
        show: false,
        title: '',
        message: '',
        onConfirm: null
      }
    };
  },
  mounted() {
    this.loadSystemInfo();
  },
  methods: {
    async loadSystemInfo() {
      try {
        const response = await fetch('/api/system_info');
        const result = await response.json();
        if (result.retcode === 0) {
          this.port = result.data.port;
          this.dataDir = result.data.data_dir;
          this.pluginsDir = result.data.plugins_root;
          this.autoStart = !!result.data.auto_start;
        }
      } catch (err) {
        console.error('Failed to load system info:', err);
      } finally {
        this.loading = false;
      }
    },
    async openDataDir() {
      try {
        await fetch('/api/open_data_dir', { method: 'POST' });
      } catch (err) {
        console.error('Failed to open data directory:', err);
      }
    },
    async openPluginsDir() {
      try {
        await fetch('/api/open_plugins_dir', { method: 'POST' });
      } catch (err) {
        console.error('Failed to open plugins directory:', err);
      }
    },
    showToast(message, type = 'info') {
      if (window.showToast) {
        window.showToast(message, type);
      }
    },
    async handleAutoStartChange(event) {
      const nextValue = event.target.checked;
      const previousValue = this.autoStart;

      this.autoStart = nextValue;
      this.savingAutoStart = true;

      try {
        const response = await fetch('/api/system/save_config', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json'
          },
          body: JSON.stringify({
            autoStart: nextValue
          })
        });

        const result = await response.json();
        if (result.retcode !== 0) {
          throw new Error(result.data || '保存系统配置失败');
        }

        this.showToast(nextValue ? '已开启开机自启' : '已关闭开机自启', 'success');
      } catch (err) {
        console.error('Failed to save auto-start setting:', err);
        this.autoStart = previousValue;
        event.target.checked = previousValue;
        this.showToast(`保存失败：${err.message}`, 'error');
      } finally {
        this.savingAutoStart = false;
      }
    },
    confirmAction() {
      if (this.confirmDialog.onConfirm) {
        this.confirmDialog.onConfirm();
      }
      this.confirmDialog.show = false;
    },
    restartProgram() {
      this.confirmDialog = {
        show: true,
        title: '重启程序',
        message: '确定要重启程序吗？正在运行的插件会被停止，随后程序会重新启动。',
        onConfirm: async () => {
          try {
            const response = await fetch('/api/restart_program', { method: 'POST' });
            const result = await response.json();
            if (result.retcode === 0) {
              this.loading = true;
              this.showToast('程序正在重启，请稍候…', 'info');
            } else {
              this.showToast(`重启失败：${result.data}`, 'error');
            }
          } catch (err) {
            console.error('Failed to restart program:', err);
            this.showToast(`重启失败：${err.message}`, 'error');
          }
        }
      };
    }
  },
  template: `
    <div>
      <div class="page-header">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path></svg>
        <h1>系统配置</h1>
      </div>

      <div class="card">
        <div class="card-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>基本信息</div>
        <div class="config-form">
          <div class="form-group">
            <label>服务端口</label>
            <input type="text" :value="port" readonly>
          </div>

          <div style="display: flex; gap: 8px; align-items: flex-end;">
            <div class="form-group" style="flex: 1; margin-bottom: 0;">
              <label>插件目录</label>
              <input type="text" :value="pluginsDir" readonly>
            </div>
            <button @click="openPluginsDir" class="btn-primary">打开</button>
          </div>

          <div style="display: flex; gap: 8px; align-items: flex-end;">
            <div class="form-group" style="flex: 1; margin-bottom: 0;">
              <label>数据目录</label>
              <input type="text" :value="dataDir" readonly>
            </div>
            <button @click="openDataDir" class="btn-primary">打开</button>
          </div>
        </div>
      </div>

      <div class="card">
        <div class="card-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2v20"></path><path d="M18 8l-6-6-6 6"></path><path d="M5 22h14"></path></svg>启动设置</div>
        <div class="info-panel" style="margin-bottom: 16px;">
          <div class="text-muted">开启后，程序会在 Windows 登录后自动启动，并以托盘方式在后台运行。</div>
        </div>

        <div class="switch-row">
          <div>
            <div style="font-size: 14px; font-weight: 600; margin-bottom: 4px;">开机自启</div>
            <div class="text-muted">仅对当前 Windows 用户生效。</div>
          </div>
          <label class="switch">
            <input
              type="checkbox"
              :checked="autoStart"
              :disabled="loading || savingAutoStart"
              @change="handleAutoStartChange"
            >
            <span class="slider"></span>
          </label>
        </div>

        <div v-if="savingAutoStart" class="text-muted" style="margin-top: 12px;">正在保存启动设置…</div>
      </div>

      <div class="card">
        <div class="card-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M23 4v6h-6"></path><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"></path></svg>程序操作</div>
        <button @click="restartProgram" class="btn-danger" style="width: 100%;">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M23 4v6h-6"></path><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"></path></svg>
          重启程序
        </button>
      </div>

      <div v-if="confirmDialog.show" class="modal-overlay" @click="confirmDialog.show = false">
        <div class="modal" @click.stop>
          <div class="modal-header">{{ confirmDialog.title }}</div>
          <div class="modal-body">{{ confirmDialog.message }}</div>
          <div class="modal-footer">
            <button class="btn-text" @click="confirmDialog.show = false">取消</button>
            <button class="btn-primary" @click="confirmAction">确定</button>
          </div>
        </div>
      </div>
    </div>
  `
};
