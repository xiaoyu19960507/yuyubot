const AboutPage = {
  data() {
    return {
      version: '-',
      chromeVersion: '-'
    };
  },
  mounted() {
    this.loadAppInfo();
    this.extractChromeVersion();
  },
  methods: {
    loadAppInfo() {
      fetch('/api/app_info')
        .then(res => res.json())
        .then(data => {
          if (data.retcode === 0) {
            this.version = data.data.version;
          }
        })
        .catch(err => console.error('Failed to load app info:', err));
    },
    extractChromeVersion() {
      const userAgent = navigator.userAgent;
      const match = userAgent.match(/Chrome\/(\d+\.\d+\.\d+\.\d+)/);
      if (match && match[1]) {
        this.chromeVersion = match[1];
      }
    }
  },
  template: `
    <div>
      <div class="page-header">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>
        <h1>关于</h1>
      </div>
      <div class="card user-card">
        <div class="user-avatar"><img src="favicon.ico" alt="Avatar"></div>
        <div class="user-info"><h3>羽羽BOT</h3><p>OO机器人平台的插件管理工具，Milky应用端</p></div>
      </div>
      <div class="card">
        <div class="card-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>版本信息</div>
        <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 12px;">
          <div style="padding: 15px; background: var(--hover-bg); border-radius: 8px;">
            <div style="font-weight: 600; margin-bottom: 8px;">羽羽BOT</div>
            <div class="version-info"><span class="version-number">版本: {{ version }}</span><span class="version-status"><svg viewBox="0 0 24 24" fill="currentColor" style="width:14px;height:14px;"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"></path></svg></span></div>
          </div>
          <div style="padding: 15px; background: var(--hover-bg); border-radius: 8px;">
            <div style="font-weight: 600; margin-bottom: 8px;">WebView</div>
            <div class="version-info"><span class="version-number">版本: {{ chromeVersion }}</span><span class="version-status"><svg viewBox="0 0 24 24" fill="currentColor" style="width:14px;height:14px;"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"></path></svg></span></div>
          </div>
        </div>
      </div>
      <div class="card">
        <div class="card-title"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"></path><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"></path></svg>相关链接</div>
        <ul class="links-list">
          <li><a href="https://qm.qq.com/cgi-bin/qm/qr?k=s8AQdoqzhKk9NoZsGcRuuWt2DVh1mqwc&jump_from=webapi&authKey=Rax8cfJvNfGaHOgC8ocPdS3TrA0FW5wSEfWTvRPcGG8WYIG0UmMIqXE2wTPDR9QK" target="_blank"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"></path></svg>群 · 2155039992</a></li>
          <li><a href="https://github.com/xiaoyu19960507/yuyubot" target="_blank"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22"></path></svg>羽羽Bot GitHub</a></li>
          <li><a href="https://milky.ntqqrev.org" target="_blank"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"></path><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"></path></svg>Milky Link</a></li>
        </ul>
      </div>
    </div>
  `
};
