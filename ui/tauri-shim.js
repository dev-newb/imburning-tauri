// window.electronAPI, backed by Tauri.
//
// The renderer is the Electron build's, carried over unmodified — this file is
// the entire compatibility layer. Every method the preload exposed appears
// here with the same name, arity and return shape, so app.js cannot tell which
// runtime it is on. Methods whose backend command does not exist yet resolve
// to a benign value rather than throwing: a missing feature should leave the
// widget running, not blank it.

(function () {
  const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args || {});
  const listen = (event, cb) => window.__TAURI__.event.listen(event, cb);
  const noop = () => {};
  const later = (value) => () => Promise.resolve(value);

  // Same allowlist the Electron preload enforced — the renderer must not be
  // able to open arbitrary URLs, whichever runtime is underneath.
  const ALLOWED_EXTERNAL_DOMAINS = ['claude.ai', 'github.com', 'paypal.me', 'buymeacoffee.com'];
  function isAllowedExternalUrl(url) {
    try {
      const parsed = new URL(url);
      if (parsed.protocol !== 'https:') return false;
      return ALLOWED_EXTERNAL_DOMAINS.some(
        (d) => parsed.hostname === d || parsed.hostname.endsWith('.' + d)
      );
    } catch {
      return false;
    }
  }

  function sanitizeFetchOptions(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
    const out = {};
    if (value.forceExtended === true) out.forceExtended = true;
    if (value.forceProviders === true) out.forceProviders = true;
    if (value.refreshLocalCredentials === true) out.refreshLocalCredentials = true;
    return out;
  }

  window.electronAPI = {
    // ---- credentials (state only, never a token) ----
    getCredentials: () => invoke('get_credentials'),
    anthropicLogin: later({ success: false, reason: 'unsupported' }),
    setOrganization: (orgId) =>
      invoke('save_settings', { settings: { ...(window._cachedSettings || {}), organizationId: String(orgId || '') } }),
    deleteCredentials: later({ success: false }),

    // ---- window controls ----
    minimizeWindow: () => invoke('minimize_window'),
    closeWindow: () => invoke('close_window'),
    resizeWindow: (height) => invoke('resize_window', { height: Number(height) || 0 }),
    fitLandscapeWidth: (width) => invoke('fit_landscape_width', { width: Number(width) || 0 }),
    setMinHeight: noop,
    getWindowPosition: () => invoke('get_window_position'),
    setWindowPosition: (position) => invoke('set_window_position', { position }),

    // ---- events ----
    onRefreshUsage: (cb) => listen('refresh-usage', () => cb()),
    onSessionExpired: (cb) => listen('session-expired', () => cb()),
    onAnthropicDegraded: (cb) => listen('anthropic-fetch-degraded', (e) => cb(e.payload === true)),
    onWindowUserSized: (cb) => listen('window-user-sized', (e) => cb(e.payload)),
    onUsageUpdated: (cb) => listen('usage-updated', () => cb()),
    onGraphSettingsUpdated: (cb) => listen('graph-settings-updated', () => cb()),
    onGraphWindowClosed: (cb) => listen('graph-window-closed', () => cb()),
    onUpdateDownloaded: noop,

    // ---- data ----
    fetchUsageData: (options = {}) => {
      const opts = sanitizeFetchOptions(options);
      return invoke('fetch_usage_data', { force: opts.forceProviders === true });
    },
    getLatestUsage: () => invoke('get_latest_usage'),
    getUsageHistory: () => invoke('get_usage_history'),
    exportHistory: later(null),

    openExternal: (url) => {
      if (!isAllowedExternalUrl(url)) {
        console.warn('openExternal blocked — URL not in allowlist:', url);
        return;
      }
      window.__TAURI__.opener.openUrl(url);
    },

    // ---- platform ----
    platform: (navigator.userAgent.includes('Mac') && 'darwin') ||
      (navigator.userAgent.includes('Windows') && 'win32') || 'linux',
    isPortable: false,

    // ---- settings ----
    getSettings: () => invoke('get_settings'),
    saveSettings: (settings) => invoke('save_settings', { settings }),

    // ---- alert sounds ----
    pickSoundFile: async () => {
      const path = await window.__TAURI__.dialog.open({
        multiple: false,
        filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'm4a', 'aiff'] }]
      });
      return path || null;
    },
    readSoundFile: later(null),

    // ---- updates ----
    checkForUpdate: later({ available: false }),
    getAppVersion: () => invoke('get_app_version'),
    installUpdate: noop,
    runMacUpdate: noop,

    // ---- notifications ----
    showNotification: async (title, body) => {
      const api = window.__TAURI__.notification;
      let granted = await api.isPermissionGranted();
      if (!granted) granted = (await api.requestPermission()) === 'granted';
      if (granted) api.sendNotification({ title, body });
    },
    sendAlertWebhook: noop,

    // ---- layout ----
    setCompactMode: noop,
    settingsFit: (height) => invoke('resize_window', { height: Number(height) || 0 }),
    settingsRestore: noop,
    applyWindowPreset: noop,

    // ---- detachable graph window (not yet ported) ----
    openGraphWindow: noop,
    closeGraphWindow: noop,
    isGraphWindowOpen: later(false),
    graphSetAlwaysOnTop: (flag) => invoke('set_always_on_top', { flag: flag === true }),
    graphGetAlwaysOnTop: later(true),

    // ---- OAuth connect flows (not yet ported) ----
    oauthConnect: later({ success: false, reason: 'unsupported' }),
    oauthDisconnect: later({ success: false })
  };

  // Build verification: WKWebView exposes no remote-debugging port, so the
  // page reports what it rendered back to the backend, which only records it
  // when IMBURNING_DEV_REPORT=1. Silent in a normal run.
  setTimeout(() => {
    const rows = (sel) =>
      [...document.querySelectorAll(sel)].map((r) => r.textContent.replace(/\s+/g, ' ').trim()).filter(Boolean);
    invoke('dev_report', {
      text: JSON.stringify({
        title: document.title,
        google: rows('#googleRows > *'),
        anthropic: rows('#scopedRows > *'),
        openai: rows('#openaiRows > *'),
        chipGoogle: (document.getElementById('chipGoogle') || {}).textContent || null,
        errors: window.__shimErrors || []
      })
    }).catch(() => {});
  }, 12000);

  window.addEventListener('error', (e) => {
    (window.__shimErrors = window.__shimErrors || []).push(String(e.message).slice(0, 200));
  });

  // Window dragging. Electron got this from `-webkit-app-region: drag` in the
  // stylesheet, which WebKit here ignores; rather than fork the shared CSS,
  // reproduce the same regions in JS. DRAG_SELECTORS mirrors the rules that
  // set `drag` and NO_DRAG_SELECTORS the ones that set `no-drag`, so the two
  // builds stay in step — if a rule moves in styles.css, update it here too.
  const DRAG_SELECTORS = '.title-bar, .settings-header';
  const NO_DRAG_SELECTORS =
    'button, input, select, textarea, a, .title-caption, .done-settings-btn, ' +
    '.update-banner, .stale-banner, .bottom-bar, .row-hide-btn, [role="button"]';

  document.addEventListener('mousedown', async (e) => {
    if (e.button !== 0) return;
    const target = e.target instanceof Element ? e.target : null;
    if (!target) return;
    if (!target.closest(DRAG_SELECTORS)) return;
    if (target.closest(NO_DRAG_SELECTORS)) return;
    // A double-click on the caption area is a maximise gesture, not a drag.
    if (e.detail > 1) return;
    try {
      await window.__TAURI__.window.getCurrentWindow().startDragging();
    } catch (err) {
      console.warn('startDragging failed:', err);
    }
  });
})();
