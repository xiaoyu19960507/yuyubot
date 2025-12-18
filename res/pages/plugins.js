const MAX_OUTPUT_LINES = 500; // 前端最大显示行数

const PluginsPage = {
  data() {
    return {
      plugins: [],
      selectedPlugin: null,
      eventSources: {},
      statusEventSource: null,
      loading: false,
      autoScroll: true,
      pendingStatusUpdates: {},
      confirmDialog: {
        show: false,
        title: '',
        message: '',
        onConfirm: null
      }
    };
  },
  template: `
    <div>
      <div class="page-header">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2L2 7l10 5 10-5-10-5z"></path><path d="M2 17l10 5 10-5"></path><path d="M2 12l10 5 10-5"></path></svg>
        <h1>插件管理</h1>
      </div>
      
      <div class="card" style="height: calc(100vh - 170px); display: flex; flex-direction: column;">
        <div class="card-title" style="flex-shrink: 0;">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"></rect><rect x="14" y="3" width="7" height="7"></rect><rect x="14" y="14" width="7" height="7"></rect><rect x="3" y="14" width="7" height="7"></rect></svg>
          插件列表
          <div style="margin-left: auto; display: flex; align-items: center; gap: 10px;">
            <button class="btn-primary" @click="importPlugin" :disabled="loading" style="padding: 6px 12px; text-align: center;">
              导入插件
            </button>
            <label v-if="selectedPlugin" class="toggle-switch" style="font-weight: normal; font-size: 14px; margin: 0;">
              <input type="checkbox" v-model="autoScroll">
              <span class="toggle-slider"></span>
              <span class="toggle-label">自动滚动</span>
            </label>
          </div>
        </div>
        
        <div class="plugin-list-container" style="flex: 1; min-height: 0;">
          <div v-if="plugins.length === 0" style="text-align: center; color: var(--text-secondary); padding: 40px;">
            <p>暂无插件</p>
            <p style="font-size: 12px; margin-top: 10px;">请在 app 目录下创建插件文件夹</p>
          </div>
        
          <div v-for="plugin in plugins" :key="plugin.id" class="plugin-card" @click="togglePlugin(plugin.id)">
          <div class="plugin-header">
            <div class="plugin-info">
              <h3 class="plugin-name">{{ plugin.name }} <span style="font-size: 12px; color: var(--text-secondary);">({{ plugin.id }})</span></h3>
              <p class="plugin-description">{{ plugin.description }}</p>
              <p v-if="plugin.author" class="plugin-author">作者: {{ plugin.author }}</p>
              <div class="plugin-meta">
                <span class="plugin-version">v{{ plugin.version }}</span>
                <span :class="'plugin-status ' + plugin.status">{{ getStatusText(plugin.status) }}</span>
                <span :class="'plugin-enabled ' + (plugin.enabled ? 'yes' : 'no')">{{ plugin.enabled ? '已启用' : '已禁用' }}</span>
              </div>
            </div>
            <div class="plugin-actions">
              <button v-if="plugin.webui_url" class="btn-primary" @click.stop="openPluginMenu(plugin.webui_url)" :disabled="loading" style="margin-right: 5px;" title="插件菜单">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="3" y1="12" x2="21" y2="12"></line><line x1="3" y1="6" x2="21" y2="6"></line><line x1="3" y1="18" x2="21" y2="18"></line></svg>
                菜单
              </button>
              <button class="btn-primary" @click.stop="exportPlugin(plugin.id)" :disabled="loading" style="margin-right: 5px;" title="导出插件">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
                导出
              </button>
              <button v-if="!plugin.enabled" class="btn-success" @click.stop="startPlugin(plugin.id)" :disabled="loading">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg>
                启动
              </button>
              <button v-else class="btn-warning" @click.stop="stopPlugin(plugin.id)" :disabled="loading">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="6" y="4" width="4" height="16"></rect><rect x="14" y="4" width="4" height="16"></rect></svg>
                停止
              </button>
              <button class="btn-danger" @click.stop="uninstallPlugin(plugin.id)" :disabled="loading || plugin.status === 'running'" title="卸载插件">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                卸载
              </button>
            </div>
          </div>
          
          <div v-if="selectedPlugin === plugin.id" class="plugin-output" @click.stop>
            <div class="plugin-output-header">
              <span>插件输出 ({{ plugin.output ? plugin.output.length : 0 }} 行)</span>
              <button class="btn-clear" @click.stop="clearPluginOutput(plugin.id)">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                清空
              </button>
            </div>
            <div class="plugin-output-container" :id="'output-' + plugin.id">
              <div v-if="!plugin.output || plugin.output.length === 0" style="color: var(--text-secondary); padding: 10px;">
                暂无输出
              </div>
              <div v-for="(line, i) in plugin.output" :key="i" class="output-line">{{ line }}</div>
            </div>
          </div>
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
  `,
  mounted() {
    this.loadPlugins();
    this.connectStatusSSE();
  },
  beforeUnmount() {
    this.stopAllSSE();
    if (this.statusEventSource) {
      this.statusEventSource.close();
      this.statusEventSource = null;
    }
  },
  watch: {
    selectedPlugin(newVal, oldVal) {
      // 关闭旧的 SSE
      if (oldVal && this.eventSources[oldVal]) {
        this.eventSources[oldVal].close();
        delete this.eventSources[oldVal];
      }
      // 打开新的 SSE
      if (newVal) {
        this.connectPluginSSE(newVal);
      }
    }
  },
  methods: {
    getStatusText(status) {
      const map = { 'running': '运行中', 'stopped': '已停止', 'error': '出错' };
      return map[status] || status;
    },
    openPluginMenu(url) {
      if (!url) return;
      window.open(url, '_blank');
    },
    togglePlugin(id) {
      if (this.selectedPlugin === id) {
        this.selectedPlugin = null;
      } else {
        this.selectedPlugin = id;
      }
    },
    confirmAction() {
      if (this.confirmDialog.onConfirm) {
        this.confirmDialog.onConfirm();
      }
      this.confirmDialog.show = false;
    },
    importPlugin() {
      this.loading = true;
      fetch('/api/plugins/import', { method: 'POST' })
        .then(res => res.json())
        .then(data => {
          if (data.retcode === 0) {
            if (data.data === "Import cancelled") {
              window.showToast('导入已取消', 'info');
            } else {
              window.showToast('导入成功: ' + data.data, 'success');
              this.loadPlugins();
            }
          } else {
            window.showToast('导入失败: ' + data.data, 'error');
          }
        })
        .catch(err => {
          console.error('Failed to import plugin:', err);
          window.showToast('导入失败: ' + err, 'error');
        })
        .finally(() => { this.loading = false; });
    },
    exportPlugin(id) {
      this.loading = true;
      fetch('/api/plugins/export', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ plugin_id: id })
      })
        .then(res => res.json())
        .then(data => {
          if (data.retcode === 0) {
            if (data.data === 'Export cancelled') {
              window.showToast('导出已取消', 'info');
              return;
            }
            window.showToast('导出成功！', 'success');
          } else {
            window.showToast('导出失败: ' + data.data, 'error');
          }
        })
        .catch(err => {
          console.error('Failed to export plugin:', err);
          window.showToast('导出失败: ' + err, 'error');
        })
        .finally(() => { this.loading = false; });
    },
    uninstallPlugin(id) {
      const plugin = this.plugins.find(p => p.id === id);
      if (plugin && plugin.status === 'running') {
        window.showToast('无法卸载正在运行的插件，请先停止插件', 'error');
        return;
      }
      this.confirmDialog = {
        show: true,
        title: '卸载插件',
        message: '确定要卸载插件 ' + id + ' 吗？\n这将删除插件文件夹，但保留数据目录。',
        onConfirm: () => {
          this.loading = true;
          fetch('/api/plugins/' + encodeURIComponent(id) + '/uninstall', {
              method: 'POST'
          })
          .then(res => res.json())
          .then(data => {
            if (data.retcode === 0) {
              window.showToast('卸载成功', 'success');
              this.loadPlugins();
            } else {
              window.showToast('卸载失败: ' + data.data, 'error');
            }
          })
          .catch(err => {
            console.error('Failed to uninstall plugin:', err);
            window.showToast('卸载失败: ' + err, 'error');
          })
          .finally(() => { this.loading = false; });
        }
      };
    },
    loadPlugins() {
      this.loading = true;
      fetch('/api/plugins/list')
        .then(res => res.json())
        .then(data => {
          if (data.retcode === 0) {
            // 保留现有的 output 数据（使用 id 作为 key）
            const oldPlugins = {};
            this.plugins.forEach(p => { oldPlugins[p.id] = p.output; });
            
            this.plugins = data.data.map(p => {
              let output = oldPlugins[p.id] || p.output || [];
              // 限制输出行数
              if (output.length > MAX_OUTPUT_LINES) {
                output = output.slice(-MAX_OUTPUT_LINES);
              }
              
              // 应用挂起的状态更新
              if (this.pendingStatusUpdates[p.id]) {
                  const update = this.pendingStatusUpdates[p.id];
                  p.status = update.status;
                  p.enabled = update.enabled;
                  p.webui_url = update.webui_url;
              }
              
              return { ...p, output };
            });
            // 清除已应用的更新
            this.pendingStatusUpdates = {};
          }
        })
        .catch(err => console.error('Failed to load plugins:', err))
        .finally(() => { this.loading = false; });
    },
    startPlugin(id) {
      this.loading = true;
      fetch('/api/plugins/' + encodeURIComponent(id) + '/start', { method: 'POST' })
        .then(res => res.json())
        .then(data => {
          console.log('Start plugin response:', data);
          if (data.retcode === 0) {
            setTimeout(() => this.loadPlugins(), 500);
          } else {
            alert('启动失败: ' + data.data);
            this.loading = false;
          }
        })
        .catch(err => {
          console.error('Failed to start plugin:', err);
          alert('启动失败: ' + err);
          this.loading = false;
        });
    },
    stopPlugin(id) {
      this.loading = true;
      fetch('/api/plugins/' + encodeURIComponent(id) + '/stop', { method: 'POST' })
        .then(res => res.json())
        .then(data => {
          console.log('Stop plugin response:', data);
          if (data.retcode === 0) {
            setTimeout(() => this.loadPlugins(), 500);
          } else {
            alert('停止失败: ' + data.data);
            this.loading = false;
          }
        })
        .catch(err => {
          console.error('Failed to stop plugin:', err);
          alert('停止失败: ' + err);
          this.loading = false;
        });
    },
    clearPluginOutput(id) {
      fetch('/api/plugins/' + encodeURIComponent(id) + '/output/clear', { method: 'POST' })
        .then(res => res.json())
        .then(data => {
          if (data.retcode === 0) {
            const plugin = this.plugins.find(p => p.id === id);
            if (plugin) plugin.output = [];
          }
        })
        .catch(err => console.error('Failed to clear output:', err));
    },
    connectPluginSSE(pluginId) {
      // 关闭旧的连接
      if (this.eventSources[pluginId]) {
        this.eventSources[pluginId].close();
      }
      
      const eventSource = new EventSource('/api/plugins/' + encodeURIComponent(pluginId) + '/output/stream');
      this.eventSources[pluginId] = eventSource;
      
      // 清空现有输出，SSE 会发送完整历史
      const plugin = this.plugins.find(p => p.id === pluginId);
      if (plugin) {
        plugin.output = [];
      }
      
      eventSource.onmessage = (event) => {
        try {
          const line = JSON.parse(event.data);
          const plugin = this.plugins.find(p => p.id === pluginId);
          if (plugin) {
            if (!plugin.output) {
              plugin.output = [];
            }
            plugin.output.push(line);
            // 限制输出行数
            if (plugin.output.length > MAX_OUTPUT_LINES) {
              plugin.output.shift();
            }
            // 自动滚动到底部
            if (this.autoScroll) {
              this.$nextTick(() => {
                const container = document.getElementById('output-' + pluginId);
                if (container) {
                  container.scrollTop = container.scrollHeight;
                }
              });
            }
          }
        } catch (err) {
          console.error('Failed to parse plugin output:', err);
        }
      };
      
      eventSource.onerror = () => {
        console.log('Plugin ' + pluginId + ' SSE disconnected');
      };
    },
    stopAllSSE() {
      Object.values(this.eventSources).forEach(es => { if (es) es.close(); });
      this.eventSources = {};
    },
    connectStatusSSE() {
      if (this.statusEventSource) {
        this.statusEventSource.close();
      }
      
      this.statusEventSource = new EventSource('/api/plugins/status_stream');
      
      this.statusEventSource.onmessage = (event) => {
        try {
          const statusEvent = JSON.parse(event.data);
          // plugin_id 是后端发送的插件 ID
          const plugin = this.plugins.find(p => p.id === statusEvent.plugin_id);
          if (plugin) {
            plugin.status = statusEvent.status;
            plugin.enabled = statusEvent.enabled;
            plugin.webui_url = statusEvent.webui_url;
          } else {
            // 如果插件列表尚未加载或找不到插件，保存到挂起更新中
            this.pendingStatusUpdates[statusEvent.plugin_id] = {
                status: statusEvent.status,
                enabled: statusEvent.enabled,
                webui_url: statusEvent.webui_url
            };
          }
        } catch (err) {
          console.error('Failed to parse plugin status:', err);
        }
      };
      
      this.statusEventSource.onerror = () => {
        console.log('Plugin status SSE disconnected, reconnecting...');
        setTimeout(() => this.connectStatusSSE(), 3000);
      };
    }
  }
};
