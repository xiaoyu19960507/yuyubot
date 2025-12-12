const BotPage = {
  props: ['botConfig'],
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
          <div class="form-group"><label>API 地址</label><input type="text" v-model="botConfig.api" placeholder="http://localhost:3010/api"></div>
          
          <div class="form-group"><label>Event - WebSocket</label><input type="text" v-model="botConfig.eventWs" placeholder="ws://localhost:3011/event"></div>

          <div class="form-group"><label>Token (可选)</label><input type="password" v-model="botConfig.token" placeholder="共用于 API 和 Event"></div>
          
          <div class="btn-center"><button class="btn-primary">保存配置</button></div>
        </div>
      </div>
    </div>
  `
};
