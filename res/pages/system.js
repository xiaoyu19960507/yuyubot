const SystemPage = {
  data() {
    return {
      port: '-',
      dataDir: '-',
      pluginsDir: '-',
      loading: true,
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
    loadSystemInfo() {
      fetch('/api/system_info')
        .then(res => res.json())
        .then(data => {
          if (data.retcode === 0) {
            this.port = data.data.port;
            this.dataDir = data.data.data_dir;
            this.pluginsDir = data.data.plugins_root;
          }
          this.loading = false;
        })
        .catch(err => {
          console.error('Failed to load system info:', err);
          this.loading = false;
        });
    },
    openDataDir() {
      fetch('/api/open_data_dir', { method: 'POST' })
        .then(res => res.json())
        .catch(err => console.error('Failed to open directory:', err));
    },
    openPluginsDir() {
      fetch('/api/open_plugins_dir', { method: 'POST' })
        .then(res => res.json())
        .catch(err => console.error('Failed to open directory:', err));
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
        message: '确定要重启程序吗？所有正在运行的插件都将被停止。',
        onConfirm: () => {
          fetch('/api/restart_program', { method: 'POST' })
            .then(res => res.json())
            .then(data => {
              if (data.retcode === 0) {
                this.loading = true;
                if (window.showToast) {
                  window.showToast('程序正在重启，请稍候...', 'info');
                } else {
                  alert('程序正在重启，请稍候...');
                }
              }
            })
            .catch(err => console.error('Failed to restart program:', err));
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
        <div class="card-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>基本设置</div>
        <div class="config-form">
          <div class="form-group"><label>服务端口</label><input type="text" :value="port" readonly></div>
          
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

          <div class="form-group">
            <label>程序操作</label>
            <button @click="restartProgram" class="btn-danger">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 16px; height: 16px; margin-right: 4px;"><path d="M23 4v6h-6"></path><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"></path></svg>
              重启程序
            </button>
          </div>
        </div>
      </div>
      
      <!-- Confirmation Modal -->
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
