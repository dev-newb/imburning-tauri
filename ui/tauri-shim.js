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
    // force / fitPreset / userAction are the guard that stops a background
    // refit collapsing a hand-sized window — dropping them is not harmless.
    resizeWindow: (height, force, fitPreset, userAction) =>
      invoke('resize_window', {
        height: Number(height) || 0,
        force: force === true,
        fitPreset: fitPreset === true,
        userAction: userAction === true
      }),
    fitLandscapeWidth: (width) => invoke('fit_landscape_width', { width: Number(width) || 0 }),
    setMinHeight: (h) => invoke('set_min_height', { height: Number(h) || 180 }),
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
    exportHistory: (format) => invoke('export_history', { format: String(format || 'csv') }),

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
    // Both return the Electron reply SHAPE, not the raw value: the caller
    // checks `res.ok` and reads `res.path` / `res.dataUrl`, so a bare string
    // or null silently does nothing.
    pickSoundFile: async () => {
      const picked = await window.__TAURI__.dialog.open({
        multiple: false,
        filters: [{ name: 'Audio', extensions: ['m4a', 'mp3', 'wav', 'aac', 'ogg', 'flac'] }]
      });
      const path = Array.isArray(picked) ? picked[0] : picked;
      if (!path) return { ok: false, canceled: true };
      return { ok: true, path, name: String(path).split('/').pop() };
    },
    readSoundFile: (path) => invoke('read_sound_file', { path: String(path || '') }),

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
    sendAlertWebhook: (event, title, message) =>
      invoke('send_alert_webhook', {
        event: String(event || ''), title: String(title || ''), message: String(message || '')
      }),

    // ---- layout ----
    setCompactMode: (compact) => invoke('set_compact_mode', { compact: compact === true }),
    settingsFit: (height) => invoke('resize_window', { height: Number(height) || 0 }),
    settingsRestore: noop,
    applyWindowPreset: (preset) => invoke('apply_window_preset', { preset: String(preset || '') }),

    // ---- detachable graph window ----
    openGraphWindow: () => invoke('open_graph_window'),
    closeGraphWindow: () => invoke('close_graph_window'),
    isGraphWindowOpen: () => invoke('is_graph_window_open'),
    graphSetAlwaysOnTop: (flag) => invoke('graph_set_always_on_top', { flag: flag === true }),
    graphGetAlwaysOnTop: () => invoke('graph_get_always_on_top'),

    // ---- OAuth connect flows (not yet ported) ----
    oauthConnect: later({ success: false, reason: 'unsupported' }),
    oauthDisconnect: later({ success: false })
  };

  // Everything below is main-window behaviour. graph.html loads this shim too
  // (it needs electronAPI for getLatestUsage), but it has no widgetContainer,
  // no auto-height loop, and reporting its DOM would just confuse the build
  // verification.
  if (!document.getElementById('widgetContainer')) return;

  // Post-load settle passes. The renderer only re-measures its height when
  // something changes the content, and Electron's main process happened to
  // provide the extra nudges (did-finish-load, focus, the resize notifier).
  // Without them one early measurement — taken before fonts, the chart canvas
  // and the async usage fetch have settled — is the height the window keeps.
  // Re-run the renderer's OWN fit a few times as things land, rather than
  // second-guessing its arithmetic here.
  const settle = () => {
    try {
      if (typeof window.resizeWidget === 'function') window.resizeWidget(false);
    } catch (err) {
      console.warn('settle fit failed:', err);
    }
  };
  for (const delay of [1500, 4000, 9000, 15000]) setTimeout(settle, delay);
  window.addEventListener('focus', () => setTimeout(settle, 120));

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
        errors: window.__shimErrors || [],
        layout: (() => {
          const mc = document.getElementById('mainContent');
          if (!mc) return 'no mainContent';
          const top = mc.getBoundingClientRect().top;
          return {
            innerHeight: window.innerHeight,
            mainContentTop: Math.round(top),
            children: [...mc.children].map((c) => ({
              id: c.id || c.className,
              display: getComputedStyle(c).display,
              bottom: Math.round(c.getBoundingClientRect().bottom - top)
            }))
          };
        })()
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
