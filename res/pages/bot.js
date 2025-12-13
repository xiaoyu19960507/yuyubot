const BotPage = {
  props: ['botConfig'],
  data() {
    return {
      loading: false,
      message: '',
      connected: false,
      connecting: false,
      connectionStatus: '未连接',
      statusEventSource: null,
      loginInfo: null
    };
  },
  methods: {
    async loadConfig() {
      try {
        const response = await fetch('/api/bot/get_config');
        const result = await response.json();
        if (result.retcode === 0) {
          Object.assign(this.botConfig, result.data);
        }
      } catch (error) {
        console.error('Failed to load config:', error);
      }
    },
    async loadConnectionStatus() {
      try {
        const response = await fetch('/api/bot/get_status');
        const result = await response.json();
        if (result.retcode === 0) {
          this.updateStatus(result.data);
        }
      } catch (error) {
        console.error('Failed to load connection status:', error);
      }
    },
    
    updateStatus(status) {
      this.connected = status.connected;
      this.connecting = status.connecting;
      
      if (status.connected) {
        this.connectionStatus = '已连接';
        // 连接成功后获取登录信息
        this.fetchLoginInfo();
      } else {
        // 未连接或正在连接时都清除头像
        this.connectionStatus = status.connecting ? '正在连接...' : '未连接';
        this.loginInfo = null;
        this.$root.clearUserAvatar();
      }
    },
    
    async fetchLoginInfo() {
      try {
        const response = await fetch('/api/get_login_info', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json'
          },
          body: JSON.stringify({})
        });
        const result = await response.json();
        if (result.retcode === 0) {
          this.loginInfo = result.data;
          // 通知父组件更新头像
          this.$root.updateUserAvatar(result.data);
        }
      } catch (error) {
        console.error('Failed to fetch login info:', error);
      }
    },
    
    connectStatusSSE() {
      if (this.statusEventSource) {
        this.statusEventSource.close();
      }
      
      const sseUrl = `/api/bot/status_stream`;
      
      this.statusEventSource = new EventSource(sseUrl);
      
      this.statusEventSource.onopen = () => {
        console.log('Bot status SSE connected');
      };
      
      this.statusEventSource.onmessage = (event) => {
        try {
          const status = JSON.parse(event.data);
          this.updateStatus(status);
        } catch (error) {
          console.error('Failed to parse status message:', error);
        }
      };
      
      this.statusEventSource.onerror = (error) => {
        console.error('Bot status SSE error:', error);
        console.log('Bot status SSE disconnected');
        // 尝试重连
        setTimeout(() => {
          if (this.statusEventSource && this.statusEventSource.readyState === EventSource.CLOSED) {
            this.connectStatusSSE();
          }
        }, 3000);
      };
    },

    
    async toggleConnection() {
      this.loading = true;
      this.message = '';
      
      try {
        if (this.connected) {
          // 断开连接
          const response = await fetch('/api/bot/disconnect', { method: 'POST' });
          const result = await response.json();
          if (result.retcode === 0) {
            this.connected = false;
            this.connecting = false;
            this.connectionStatus = '未连接';
            this.message = '已断开连接';
          } else {
            this.message = '断开失败: ' + result.data;
          }
        } else if (this.connecting) {
          // 如果正在连接，点击按钮取消连接
          const response = await fetch('/api/bot/disconnect', { method: 'POST' });
          const result = await response.json();
          if (result.retcode === 0) {
            this.message = '已取消连接';
          } else {
            this.message = '取消失败: ' + result.data;
          }
        } else {
          // 开始连接 - 保存配置并连接
          const response = await fetch('/api/bot/save_config', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json'
            },
            body: JSON.stringify(this.botConfig)
          });
          const result = await response.json();
          if (result.retcode === 0) {
            this.message = '开始连接...';
            setTimeout(() => { this.message = ''; }, 2000);
          } else {
            this.message = '连接失败: ' + result.data;
          }
        }
      } catch (error) {
        this.message = '操作失败: ' + error.message;
      } finally {
        this.loading = false;
      }
    },
    
    getButtonClass() {
      if (this.connected) {
        return 'btn-success'; // 绿色
      } else if (this.connecting) {
        return 'btn-warning'; // 黄色
      } else {
        return 'btn-primary'; // 默认颜色
      }
    },
    
    getButtonText() {
      if (this.loading) {
        return '处理中...';
      } else if (this.connected) {
        return '已连接，点我断开';
      } else if (this.connecting) {
        return '正在连接...，点我取消';
      } else {
        return '未连接，点我连接';
      }
    },
    
    isInputDisabled() {
      return this.connected || this.connecting;
    }
  },
  mounted() {
    this.loadConfig();
    this.loadConnectionStatus();
    this.connectStatusSSE();
  },
  
  beforeUnmount() {
    if (this.statusEventSource) {
      this.statusEventSource.close();
      this.statusEventSource = null;
    }
  },
  template: `
    <div>
      <div class="page-header">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="4" y1="21" x2="4" y2="14"></line><line x1="4" y1="10" x2="4" y2="3"></line><line x1="12" y1="21" x2="12" y2="12"></line><line x1="12" y1="8" x2="12" y2="3"></line><line x1="20" y1="21" x2="20" y2="16"></line><line x1="20" y1="12" x2="20" y2="3"></line><line x1="1" y1="14" x2="7" y2="14"></line><line x1="9" y1="8" x2="15" y2="8"></line><line x1="17" y1="16" x2="23" y2="16"></line></svg>
        <h1>Bot配置</h1>
      </div>
      <div class="card">
        <div class="card-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>连接设置</div>
        <div style="padding: 12px 15px; margin-bottom: 15px; background: var(--hover-bg); border-radius: 8px; font-size: 13px; color: var(--text-secondary);">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 16px; height: 16px; display: inline-block; vertical-align: middle; margin-right: 6px;"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>
          使用 <a href="https://milky.ntqqrev.org/" target="_blank" style="color: var(--accent-color); text-decoration: none;">Milky 协议</a> 进行通信
        </div>
        <div class="config-form">
          <div class="form-group"><label>Host</label><input type="text" v-model="botConfig.host" placeholder="localhost" :disabled="isInputDisabled()"></div>
          
          <div class="form-group"><label>API 端口</label><input type="number" v-model.number="botConfig.apiPort" placeholder="3010" :disabled="isInputDisabled()"></div>
          
          <div class="form-group"><label>Event 端口</label><input type="number" v-model.number="botConfig.eventPort" placeholder="3011" :disabled="isInputDisabled()"></div>

          <div class="form-group"><label>Token (可选)</label><input type="password" v-model="botConfig.token" placeholder="共用于 API 和 Event" :disabled="isInputDisabled()"></div>
          
          <div class="btn-center">

            <button :class="getButtonClass()" @click="toggleConnection" :disabled="loading">{{ getButtonText() }}</button>
          </div>
          <div v-if="message" style="margin-top: 12px; padding: 10px; border-radius: 4px; text-align: center; background: var(--hover-bg); color: var(--text-secondary);">{{ message }}</div>
        </div>
      </div>
    </div>
  `
};
