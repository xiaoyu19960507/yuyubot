const PermissionsPage = {
  data() {
    return {
      loading: false,
      loadingGroups: false,
      saving: false,
      autoSaveReady: false,
      saveStatus: 'idle',
      saveTimer: null,
      saveQueued: false,
      saveStatusTimer: null,
      lastSavedSnapshot: '',
      config: {
        mode: 'blacklist',
        blacklistGroups: [],
        whitelistGroups: []
      },
      groupOptions: [],
      groupOptionsConnected: false,
      groupOptionsMessage: '',
      groupFilter: ''
    };
  },
  computed: {
    pageBusy() {
      return this.loading || this.loadingGroups;
    },
    currentGroups() {
      return this.config.mode === 'blacklist'
        ? this.config.blacklistGroups
        : this.config.whitelistGroups;
    },
    filteredGroupOptions() {
      const keyword = (this.groupFilter || '').trim().toLowerCase();
      if (!keyword) {
        return this.groupOptions;
      }

      return this.groupOptions.filter(group => {
        return group.groupName.toLowerCase().includes(keyword)
          || String(group.groupId).includes(keyword);
      });
    },
    unknownSelectedGroups() {
      const known = new Set(this.groupOptions.map(group => group.groupId));
      return this.currentGroups.filter(groupId => !known.has(groupId));
    }
  },
  mounted() {
    this.loadPageData();
  },
  beforeUnmount() {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
    }
    if (this.saveStatusTimer) {
      clearTimeout(this.saveStatusTimer);
    }
  },
  methods: {
    showToast(message, type = 'info') {
      if (window.showToast) {
        window.showToast(message, type);
      }
    },
    async loadPageData() {
      await Promise.all([this.loadConfig(), this.loadGroupOptions()]);
    },
    normalizeGroups(groups) {
      return [...new Set((groups || []).map(Number).filter(Number.isSafeInteger))]
        .filter(groupId => groupId > 0)
        .sort((a, b) => a - b);
    },
    buildPayload() {
      return {
        mode: this.config.mode,
        blacklistGroups: this.normalizeGroups(this.config.blacklistGroups),
        whitelistGroups: this.normalizeGroups(this.config.whitelistGroups)
      };
    },
    serializePayload(payload) {
      return JSON.stringify(payload);
    },
    setSaveStatus(status) {
      this.saveStatus = status;

      if (this.saveStatusTimer) {
        clearTimeout(this.saveStatusTimer);
        this.saveStatusTimer = null;
      }

      if (status === 'saved') {
        this.saveStatusTimer = setTimeout(() => {
          this.saveStatus = 'idle';
          this.saveStatusTimer = null;
        }, 1600);
      }
    },
    scheduleAutoSave() {
      if (!this.autoSaveReady) {
        return;
      }

      if (this.saveTimer) {
        clearTimeout(this.saveTimer);
      }

      this.setSaveStatus('pending');
      this.saveTimer = setTimeout(() => {
        this.saveTimer = null;
        this.flushAutoSave();
      }, 350);
    },
    async flushAutoSave() {
      const payload = this.buildPayload();
      const snapshot = this.serializePayload(payload);

      if (snapshot === this.lastSavedSnapshot) {
        this.saveQueued = false;
        this.setSaveStatus('idle');
        return;
      }

      if (this.saving) {
        this.saveQueued = true;
        return;
      }

      this.saving = true;
      this.saveQueued = false;
      this.setSaveStatus('saving');

      try {
        const response = await fetch('/api/permissions/save_config', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json'
          },
          body: JSON.stringify(payload)
        });
        const result = await response.json();

        if (result.retcode === 0) {
          this.config.blacklistGroups = payload.blacklistGroups;
          this.config.whitelistGroups = payload.whitelistGroups;
          this.lastSavedSnapshot = snapshot;
          this.setSaveStatus('saved');
        } else {
          this.setSaveStatus('error');
          this.showToast('\u4FDD\u5B58\u6743\u9650\u914D\u7F6E\u5931\u8D25: ' + result.data, 'error');
        }
      } catch (error) {
        console.error('Failed to save permission config:', error);
        this.setSaveStatus('error');
        this.showToast('\u4FDD\u5B58\u6743\u9650\u914D\u7F6E\u5931\u8D25: ' + error.message, 'error');
      } finally {
        this.saving = false;

        if (this.saveQueued) {
          this.saveQueued = false;
          this.flushAutoSave();
        }
      }
    },
    async loadConfig() {
      this.loading = true;

      try {
        const response = await fetch('/api/permissions/get_config');
        const result = await response.json();

        if (result.retcode === 0) {
          this.config = {
            mode: result.data.mode || 'blacklist',
            blacklistGroups: this.normalizeGroups(result.data.blacklistGroups || []),
            whitelistGroups: this.normalizeGroups(result.data.whitelistGroups || [])
          };
          this.lastSavedSnapshot = this.serializePayload(this.buildPayload());
          this.autoSaveReady = true;
        } else {
          this.showToast('\u52A0\u8F7D\u6743\u9650\u914D\u7F6E\u5931\u8D25: ' + result.data, 'error');
        }
      } catch (error) {
        console.error('Failed to load permission config:', error);
        this.showToast('\u52A0\u8F7D\u6743\u9650\u914D\u7F6E\u5931\u8D25: ' + error.message, 'error');
      } finally {
        if (!this.autoSaveReady) {
          this.lastSavedSnapshot = this.serializePayload(this.buildPayload());
          this.autoSaveReady = true;
        }
        this.loading = false;
      }
    },
    async loadGroupOptions() {
      this.loadingGroups = true;

      try {
        const response = await fetch('/api/permissions/group_options');
        const result = await response.json();
        const data = result.data || {};

        this.groupOptions = (data.groups || []).map(group => ({
          groupId: Number(group.groupId),
          groupName: group.groupName || ('\u7FA4 ' + group.groupId),
          memberCount: Number(group.memberCount || 0),
          maxMemberCount: Number(group.maxMemberCount || 0)
        }));
        this.groupOptionsConnected = !!data.connected;
        this.groupOptionsMessage = data.message || '';

        if (result.retcode !== 0) {
          this.showToast('\u52A0\u8F7D\u7FA4\u5217\u8868\u5931\u8D25: ' + (data.message || result.data), 'error');
        }
      } catch (error) {
        console.error('Failed to load group options:', error);
        this.groupOptions = [];
        this.groupOptionsConnected = false;
        this.groupOptionsMessage = error.message;
        this.showToast('\u52A0\u8F7D\u7FA4\u5217\u8868\u5931\u8D25: ' + error.message, 'error');
      } finally {
        this.loadingGroups = false;
      }
    },
    selectMode(mode) {
      this.config.mode = mode;
      this.groupFilter = '';
      this.scheduleAutoSave();
    },
    modeButtonClass(mode) {
      return this.config.mode === mode ? 'btn-primary' : 'btn-clear';
    },
    currentModeTitle() {
      return this.config.mode === 'blacklist'
        ? '\u9ED1\u540D\u5355\u7FA4'
        : '\u767D\u540D\u5355\u7FA4';
    },
    currentModeDescription() {
      return this.config.mode === 'blacklist'
        ? '\u70B9\u51FB\u4E0B\u65B9\u7684\u7FA4\u5361\u7247\u52A0\u5165\u9ED1\u540D\u5355\u3002\u88AB\u9009\u4E2D\u7684\u7FA4\u5C06\u88AB\u62D2\u7EDD\u4E8B\u4EF6\u4E0E\u526F\u4F5C\u7528 API\u3002'
        : '\u70B9\u51FB\u4E0B\u65B9\u7684\u7FA4\u5361\u7247\u52A0\u5165\u767D\u540D\u5355\u3002\u53EA\u6709\u88AB\u9009\u4E2D\u7684\u7FA4\u4F1A\u5141\u8BB8\u4E8B\u4EF6\u4E0E\u526F\u4F5C\u7528 API\u3002';
    },
    isCurrentGroupSelected(groupId) {
      return this.currentGroups.includes(groupId);
    },
    toggleCurrentGroup(groupId) {
      const key = this.config.mode === 'blacklist' ? 'blacklistGroups' : 'whitelistGroups';
      const nextGroups = new Set(this.config[key]);

      if (nextGroups.has(groupId)) {
        nextGroups.delete(groupId);
      } else {
        nextGroups.add(groupId);
      }

      this.config[key] = Array.from(nextGroups).sort((a, b) => a - b);
      this.scheduleAutoSave();
    },
    removeCurrentGroup(groupId) {
      if (this.isCurrentGroupSelected(groupId)) {
        this.toggleCurrentGroup(groupId);
      }
    },
    getGroupLabel(groupId) {
      const group = this.groupOptions.find(item => item.groupId === groupId);
      return group ? `${group.groupName} (${groupId})` : `QQ\u7FA4 ${groupId}`;
    },
    saveStatusText() {
      const map = {
        pending: '\u5C06\u81EA\u52A8\u4FDD\u5B58...',
        saving: '\u6B63\u5728\u81EA\u52A8\u4FDD\u5B58...',
        saved: '\u5DF2\u81EA\u52A8\u4FDD\u5B58',
        error: '\u81EA\u52A8\u4FDD\u5B58\u5931\u8D25'
      };
      return map[this.saveStatus] || '';
    }
  },
  template: `
    <div>
      <div class="page-header">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"></path>
        </svg>
        <h1>\u6743\u9650\u914D\u7F6E</h1>
      </div>

      <div class="card">
        <div class="card-title">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 1v22"></path>
            <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7H14.5a3.5 3.5 0 0 1 0 7H6"></path>
          </svg>
          \u6A21\u5F0F\u9009\u62E9
        </div>

        <div class="info-panel">
          <div class="text-muted">
            \u53EA\u5F71\u54CD\u63D2\u4EF6\u7ECF\u7531 Milky \u4EE3\u7406\u63A5\u6536\u7684\u7FA4\u4E8B\u4EF6\u4E0E\u8C03\u7528\u7684\u7FA4\u804A\u6709\u526F\u4F5C\u7528 API\u3002\u53EF\u4EE5\u76F4\u63A5\u4ECE\u5F53\u524D Bot \u7684\u7FA4\u5217\u8868\u4E2D\u9009\u62E9\uFF0C\u4E0D\u9700\u8981\u624B\u52A8\u8BB0\u7FA4\u53F7\u3002
          </div>
        </div>

        <div class="mode-grid" style="margin-top: 16px;">
          <button :class="modeButtonClass('blacklist')" @click="selectMode('blacklist')" :disabled="pageBusy">
            \u9ED1\u540D\u5355\u6A21\u5F0F
          </button>
          <button :class="modeButtonClass('whitelist')" @click="selectMode('whitelist')" :disabled="pageBusy">
            \u767D\u540D\u5355\u6A21\u5F0F
          </button>
        </div>

        <div class="info-panel" style="margin-top: 16px;">
          <div style="font-size: 15px; font-weight: 600; margin-bottom: 8px;">
            \u5F53\u524D\u751F\u6548\uFF1A{{ config.mode === 'blacklist' ? '\\u9ED1\\u540D\\u5355\\u6A21\\u5F0F' : '\\u767D\\u540D\\u5355\\u6A21\\u5F0F' }}
          </div>
          <div class="text-muted" v-if="config.mode === 'blacklist'">
            \u63D2\u4EF6\u4E0D\u80FD\u63A5\u6536\u6216\u64CD\u4F5C\u9ED1\u540D\u5355\u4E2D\u7684\u7FA4\uFF0C\u672A\u5217\u51FA\u7684\u7FA4\u9ED8\u8BA4\u5141\u8BB8\u3002
          </div>
          <div class="text-muted" v-else>
            \u63D2\u4EF6\u53EA\u80FD\u63A5\u6536\u6216\u64CD\u4F5C\u767D\u540D\u5355\u4E2D\u7684\u7FA4\uFF0C\u672A\u5217\u51FA\u7684\u7FA4\u9ED8\u8BA4\u62D2\u7EDD\u3002
          </div>
          <div class="text-muted" style="margin-top: 8px;">
            \u5F53\u524D\u751F\u6548\u7FA4\u6570\uFF1A{{ currentGroups.length }}
          </div>
          <div v-if="config.mode === 'whitelist' && currentGroups.length === 0" class="text-muted" style="margin-top: 8px; color: #ff9800;">
            \u767D\u540D\u5355\u4E3A\u7A7A\u65F6\uFF0C\u6240\u6709\u7FA4\u4E8B\u4EF6\u548C\u7FA4\u804A\u526F\u4F5C\u7528 API \u90FD\u4F1A\u88AB\u62D2\u7EDD\u3002
          </div>
        </div>
      </div>

      <div class="card" style="border: 1px solid var(--accent-color);">
        <div class="card-title">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M3 6h18"></path>
            <path d="M8 6V4h8v2"></path>
            <path d="M6 6l1 14h10l1-14"></path>
          </svg>
          {{ currentModeTitle() }}
        </div>

        <div class="text-muted">
          {{ currentModeDescription() }}
        </div>

        <div class="group-selector-toolbar">
          <div class="form-group" style="flex: 1; min-width: 220px; margin-bottom: 0;">
            <label>\u641C\u7D22\u7FA4</label>
            <input
              type="text"
              v-model="groupFilter"
              :disabled="pageBusy"
              placeholder="\u6309\u7FA4\u540D\u6216\u7FA4\u53F7\u641C\u7D22"
            >
          </div>
          <button class="btn-clear" @click="loadGroupOptions" :disabled="pageBusy" style="align-self: flex-end;">
            {{ loadingGroups ? '\\u5237\\u65B0\\u4E2D...' : '\\u5237\\u65B0\\u7FA4\\u5217\\u8868' }}
          </button>
        </div>

        <div class="info-panel" style="margin-top: 16px;">
          <div class="text-muted" v-if="groupOptionsConnected">
            \u5DF2\u8FDE\u63A5 Bot\uFF0C\u5F53\u524D\u53EF\u9009\u7FA4\u6570\uFF1A{{ groupOptions.length }}
          </div>
          <div class="text-muted" v-else>
            {{ groupOptionsMessage || '\\u8BF7\\u5148\\u8FDE\\u63A5 Bot \\u4EE5\\u52A0\\u8F7D\\u7FA4\\u5217\\u8868' }}
          </div>
        </div>

        <div style="margin-top: 16px;">
          <div class="text-muted" style="margin-bottom: 10px;">\u5DF2\u9009\u62E9\u7684\u7FA4</div>
          <div v-if="currentGroups.length > 0" class="tag-list">
            <button
              v-for="groupId in currentGroups"
              :key="'selected-' + config.mode + '-' + groupId"
              class="tag-chip tag-chip-button"
              type="button"
              @click="removeCurrentGroup(groupId)"
              :disabled="pageBusy"
            >
              {{ getGroupLabel(groupId) }}
            </button>
          </div>
          <div v-else class="text-muted">\u6682\u672A\u9009\u62E9\u4EFB\u4F55\u7FA4\u3002</div>
        </div>

        <div v-if="unknownSelectedGroups.length > 0" class="info-panel" style="margin-top: 16px;">
          <div class="text-muted">
            \u4EE5\u4E0B\u5DF2\u9009\u62E9\u7FA4\u6682\u672A\u51FA\u73B0\u5728\u5F53\u524D\u7FA4\u5217\u8868\u4E2D\uFF1A{{ unknownSelectedGroups.join(', ') }}
          </div>
        </div>

        <div class="group-options-grid" v-if="filteredGroupOptions.length > 0">
          <button
            v-for="group in filteredGroupOptions"
            :key="group.groupId"
            type="button"
            class="group-option-card"
            :class="{ active: isCurrentGroupSelected(group.groupId) }"
            @click="toggleCurrentGroup(group.groupId)"
            :disabled="pageBusy"
          >
            <span class="group-option-name">{{ group.groupName }}</span>
            <span class="group-option-meta">QQ\u7FA4 {{ group.groupId }}</span>
            <span class="group-option-meta" v-if="group.maxMemberCount > 0">
              \u6210\u5458 {{ group.memberCount }} / {{ group.maxMemberCount }}
            </span>
          </button>
        </div>
        <div v-else class="text-muted" style="margin-top: 16px;">
          {{ groupOptionsConnected ? '\\u6CA1\\u6709\\u627E\\u5230\\u5339\\u914D\\u7684\\u7FA4' : '\\u5F53\\u524D\\u6CA1\\u6709\\u53EF\\u9009\\u7FA4\\u5217\\u8868' }}
        </div>
      </div>

      <div class="card">
        <div style="display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap;">
          <div class="text-muted">\u66F4\u6539\u540E\u4F1A\u81EA\u52A8\u4FDD\u5B58\u5E76\u7ACB\u5373\u751F\u6548\uFF0C\u4E0D\u9700\u8981\u91CD\u542F\u7A0B\u5E8F\u6216\u63D2\u4EF6\u3002</div>
          <div v-if="saveStatus !== 'idle'" class="text-muted" :style="saveStatus === 'error' ? 'color: #f44336;' : 'color: var(--accent-color);'">
            {{ saveStatusText() }}
          </div>
        </div>
      </div>
    </div>
  `
};
