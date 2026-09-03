// Application state
let credentials = null;
let updateInterval = null;
let countdownInterval = null;
let latestUsageData = null;
let isExpanded = false;
let isCompactMode = false;
let _settingsOpenedFromCompact = false;
let _settingsReturnToWide = false;
let _settingsReturnBounds = null;
let usageChart = null;
let graphVisible = false;
let graphWasVisible = false; // preserves graph state across compact mode toggle
let appInitializing = true;  // suppresses _saveViewState during startup restore
let isFetching = false;       // in-flight guard — prevents overlapping fetchUsageData calls
let _updateReadyToInstall = false; // an auto-update is downloaded and ready to apply
let _canSelfUpdate = false;        // macOS source install that can rebuild itself
let isOpenaiExtrasOpen = true;     // OpenAI Credits + Limit Resets sub-panel
let projectionsVisible = true;     // graph forecast lines (the psychic's trance)
const UPDATE_INTERVAL = 5 * 60 * 1000; // 5 minutes
const WIDGET_HEIGHT_COLLAPSED = 155;
const WIDGET_ROW_HEIGHT = 30;
const GRAPH_HEIGHT = 232;
const chartUtils = window.BurnwatchChartUtils;

// Debug logging — only shows in DevTools (development mode).
// Regular users won't see verbose logs in production.
const DEBUG = (new URLSearchParams(window.location.search)).has('debug');
function debugLog(...args) {
  if (DEBUG) console.log('[Debug]', ...args);
}

function hasUsableCredentials(value = credentials) {
    return !!(
        value
        && ((value.loggedIn && value.organizationId)
            || value.cliFallbackAvailable
            || value.providerFallbackAvailable)
    );
}

function hasClaudeWebCredentials(value = credentials) {
    return !!(value && value.loggedIn && value.organizationId);
}

// DOM elements
const elements = {
    loadingContainer: document.getElementById('loadingContainer'),
    noUsageContainer: document.getElementById('noUsageContainer'),
    mainContent: document.getElementById('mainContent'),
    refreshBtn: document.getElementById('refreshBtn'),
    graphBtn: document.getElementById('graphBtn'),
    graphPopoutBtn: document.getElementById('graphPopoutBtn'),
    wideBtn: document.getElementById('wideBtn'),
    tallBtn: document.getElementById('tallBtn'),
    minimizeBtn: document.getElementById('minimizeBtn'),
    closeBtn: document.getElementById('closeBtn'),

    sessionPercentage: document.getElementById('sessionPercentage'),
    sessionProgress: document.getElementById('sessionProgress'),
    sessionTimer: document.getElementById('sessionTimer'),
    sessionTimeText: document.getElementById('sessionTimeText'),

    weeklyPercentage: document.getElementById('weeklyPercentage'),
    weeklyProgress: document.getElementById('weeklyProgress'),
    weeklyTimer: document.getElementById('weeklyTimer'),
    weeklyTimeText: document.getElementById('weeklyTimeText'),
    weeklyResetsAt: document.getElementById('weeklyResetsAt'),

    sessionResetsAt: document.getElementById('sessionResetsAt'),

    expandToggle: document.getElementById('expandToggle'),
    expandArrow: document.getElementById('expandArrow'),
    expandSection: document.getElementById('expandSection'),
    extraRows: document.getElementById('extraRows'),
    scopedRows: document.getElementById('scopedRows'),
    anthropicCliRows: document.getElementById('anthropicCliRows'),
    openaiCliRows: document.getElementById('openaiCliRows'),
    googleCliRows: document.getElementById('googleCliRows'),
    titleBar: document.getElementById('titleBar'),
    headerAnthropic: document.getElementById('headerAnthropic'),
    bodyAnthropic: document.getElementById('bodyAnthropic'),
    eyeAnthropic: document.getElementById('eyeAnthropic'),
    sectionOpenai: document.getElementById('sectionOpenai'),
    headerOpenai: document.getElementById('headerOpenai'),
    bodyOpenai: document.getElementById('bodyOpenai'),
    openaiRows: document.getElementById('openaiRows'),
    eyeOpenai: document.getElementById('eyeOpenai'),
    sectionGoogle: document.getElementById('sectionGoogle'),
    headerGoogle: document.getElementById('headerGoogle'),
    bodyGoogle: document.getElementById('bodyGoogle'),
    googleRows: document.getElementById('googleRows'),
    eyeGoogle: document.getElementById('eyeGoogle'),
    graphSection: document.getElementById('graphSection'),
    usageChart: document.getElementById('usageChart'),

    settingsBtn: document.getElementById('settingsBtn'),
    settingsOverlay: document.getElementById('settingsOverlay'),
    closeSettingsBtn: document.getElementById('closeSettingsBtn'),
    logoutBtn: document.getElementById('logoutBtn'),
    anthropicLoginStatus: document.getElementById('anthropicLoginStatus'),
    refreshLocalLoginsBtn: document.getElementById('refreshLocalLoginsBtn'),
    githubBtn: document.getElementById('githubBtn'),
    psychicBtn: document.getElementById('psychicBtn'),
    chipAnthropic: document.getElementById('chipAnthropic'),
    chipOpenai: document.getElementById('chipOpenai'),
    chipGoogle: document.getElementById('chipGoogle'),
    connectRowOpenai: document.getElementById('connectRowOpenai'),
    connectOpenaiBtn: document.getElementById('connectOpenaiBtn'),
    connectErrorOpenai: document.getElementById('connectErrorOpenai'),
    connectRowGoogle: document.getElementById('connectRowGoogle'),
    connectGoogleBtn: document.getElementById('connectGoogleBtn'),
    connectErrorGoogle: document.getElementById('connectErrorGoogle'),
    settingsConnectOpenaiBtn: document.getElementById('settingsConnectOpenaiBtn'),
    settingsConnectGoogleBtn: document.getElementById('settingsConnectGoogleBtn'),
    disconnectOpenaiBtn: document.getElementById('disconnectOpenaiBtn'),
    disconnectGoogleBtn: document.getElementById('disconnectGoogleBtn'),
    openaiLoginStatus: document.getElementById('openaiLoginStatus'),
    googleLoginStatus: document.getElementById('googleLoginStatus'),
    psychicImg: document.getElementById('psychicImg'),
    openaiExtras: document.getElementById('openaiExtras'),
    openaiExtraRows: document.getElementById('openaiExtraRows'),
    openaiExpandToggle: document.getElementById('openaiExpandToggle'),
    openaiExpandArrow: document.getElementById('openaiExpandArrow'),
    autoStartCol: document.getElementById('autoStartCol'),
    autoStartToggle: document.getElementById('autoStartToggle'),
    autoStartHint: document.getElementById('autoStartHint'),
    minimizeToTrayToggle: document.getElementById('minimizeToTrayToggle'),
    alwaysOnTopToggle: document.getElementById('alwaysOnTopToggle'),
    warnThreshold: document.getElementById('warnThreshold'),
    dangerThreshold: document.getElementById('dangerThreshold'),
    themeCycleBtn: document.getElementById('themeCycleBtn'),
    timeFormat: document.getElementById('timeFormat'),
    weeklyDateFormat: document.getElementById('weeklyDateFormat'),
    refreshInterval: document.getElementById('refreshInterval'),
    orgSelector: document.getElementById('orgSelector'),
    orgSelectorCol: document.getElementById('orgSelectorCol'),

    updateBanner: document.getElementById('updateBanner'),
    updateBannerText: document.getElementById('updateBannerText'),
    updateBannerDismiss: document.getElementById('updateBannerDismiss'),
    settingsVersionLabel: document.getElementById('settingsVersionLabel'),
    settingsUpdateLink: document.getElementById('settingsUpdateLink'),
    usageAlertsToggle: document.getElementById('usageAlertsToggle'),
    compactModeToggle: document.getElementById('compactModeToggle'),
    compactModeToggleCompact: document.getElementById('compactModeToggleCompact'),
    compactContent: document.getElementById('compactContent'),
    compactSessionFill: document.getElementById('compactSessionFill'),
    compactSessionPct: document.getElementById('compactSessionPct'),
    compactWeeklyFill: document.getElementById('compactWeeklyFill'),
    compactWeeklyPct: document.getElementById('compactWeeklyPct'),
    compactScopedRow: document.getElementById('compactScopedRow'),
    compactScopedLabel: document.getElementById('compactScopedLabel'),
    compactScopedFill: document.getElementById('compactScopedFill'),
    compactScopedPct: document.getElementById('compactScopedPct'),
    showClaudeCodeToggle: document.getElementById('showClaudeCodeToggle'),
    trayOpenaiBg: document.getElementById('trayOpenaiBg'),
    trayOpenaiText: document.getElementById('trayOpenaiText'),
    trayGoogleBg: document.getElementById('trayGoogleBg'),
    trayGoogleText: document.getElementById('trayGoogleText'),
    traySessionBg: document.getElementById('traySessionBg'),
    traySessionText: document.getElementById('traySessionText'),
    trayWeeklyBg: document.getElementById('trayWeeklyBg'),
    trayWeeklyText: document.getElementById('trayWeeklyText'),
    trayFableBg: document.getElementById('trayFableBg'),
    trayFableText: document.getElementById('trayFableText'),
    trayOutlineToggle: document.getElementById('trayOutlineToggle'),
    trayOutlineColor: document.getElementById('trayOutlineColor'),
    burnAlertsToggle: document.getElementById('burnAlertsToggle'),
    fontColorToggle: document.getElementById('fontColorToggle'),
    fontColorPicker: document.getElementById('fontColorPicker'),
    planNote: document.getElementById('planNote'),
    planNoteOpenai: document.getElementById('planNoteOpenai'),
    planNoteGoogle: document.getElementById('planNoteGoogle'),
    webhookToggle: document.getElementById('webhookToggle'),
    webhookUrl: document.getElementById('webhookUrl'),
    dailyDigestToggle: document.getElementById('dailyDigestToggle'),
    sortByUsageToggle: document.getElementById('sortByUsageToggle'),
    showAccountEmailsToggle: document.getElementById('showAccountEmailsToggle'),
    showCodexToggle: document.getElementById('showCodexToggle'),
    showCodexCliToggle: document.getElementById('showCodexCliToggle'),
    showGeminiToggle: document.getElementById('showGeminiToggle'),
    showGeminiCliToggle: document.getElementById('showGeminiCliToggle'),
    googleSource: document.getElementById('googleSource'),
    compactSettingsOverlay: document.getElementById('compactSettingsOverlay'),
    closeCompactSettingsBtn: document.getElementById('closeCompactSettingsBtn')
};

// Populate organization selector dropdown
function populateOrgSelector(organizations, selectedOrgId) {
    if (!organizations || organizations.length === 0) {
        // No orgs - hide selector column
        elements.orgSelectorCol.style.display = 'none';
        return;
    }

    // Only show selector if user has multiple chat orgs
    if (organizations.length > 1) {
        elements.orgSelectorCol.style.display = '';  // Show column (use default flex display)
        
        // Clear existing options
        elements.orgSelector.innerHTML = '';
        
        // Add each org as an option
        organizations.forEach(org => {
            const option = document.createElement('option');
            option.value = org.id;
            option.textContent = `${org.name}${org.isTeam ? ' (Team)' : ' (Personal)'}`;
            if (org.id === selectedOrgId) {
                option.selected = true;
            }
            elements.orgSelector.appendChild(option);
        });
    } else {
        // Single org - hide selector column
        elements.orgSelectorCol.style.display = 'none';
    }
}

// Handle organization change — main only needs the org id
async function handleOrgChange() {
    const newOrgId = elements.orgSelector.value;
    if (newOrgId && newOrgId !== credentials.organizationId) {
        credentials.organizationId = newOrgId;
        await window.electronAPI.setOrganization(newOrgId);
        // Refresh usage data with new org
        await fetchUsageData();
    }
}

// Initialize
async function init() {
    setupEventListeners();
    initSubheadings();
    // Give every settings toggle an accessible name from its row label
    document.querySelectorAll('.settings-col').forEach((col) => {
        const label = col.querySelector('.settings-row-label');
        const input = col.querySelector('.toggle-switch input');
        if (label && input && !input.getAttribute('aria-label')) {
            input.setAttribute('aria-label', label.textContent.trim());
        }
    });
    // Icon buttons whose visible glyph (⚙️, −, ×) would otherwise become
    // their accessible name get a real one from their tooltip instead
    document.querySelectorAll('button[title]:not([aria-label])').forEach((btn) => {
        btn.setAttribute('aria-label', btn.title.split('—')[0].trim());
    });
    credentials = await window.electronAPI.getCredentials();

    // Apply saved theme and load thresholds immediately
    const settings = await window.electronAPI.getSettings();
    window._cachedSettings = settings;
    applyTheme(settings.theme);
    syncThemeCycleBtn(settings.theme);
    if (window.electronAPI.platform === 'darwin') {
        document.getElementById('trayLabel').textContent = 'Hide from Dock';
    }
    warnThreshold = settings.warnThreshold;
    dangerThreshold = settings.dangerThreshold;
    applyFontColor(settings);
    applySectionStates(settings);

    // Restore compact mode from saved settings
    if (settings.compactMode) {
        applyCompactMode(true);
    } else {
        // Ensure compact overlay is hidden in normal mode
        if (elements.compactSettingsOverlay) elements.compactSettingsOverlay.style.display = 'none';
    }

    // Restore graph visibility
    if (settings.graphVisible) {
        if (!settings.compactMode) {
            // Normal mode — show graph immediately
            graphVisible = true;
            elements.graphBtn.classList.add('active');
            elements.graphSection.style.display = 'block';
        } else {
            // Compact mode — store so it restores when exiting compact
            graphWasVisible = true;
        }
    }
    syncGraphLayoutState();

    // Restore expanded state (default: everything expanded)
    if (settings.expandedOpen !== false) {
        isExpanded = true;
        elements.expandArrow.classList.add('expanded');
        elements.expandSection.style.display = 'block';
    }
    isOpenaiExtrasOpen = settings.openaiExtrasOpen !== false;
    elements.openaiExpandArrow.classList.toggle('expanded', isOpenaiExtrasOpen);
    elements.openaiExtras.style.display = isOpenaiExtrasOpen ? 'block' : 'none';
    projectionsVisible = settings.projectionsOn !== false;
    applyPsychicState();
    applyPizazz(settings.pizazz !== false);

    if (hasUsableCredentials()) {
        // Populate org selector if user has multiple orgs
        if (credentials.organizations && credentials.organizations.length > 0) {
            populateOrgSelector(credentials.organizations, credentials.organizationId);
        }
        showMainContent();
        await fetchUsageData();
        startAutoUpdate();
    } else {
        showLoginRequired();
    }

    // Populate version label then check for updates after a short delay
    const version = await window.electronAPI.getAppVersion();
    if (elements.settingsVersionLabel) {
        elements.settingsVersionLabel.textContent = `Application Version: v${version}`;
    }
    setTimeout(checkForUpdate, 2000);
    // Re-check every 3 hours (was 24h) so a missed/failed check heals within
    // hours rather than a day.
    setInterval(checkForUpdate, 3 * 60 * 60 * 1000);

    // Startup restore complete — allow _saveViewState to persist changes
    appInitializing = false;
}

// Merge a partial settings change into the cached settings and persist —
// used by controls that live outside the settings overlay (section headers)
async function _saveSettingsPatch(patch) {
    const settings = window._cachedSettings || await window.electronAPI.getSettings();
    Object.assign(settings, patch);
    window._cachedSettings = settings;
    await window.electronAPI.saveSettings(settings);
}

// Provider sections: collapsible, each with its own tray-icon checkbox
const PROVIDER_SECTIONS = [
    { key: 'anthropic', traySetting: 'showTrayStats' },
    { key: 'openai', traySetting: 'trayOpenai' },
    { key: 'google', traySetting: 'trayGoogle' }
];

function sectionEls(key) {
    const cap = key.charAt(0).toUpperCase() + key.slice(1);
    return {
        header: elements['header' + cap],
        body: elements['body' + cap],
        eye: elements['eye' + cap]
    };
}

function applySectionStates(settings) {
    const collapsed = settings.sectionCollapsed || {};
    for (const { key, traySetting } of PROVIDER_SECTIONS) {
        const { header, body, eye } = sectionEls(key);
        if (!header) continue;
        header.classList.toggle('collapsed', !!collapsed[key]);
        body.classList.toggle('collapsed', !!collapsed[key]);
        if (eye) eye.classList.toggle('on', settings[traySetting] === true);
    }
}

function setupProviderSections() {
    for (const { key, traySetting } of PROVIDER_SECTIONS) {
        const { header, body, eye } = sectionEls(key);
        if (!header) continue;
        // Headers are div click-targets — make them keyboard-operable too
        header.setAttribute('tabindex', '0');
        header.setAttribute('role', 'button');
        header.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); header.click(); }
        });
        header.addEventListener('click', async (e) => {
            if (e.target.closest('.section-eye')) return; // eye handles itself
            const nowCollapsed = !body.classList.contains('collapsed');
            header.classList.toggle('collapsed', nowCollapsed);
            body.classList.toggle('collapsed', nowCollapsed);
            if (!isCompactMode) resizeWidget();
            const settings = window._cachedSettings || await window.electronAPI.getSettings();
            const sectionCollapsed = { ...(settings.sectionCollapsed || {}), [key]: nowCollapsed };
            await _saveSettingsPatch({ sectionCollapsed });
        });
        if (eye) {
            eye.addEventListener('click', async (e) => {
                e.stopPropagation();
                const nowOn = !eye.classList.contains('on');
                eye.classList.toggle('on', nowOn);
                // Closing the Anthropic eye while hidden from taskbar would
                // leave no icon to restore the app from
                if (key === 'anthropic' && !nowOn && elements.minimizeToTrayToggle.checked) {
                    elements.minimizeToTrayToggle.checked = false;
                    await _saveSettingsPatch({ minimizeToTray: false });
                }
                await _saveSettingsPatch({ [traySetting]: nowOn });
            });
        }
    }
}

// Event Listeners
// ---- Alert sounds -------------------------------------------------------
// Two events make noise: a limit clearing EARLY (a banked/immediate reset),
// and the burn detector tripping. Either can be switched off, pointed at the
// user's own file, or volume-adjusted in Settings.
const SOUND_DEFAULTS = {
    reset: { src: '../../assets/sounds/reset-default.mp3', label: 'Default (heavenly choir)' },
    burn: { src: '../../assets/sounds/burn-default.wav', label: 'Default (fire)' },
    // A banked weekly-limit reset arriving in the OpenAI account. Distinct
    // from `reset` (a limit clearing early) because it is a different event —
    // credit landing in the bank, not a window rolling over.
    banked: { src: '../../assets/sounds/banked-default.mp3', label: 'Default (banked reset)' },
    // A pool crossing to 100% — you just hit the wall. Bad news gets a thud,
    // not a fanfare.
    wall: { src: '../../assets/sounds/wall-default.mp3', label: 'Default (rock punch)' }
};
const _soundCache = {};          // kind -> resolved src (data: URL for custom files)
let _soundPlaying = {};          // kind -> Audio, so a repeat retriggers cleanly

function soundCfg(kind) {
    const s = (window._cachedSettings && window._cachedSettings.sounds) || {};
    return { enabled: true, path: null, volume: 0.85, ...(s[kind] || {}) };
}
async function resolveSoundSrc(kind) {
    const cfg = soundCfg(kind);
    if (!cfg.path) return SOUND_DEFAULTS[kind].src;
    if (_soundCache[kind] && _soundCache[kind].path === cfg.path) return _soundCache[kind].src;
    const res = await window.electronAPI.readSoundFile(cfg.path);
    if (!res || !res.ok) {
        debugLog('[Sound] custom file unreadable, using default:', res && res.error);
        return SOUND_DEFAULTS[kind].src;
    }
    _soundCache[kind] = { path: cfg.path, src: res.dataUrl };
    return res.dataUrl;
}
async function playAlertSound(kind, { force = false } = {}) {
    const cfg = soundCfg(kind);
    if (!force && cfg.enabled === false) return;
    try {
        const src = await resolveSoundSrc(kind);
        const prev = _soundPlaying[kind];
        if (prev) { try { prev.pause(); } catch (_) {} }
        const audio = new Audio(src);
        audio.volume = Math.min(Math.max(cfg.volume, 0), 1);
        _soundPlaying[kind] = audio;
        await audio.play();
    } catch (err) {
        debugLog('[Sound] playback failed:', err && err.message);
    }
}

// Burn-spike: fire once when a series newly starts burning.
let _prevBurningKeys = new Set();
let _burnWatchSeeded = false;
function checkBurnSpikeSound(keys) {
    if (_burnWatchSeeded) {
        for (const k of keys) {
            if (!_prevBurningKeys.has(k)) { playAlertSound('burn'); break; }
        }
    }
    _prevBurningKeys = new Set(keys);
    _burnWatchSeeded = true;
}

function setupSoundSettings() {
    const wire = (kind, ids) => {
        const toggle = document.getElementById(ids.toggle);
        const vol = document.getElementById(ids.volume);
        const nameEl = document.getElementById(ids.name);
        const setName = () => {
            const cfg = soundCfg(kind);
            nameEl.textContent = cfg.path ? cfg.path.split('/').pop() : SOUND_DEFAULTS[kind].label;
            nameEl.title = cfg.path || SOUND_DEFAULTS[kind].label;
        };
        const patch = async (changes) => {
            const sounds = { ...((window._cachedSettings || {}).sounds || {}) };
            sounds[kind] = { ...soundCfg(kind), ...changes };
            await _saveSettingsPatch({ sounds });
            setName();
        };
        if (toggle) toggle.addEventListener('change', () => patch({ enabled: toggle.checked }));
        if (vol) vol.addEventListener('change', () => patch({ volume: vol.value / 100 }));
        const testBtn = document.getElementById(ids.test);
        if (testBtn) testBtn.addEventListener('click', () => playAlertSound(kind, { force: true }));
        const pickBtn = document.getElementById(ids.pick);
        if (pickBtn) pickBtn.addEventListener('click', async () => {
            const res = await window.electronAPI.pickSoundFile();
            if (res && res.ok) { delete _soundCache[kind]; await patch({ path: res.path }); }
        });
        const resetBtn = document.getElementById(ids.reset);
        if (resetBtn) resetBtn.addEventListener('click', async () => {
            delete _soundCache[kind]; await patch({ path: null });
        });
        setName();
    };
    wire('reset', { toggle:'soundResetToggle', volume:'soundResetVolume', name:'soundResetName',
                    test:'soundResetTest', pick:'soundResetPick', reset:'soundResetReset' });
    wire('burn', { toggle:'soundBurnToggle', volume:'soundBurnVolume', name:'soundBurnName',
                   test:'soundBurnTest', pick:'soundBurnPick', reset:'soundBurnReset' });
    wire('banked', { toggle:'soundBankedToggle', volume:'soundBankedVolume', name:'soundBankedName',
                     test:'soundBankedTest', pick:'soundBankedPick', reset:'soundBankedReset' });
    wire('wall', { toggle:'soundWallToggle', volume:'soundWallVolume', name:'soundWallName',
                   test:'soundWallTest', pick:'soundWallPick', reset:'soundWallReset' });
}

function setupEventListeners() {
    // The account-email hide pills hide every account email at once, like the
    // row-hide pills hide a row. Restored from Settings.
    document.querySelectorAll('.account-email-hide').forEach((btn) => {
        btn.addEventListener('click', async (e) => {
            e.stopPropagation();
            await _saveSettingsPatch({ hideAccountEmails: true });
            if (latestUsageData) renderAccountEmails(latestUsageData);
        });
    });

    // Provider dynamite (header): Shockwave & Ash, then the permahide.
    for (const prov of Object.keys(PERMAHIDE_SECTIONS)) {
        const tnt = document.getElementById('tnt' + PERMAHIDE_TITLECASE[prov]);
        if (tnt) tnt.addEventListener('click', (e) => { e.stopPropagation(); detonateProvider(prov); });
        // Settings: Hide/Restore (no fireworks — the section is behind the panel)
        const hideBtn = document.getElementById('providerHide' + PERMAHIDE_TITLECASE[prov]);
        if (hideBtn) hideBtn.addEventListener('click', async () => {
            const hidden = ((window._cachedSettings || {}).hiddenProviders || {})[prov] === true;
            await setProviderHidden(prov, !hidden);
        });
        // Settings: pull-from-CLI adoption toggle
        const adoptToggle = document.getElementById('cliAdopt' + PERMAHIDE_TITLECASE[prov] + 'Toggle');
        if (adoptToggle) adoptToggle.addEventListener('change', async () => {
            await window.electronAPI.setCliAdopted(prov, adoptToggle.checked);
            await fetchUsageData({ forceProviders: true, refreshLocalCredentials: true });
        });
    }

    setupSoundSettings();
    elements.refreshBtn.addEventListener('click', async () => {
        debugLog('Refresh button clicked');
        elements.refreshBtn.classList.add('spinning');
        credentials = await window.electronAPI.getCredentials();
        await fetchUsageData({ forceProviders: true });
        elements.refreshBtn.classList.remove('spinning');
    });

    elements.graphBtn.addEventListener('click', async () => {
        graphVisible = !graphVisible;
        elements.graphBtn.classList.toggle('active', graphVisible);
        // Keep the inline graph hidden while it's detached into its own window.
        elements.graphSection.style.display = (graphVisible && !graphDetached) ? 'block' : 'none';
        syncGraphLayoutState();
        if (graphVisible && !graphDetached) {
            await loadChart();
            requestAnimationFrame(() => usageChart?.resize());
        }
        _forceFitHeight({
            fitPreset: _activePreset !== null,
            intrinsic: !_graphIsInline(),
            userAction: true
        });
        // Turning the graph on inside the wide preset needs the taller,
        // graph-aware fit so the chart isn't clipped.
        _fitWidePresetWithGraph();
        _saveViewState();
    });

    // Wide / tall preset arrangements — one click snaps the window to a
    // landscape or tall geometry; the existing squeeze/landscape/tall reflow
    // then engages smoothly (main leaves the size trackers alone so the
    // window reports "user-sized").
    // Pop the graph out into its own always-on-top window. While detached, the
    // inline graph is hidden in the main window; it returns when the pop-out closes.
    if (elements.graphPopoutBtn) {
        elements.graphPopoutBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            window.electronAPI.openGraphWindow();
            graphDetached = true;
            elements.graphPopoutBtn.classList.add('active');
            if (elements.graphSection) elements.graphSection.style.display = 'none';
            syncGraphLayoutState();
            _forceFitHeight({ fitPreset: _activePreset !== null, intrinsic: true, userAction: true });
        });
        if (window.electronAPI.onGraphWindowClosed) {
            window.electronAPI.onGraphWindowClosed(() => {
                graphDetached = false;
                elements.graphPopoutBtn.classList.remove('active');
                if (graphVisible && elements.graphSection) {
                    elements.graphSection.style.display = 'block';
                    loadChart();
                    requestAnimationFrame(() => usageChart?.resize());
                }
                syncGraphLayoutState();
                _forceFitHeight({ fitPreset: _activePreset !== null, intrinsic: !_graphIsInline(), userAction: true });
            });
        }
    }

    // Buttons toggle: clicking the active layout again returns to the default
    // auto-sized widget.
    if (elements.wideBtn) {
        elements.wideBtn.addEventListener('click', () => {
            const goWide = _activePreset !== 'wide';
            _activePreset = goWide ? 'wide' : null;
            window.electronAPI.applyWindowPreset(goWide ? 'wide' : 'reset');
            elements.wideBtn.classList.toggle('active', goWide);
            if (elements.tallBtn) elements.tallBtn.classList.remove('active');
            _fitPresetHeight();
            _fitWidePresetWithGraph();
            if (goWide) setTimeout(syncLandscapeCliWidth, 180);
        });
    }
    if (elements.tallBtn) {
        elements.tallBtn.addEventListener('click', () => {
            const goTall = _activePreset !== 'tall';
            _activePreset = goTall ? 'tall' : null;
            window.electronAPI.applyWindowPreset(goTall ? 'tall' : 'reset');
            elements.tallBtn.classList.toggle('active', goTall);
            if (elements.wideBtn) elements.wideBtn.classList.remove('active');
            _fitPresetHeight();
        });
    }

    elements.minimizeBtn.addEventListener('click', () => {
        window.electronAPI.minimizeWindow();
    });

    elements.closeBtn.addEventListener('click', () => {
        window.electronAPI.closeWindow();
    });

    // Expand/collapse toggle
    elements.expandToggle.addEventListener('click', async () => {
        const wasExpanded = isExpanded;
        isExpanded = !isExpanded;
        elements.expandArrow.classList.toggle('expanded', isExpanded);
        elements.expandSection.style.display = isExpanded ? 'block' : 'none';
        if (graphVisible) {
            loadChart();
        }
        resizeWidget();
        
        // CRITICAL: Update expandedOpen setting IMMEDIATELY (no debounce) to prevent race condition
        // If we wait for the debounced save, auto-refresh might fetch with stale expandedOpen=false
        const settings = window._cachedSettings || await window.electronAPI.getSettings();
        settings.expandedOpen = isExpanded;
        window._cachedSettings = settings;
        await window.electronAPI.saveSettings(settings);
        
        // Trigger immediate fetch if panel was just opened (collapsed → expanded)
        // This ensures fresh overage/prepaid data is available when user expands the panel
        // Pass forceExtended to bypass any cached setting and fetch extended data immediately
        if (!wasExpanded && isExpanded) {
            debugLog('[Conditional Polling] Panel expanded - triggering immediate fetch with extended data');
            await fetchUsageData({ forceExtended: true });
        }
    });

    // Settings close — return to exactly the window the user had before
    elements.closeSettingsBtn.addEventListener('click', async () => {
        await saveSettings();
        elements.settingsOverlay.style.display = 'none';
        if (_settingsOpenedFromCompact) {
            _settingsOpenedFromCompact = false;
            // Re-enter compact ourselves; don't restore the pre-compact bounds
            window.electronAPI.settingsRestore({ reCompact: true });
            window.electronAPI.setCompactMode(true);
        } else {
            window.electronAPI.settingsRestore();
        }
        if (_settingsReturnToWide) {
            _settingsReturnToWide = false;
            if (elements.wideBtn && _activePreset !== 'wide') {
                setTimeout(() => elements.wideBtn.click(), 200);
            }
        } else if (_settingsReturnBounds) {
            const bounds = _settingsReturnBounds;
            _settingsReturnBounds = null;
            // Release the tall preset we borrowed, then put the exact
            // hand-sized geometry back.
            if (_activePreset === 'tall') {
                _activePreset = null;
                if (elements.tallBtn) elements.tallBtn.classList.remove('active');
                window.electronAPI.applyWindowPreset('reset');
            }
            setTimeout(() => {
                if (window.electronAPI.setWindowBounds) window.electronAPI.setWindowBounds(bounds);
            }, 250);
        }
        startAutoUpdate();
        // Account toggles filter post-fetch, so a refetch applies them now
        await fetchUsageData();
    });

    elements.logoutBtn.addEventListener('click', handleAnthropicAuthAction);
    elements.refreshLocalLoginsBtn.addEventListener('click', refreshLocalLogins);

    elements.githubBtn.addEventListener('click', () => {
        window.electronAPI.openExternal('https://github.com/dev-newb/imburning-electron');
    });

    document.getElementById('coffeeBtn').addEventListener('click', () => {
        window.electronAPI.openExternal('https://buymeacoffee.com/devnewb');
    });

    const exportHistory = async (format) => {
        const status = document.getElementById('exportStatus');
        if (status) status.textContent = 'Saving…';
        try {
            const r = await window.electronAPI.exportHistory(format);
            if (status) status.textContent = r.ok ? `Saved ${r.count} rows` : (r.canceled ? '' : (r.error || 'Failed'));
        } catch (e) { if (status) status.textContent = 'Failed'; }
    };
    document.getElementById('exportCsvBtn').addEventListener('click', () => exportHistory('csv'));
    document.getElementById('exportJsonBtn').addEventListener('click', () => exportHistory('json'));

    document.getElementById('creditLink').addEventListener('click', () => {
        window.electronAPI.openExternal('https://github.com/SlavomirDurej');
    });

    // The hydraulic press: crushes the widget into compact mode
    const pressBtn = document.getElementById('compactPressBtn');
    pressBtn.addEventListener('click', async () => {
        const compact = !isCompactMode;
        applyCompactMode(compact);
        await _saveCompactSetting(compact);
    });

    // The clown: jail him to turn off all visual pizazz
    const clownBtn = document.getElementById('clownBtn');
    clownBtn.addEventListener('click', async () => {
        const jailed = !document.body.classList.contains('no-pizazz');
        applyPizazz(!jailed);
        await _saveSettingsPatch({ pizazz: !jailed });
    });

    // Official OAuth connect paths (OpenAI / Google): section connect rows,
    // the clickable 'via CLI login' chips, and the Settings Connect buttons
    const runConnect = async (provider, busyFn, doneFn, errFn) => {
        if (busyFn) busyFn();
        const result = await window.electronAPI.oauthConnect(provider);
        if (doneFn) doneFn();
        if (result.ok) {
            await fetchUsageData({ forceExtended: true });
            if (elements.settingsOverlay.style.display !== 'none') await loadSettings();
        } else if (errFn) {
            errFn(result.error || 'Connection failed');
        }
    };
    const wireConnect = (btn, errEl, provider, extraErrFn) => {
        if (!btn) return;
        btn.addEventListener('click', () => {
            const original = btn.textContent;
            runConnect(provider,
                () => { btn.disabled = true; btn.textContent = 'Waiting for browser sign-in...'; if (errEl) errEl.textContent = ''; },
                () => { btn.disabled = false; btn.textContent = original; },
                (msg) => { if (errEl) errEl.textContent = msg; if (extraErrFn) extraErrFn(msg); });
        });
    };
    wireConnect(elements.connectOpenaiBtn, elements.connectErrorOpenai, 'openai');
    wireConnect(elements.connectGoogleBtn, elements.connectErrorGoogle, 'google');
    // The Settings buttons report through the login-status line — passing null
    // here silently swallowed every failure (port busy, refused login, timeout),
    // which read as "the Connect button does nothing".
    const settingsErr = (statusEl) => (msg) => {
        if (!statusEl) return;
        statusEl.textContent = msg;
        statusEl.classList.add('login-status-error');
    };
    wireConnect(elements.settingsConnectOpenaiBtn, null, 'openai',
        settingsErr(elements.openaiLoginStatus));
    wireConnect(elements.settingsConnectGoogleBtn, null, 'google',
        settingsErr(elements.googleLoginStatus));

    // The 'via CLI login' chips are purely informational. They used to start
    // an OAuth sign-in, which implied a CLI login is a lesser state that
    // wants upgrading — it isn't; plenty of setups are CLI-only by choice.
    // Signing in with OAuth is still available in Settings for anyone who
    // wants a widget-owned connection.

    const wireDisconnect = (btn, provider) => {
        if (!btn) return;
        btn.addEventListener('click', async () => {
            await window.electronAPI.oauthDisconnect(provider);
            await fetchUsageData({ forceExtended: true });
            await loadSettings();
        });
    };
    wireDisconnect(elements.disconnectOpenaiBtn, 'openai');
    wireDisconnect(elements.disconnectGoogleBtn, 'google');

    // The psychic: toggles the forecast projection lines on the graph
    elements.psychicBtn.addEventListener('click', async () => {
        projectionsVisible = !projectionsVisible;
        applyPsychicState();
        if (graphVisible) await loadChart();
        await _saveSettingsPatch({ projectionsOn: projectionsVisible });
    });

    // OpenAI extras (Credits + Limit Resets) collapse toggle
    elements.openaiExpandToggle.addEventListener('click', async () => {
        isOpenaiExtrasOpen = !isOpenaiExtrasOpen;
        elements.openaiExpandArrow.classList.toggle('expanded', isOpenaiExtrasOpen);
        elements.openaiExtras.style.display = isOpenaiExtrasOpen ? 'block' : 'none';
        if (!isCompactMode) resizeWidget();
        await _saveSettingsPatch({ openaiExtrasOpen: isOpenaiExtrasOpen });
    });

    // Theme buttons
    // Theme cycle button (leftmost on the toolbar): dark -> light -> system.
    // Applies immediately and persists on its own — no Settings visit needed.
    if (elements.themeCycleBtn) {
        elements.themeCycleBtn.addEventListener('click', async () => {
            const order = ['dark', 'light', 'system'];
            const current = (window._cachedSettings && window._cachedSettings.theme) || 'dark';
            const next = order[(order.indexOf(current) + 1) % order.length];
            syncThemeCycleBtn(next);
            applyTheme(next);
            await _saveSettingsPatch({ theme: next });
        });
    }

    // Prevent accidental app hiding: enabling "Hide from Taskbar" force-enables
    // the Anthropic tray icons (ensures a tray icon exists to restore from)
    elements.minimizeToTrayToggle.addEventListener('change', () => {
        if (elements.minimizeToTrayToggle.checked && !elements.eyeAnthropic.classList.contains('on')) {
            elements.eyeAnthropic.classList.add('on');
            _saveSettingsPatch({ showTrayStats: true });
        }
    });

    setupProviderSections();

    // Click a bar that's on fire to switch its flame style (classic pixel ⇄
    // particle inferno). Only burning bars respond; the ambient fire loop
    // reads the new style on its very next frame, so it changes live.
    document.addEventListener('click', (e) => {
        const group = e.target.closest('.usage-bar-group, .compact-bar-wrap');
        if (!group || !group.querySelector('.progress-fill.on-fire, .compact-bar-fill.on-fire')) return;
        const next = (window._cachedSettings || {}).flameStyle === 'particle' ? 'classic' : 'particle';
        _saveSettingsPatch({ flameStyle: next });
        _flameStyleToast(group, next);
    });

    // Keyboard access for the click-only expand toggles (headers get theirs
    // inside setupProviderSections)
    for (const toggle of [elements.expandToggle, elements.openaiExpandToggle]) {
        if (!toggle) continue;
        toggle.setAttribute('tabindex', '0');
        toggle.setAttribute('role', 'button');
        toggle.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle.click(); }
        });
    }

    // Listen for refresh requests from tray
    window.electronAPI.onRefreshUsage(async () => {
        if (elements.refreshBtn) elements.refreshBtn.classList.add('spinning');
        credentials = await window.electronAPI.getCredentials();
        await fetchUsageData({ forceProviders: true });
        if (elements.refreshBtn) elements.refreshBtn.classList.remove('spinning');
    });

    if (window.electronAPI.onGraphSettingsUpdated) {
        window.electronAPI.onGraphSettingsUpdated(async () => {
            const settings = await window.electronAPI.getSettings();
            window._cachedSettings = settings;
            projectionsVisible = settings.projectionsOn !== false;
            applyPsychicState();
            if (graphVisible && !graphDetached) await loadChart();
        });
    }

    // Listen for session expiration events (403 errors)
    window.electronAPI.onSessionExpired(async () => {
        debugLog('Session expired event received');
        credentials = await window.electronAPI.getCredentials();
        if (hasUsableCredentials()) {
            showMainContent();
            await fetchUsageData({ forceExtended: true, forceProviders: true });
        } else {
            showLoginRequired();
        }
    });

    // Quiet, non-destructive notice when Claude.ai has failed several
    // consecutive refreshes (a session that is dead behind an HTML block is
    // deliberately NOT wiped — this banner is the recovery path instead).
    if (window.electronAPI.onAnthropicDegraded) {
        window.electronAPI.onAnthropicDegraded((degraded) => {
            const banner = document.getElementById('staleBanner');
            if (!banner) return;
            const show = !!degraded && hasClaudeWebCredentials();
            const wasShown = banner.style.display !== 'none';
            banner.style.display = show ? 'flex' : 'none';
            if (wasShown !== show && !isCompactMode) resizeWidget();
        });
    }

    // Update banner
    elements.updateBannerDismiss.addEventListener('click', () => {
        elements.updateBanner.style.display = 'none';
        resizeWidget();
    });
    // One click path for the banner and the settings link alike: apply a
    // downloaded update (Windows), rebuild in place (macOS source install),
    // or fall back to opening the releases page.
    const applyUpdateClick = () => {
        if (_updateReadyToInstall) {
            window.electronAPI.installUpdate();
        } else if (_canSelfUpdate) {
            // The app is about to quit so its own bundle can be replaced.
            elements.updateBannerText.textContent = '▲  Updating — reopens when the rebuild finishes';
            window.electronAPI.runMacUpdate();
        } else {
            window.electronAPI.openExternal(`https://github.com/dev-newb/imburning-electron/releases/latest`);
        }
    };
    elements.updateBannerText.addEventListener('click', applyUpdateClick);
    elements.settingsUpdateLink.addEventListener('click', applyUpdateClick);

    // Auto-update: a downloaded release turns the banner into a one-click
    // "restart & apply" — no installer wizard involved
    window.electronAPI.onUpdateDownloaded((version) => {
        _updateReadyToInstall = true;
        elements.updateBannerText.textContent = `▲  v${version} downloaded — click to restart & apply`;
        elements.updateBanner.style.display = 'flex';
        if (elements.settingsUpdateLink) {
            elements.settingsUpdateLink.textContent = `→ v${version} ready — click to apply`;
            elements.settingsUpdateLink.style.display = 'inline';
        }
        if (!isCompactMode) resizeWidget(true);
    });

    // Compact mode enter/exit lives on the title-bar press button (the old
    // edge chevrons are fully retired — markup, CSS and wiring)

    // Compact mode toggle in normal settings panel — deferred to Done click

    // Compact mode toggle in compact settings panel — just updates the checkbox, Done applies it
    elements.compactModeToggleCompact.addEventListener('change', () => {
        // No immediate action — Done button reads this value and applies
    });

    // Organization selector — change triggers immediate save and refresh
    elements.orgSelector.addEventListener('change', handleOrgChange);

    // Settings button — always open full settings; if in compact mode, temporarily expand the window first
    elements.settingsBtn.addEventListener('click', async () => {
        stopAutoUpdate();
        if (isCompactMode) {
            _settingsOpenedFromCompact = true;
            window.electronAPI.setCompactMode(false);
        }
        // Settings is designed for the tall/portrait layout. Opening it from
        // the rectangular window kept the wide geometry and squeezed the
        // panel; switch to tall first, and go back to wide on close when
        // wide was the active preset.
        if (document.body.classList.contains('landscape')) {
            _settingsReturnToWide = _activePreset === 'wide';
            // A hand-stretched landscape window (no preset) gets its exact
            // geometry back after Settings closes.
            _settingsReturnBounds = (!_activePreset && window.electronAPI.getWindowBounds)
                ? await window.electronAPI.getWindowBounds() : null;
            if (elements.tallBtn && _activePreset !== 'tall') elements.tallBtn.click();
            await new Promise((resolve) => setTimeout(resolve, 350));
        }
        await loadSettings();
        elements.settingsOverlay.style.display = 'flex';
        // Grow the window to show EVERY setting (and lock resizing) once the
        // panel has laid out. Two frames: display change, then measure.
        requestAnimationFrame(() => requestAnimationFrame(fitSettingsWindow));
    });

    // Close compact settings — apply compact toggle value then close
    elements.closeCompactSettingsBtn.addEventListener('click', async () => {
        const compact = elements.compactModeToggleCompact.checked;
        if (compact !== isCompactMode) {
            applyCompactMode(compact);
            await _saveCompactSetting(compact);
        }
        elements.compactSettingsOverlay.style.display = 'none';
        startAutoUpdate();
    });
}

function syncAnthropicAuthControls() {
    const connected = hasClaudeWebCredentials();
    const usingCli = !connected && !!credentials?.cliFallbackAvailable;
    if (elements.anthropicLoginStatus) {
        // The CLI-fallback wording matters: after a logout the section keeps
        // showing data from the local claude CLI login, which used to read as
        // "the logout didn't work". Say exactly what is happening instead.
        elements.anthropicLoginStatus.textContent = connected
            ? 'Connected to Claude.ai'
            : (usingCli
                ? 'Claude.ai logged out — still tracking your local claude CLI login (toggle "CLI account" off to stop)'
                : 'Not connected');
    }
    if (elements.logoutBtn) {
        elements.logoutBtn.textContent = connected ? 'Log Out' : 'Log In';
        elements.logoutBtn.className = connected ? 'logout-btn' : 'settings-connect-btn';
        elements.logoutBtn.title = connected
            ? "Clear I'm Burning!'s Claude.ai login. Other provider accounts are unaffected."
            : 'Sign in to Claude.ai';
    }
}

async function handleAnthropicAuthAction() {
    const button = elements.logoutBtn;
    if (!button || button.disabled) return;

    button.disabled = true;
    try {
        if (hasClaudeWebCredentials()) {
            button.textContent = 'Logging out...';
            await window.electronAPI.deleteCredentials();
            credentials = await window.electronAPI.getCredentials();
            if (hasUsableCredentials()) {
                await fetchUsageData({ forceExtended: true, forceProviders: true });
                startAutoUpdate();
            } else {
                showLoginRequired();
                stopAutoUpdate();
            }
            await loadSettings();
            return;
        }

        button.textContent = 'Waiting for browser sign-in...';
        if (elements.anthropicLoginStatus) elements.anthropicLoginStatus.textContent = 'Waiting for Claude.ai...';
        // The whole login (browser window → validation → encrypted storage)
        // runs in the main process; only success/failure comes back here.
        const result = await window.electronAPI.anthropicLogin();
        if (!result.success) throw new Error(result.error || 'Login failed');

        credentials = await window.electronAPI.getCredentials();
        populateOrgSelector(credentials.organizations || [], credentials.organizationId);
        await fetchUsageData({ forceExtended: true, forceProviders: true });
        startAutoUpdate();
        await loadSettings();
    } catch (error) {
        credentials = await window.electronAPI.getCredentials();
        syncAnthropicAuthControls();
        if (elements.anthropicLoginStatus) {
            elements.anthropicLoginStatus.textContent = error.message || 'Login failed';
        }
    } finally {
        button.disabled = false;
        if (hasClaudeWebCredentials()) syncAnthropicAuthControls();
    }
}

async function refreshLocalLogins() {
    const button = elements.refreshLocalLoginsBtn;
    if (!button || button.disabled) return;

    const defaultTitle = 'Refresh local CLI logins and account tokens';
    clearTimeout(button._feedbackTimer);
    button.disabled = true;
    button.classList.remove('refresh-success', 'refresh-error');
    button.classList.add('spinning');
    button.title = 'Refreshing local CLI logins...';
    let feedbackClass = 'refresh-success';
    let feedbackTitle = 'Local CLI logins refreshed';

    try {
        // get-credentials reads the provider auth files again. A forced provider
        // fetch then bypasses the usage cache so an account switch is reflected
        // immediately instead of waiting for the app to restart or cache to age.
        credentials = await window.electronAPI.getCredentials();
        if (!credentials?.localProviderCredentialsAvailable) {
            await loadSettings();
            feedbackClass = 'refresh-error';
            feedbackTitle = 'No usable local CLI logins found';
            return;
        }

        await fetchUsageData({
            forceExtended: true,
            forceProviders: true,
            refreshLocalCredentials: true
        });
        await loadSettings();
        startAutoUpdate();
    } catch (error) {
        feedbackClass = 'refresh-error';
        feedbackTitle = error.message || 'Local login refresh failed';
    } finally {
        button.disabled = false;
        button.classList.remove('spinning');
        button.classList.add(feedbackClass);
        button.title = feedbackTitle;
        button._feedbackTimer = setTimeout(() => {
            button.classList.remove('refresh-success', 'refresh-error');
            button.title = defaultTitle;
        }, 2500);
    }
}

// Fetch usage data from Claude API
async function fetchUsageData(options = {}) {
    debugLog('fetchUsageData called');

    if (isFetching) {
        debugLog('Fetch already in flight — skipping');
        return;
    }

    if (!hasUsableCredentials()) {
        debugLog('Missing credentials, showing login');
        showLoginRequired();
        return;
    }

    isFetching = true;
    try {
        debugLog('Calling electronAPI.fetchUsageData...');
        const data = await window.electronAPI.fetchUsageData(options);
        debugLog('Received usage data:', data);
        updateUI(data);
    } catch (error) {
        console.error('Error fetching usage data:', error);
        if (error.message.includes('SessionExpired') || error.message.includes('Unauthorized')) {
            credentials = await window.electronAPI.getCredentials();
            if (hasUsableCredentials()) {
                showMainContent();
                setTimeout(() => fetchUsageData({ forceExtended: true, forceProviders: true }), 1500);
            } else {
                showLoginRequired();
            }
        } else if (error.message.includes('Missing credentials')) {
            showLoginRequired();
        } else {
            debugLog('Failed to fetch usage data');
        }
    } finally {
        isFetching = false;
    }
}


// Update UI with usage data
// Format a cent-based amount with the correct currency symbol.
// Known unambiguous symbols are used; everything else falls back to the
// ISO 4217 code as a suffix so the display is always correct.
function formatCurrency(amountCents, currencyCode) {
  const amount = (amountCents / 100).toFixed(2);
  const symbols = { USD: '$', EUR: '€', GBP: '£' };
  const sym = symbols[currencyCode];
  return sym ? `${sym}${amount}` : `${amount} ${currencyCode || 'USD'}`;
}

// Extra row label mapping for API fields
const EXTRA_ROW_CONFIG = {
    seven_day_sonnet: { label: 'Sonnet (7d)', color: 'sonnet' },
    seven_day_opus: { label: 'Opus (7d)', color: 'opus' },
    seven_day_cowork: { label: 'Cowork (7d)', color: 'cowork' },
    seven_day_omelette: { label: 'Design (7d)', color: 'design' },
    seven_day_oauth_apps: { label: 'OAuth Apps (7d)', color: 'oauth' },
    extra_usage: { label: 'Extra Usage', color: 'extra' },
};

// Keys that have data THIS refresh (regardless of hidden state) — the "N
// hidden" chip only counts hidden rows that actually exist, so it never shows
// a count for a pool the API stopped reporting.
const _availableRowKeys = new Set();

function appendCodexCreditsRow(codexData, key, container, hiddenRows) {
    const credits = codexData?.credits;
    if (!credits || !(codexData.limits || []).length) return;
    _availableRowKeys.add(key);
    if (hiddenRows[key]) return;

    const row = document.createElement('div');
    row.className = 'usage-section stretch-bar';
    const label = document.createElement('span');
    label.className = 'usage-label';
    label.dataset.code = 'CR';
    label.dataset.abbr = 'Credits';
    label.style.setProperty('--row-col', CODE_COLORS.codex);
    const creditsOn = credits.unlimited || credits.hasCredits;
    const statusTag = document.createElement('span');
    statusTag.className = `extra-status ${creditsOn ? 'on' : 'off'}`;
    statusTag.textContent = creditsOn ? 'ON' : 'OFF';
    label.appendChild(statusTag);
    label.appendChild(document.createTextNode(' Credits'));
    row.appendChild(label);

    const barGroup = document.createElement('div');
    barGroup.className = 'usage-bar-group';
    const approx = document.createElement('span');
    approx.className = 'credits-approx';
    const fmtRange = (range) => (Array.isArray(range) && range.length === 2)
        ? (range[0] === range[1] ? String(range[0]) : `${range[0]}–${range[1]}`)
        : null;
    if (credits.unlimited) {
        approx.textContent = 'unlimited';
    } else if (credits.hasCredits) {
        const local = fmtRange(credits.approxLocal);
        const cloud = fmtRange(credits.approxCloud);
        approx.textContent = [local && `≈ ${local} local msgs`, cloud && `${cloud} cloud`]
            .filter(Boolean).join(' · ') || 'balance available';
    } else {
        approx.textContent = 'none purchased';
        approx.classList.add('dim');
    }
    barGroup.appendChild(approx);
    row.appendChild(barGroup);

    // Label + amount live in one wrapper spanning the In/At columns, so the
    // pair centres as a unit on the columns' combined midline instead of
    // hanging off whichever column each half happened to occupy.
    const balPair = document.createElement('span');
    balPair.className = 'balance-pair';
    const balLabel = document.createElement('span');
    balLabel.className = 'timer-text extra-balance-label';
    balLabel.textContent = 'Credits:';
    balPair.appendChild(balLabel);
    const balAmount = document.createElement('span');
    balAmount.className = 'resets-at-text extra-balance-amount';
    balAmount.textContent = credits.unlimited ? 'unlimited' : String(credits.balance ?? 0);
    balPair.appendChild(balAmount);
    row.appendChild(balPair);

    attachHideBtn(row, key, 'Credits');
    container.appendChild(row);
}

// One orb, one banked reset, one popup. Says which of the N it is, when it
// lapses and how long that leaves — plus whatever label OpenAI attached.
function describeResetCredit(credit, index, total) {
    const settings = window._cachedSettings || {};
    const lines = [`Reset ${index + 1} of ${total}`];
    if (credit.title) lines[0] += ` — ${credit.title}`;
    if (credit.expiresAt) {
        const left = credit.expiresAt - Date.now();
        const when = formatResetsAt(new Date(credit.expiresAt).toISOString(), true,
            settings.timeFormat || '12h', 'date-day-time');
        lines.push(left > 0 ? `Expires ${when} (in ${formatCountdown(left)})` : `Expired ${when}`);
    } else {
        lines.push('No expiry reported');
    }
    if (credit.grantedAt) {
        lines.push(`Granted ${formatResetsAt(new Date(credit.grantedAt).toISOString(), true,
            settings.timeFormat || '12h', 'date-day-time')}`);
    }
    if (credit.description) lines.push(credit.description);
    return lines.join('\n');
}

function appendCodexResetsRow(codexData, key, container, hiddenRows) {
    const resets = codexData?.resetCredits;
    if (!resets || !(codexData.limits || []).length) return;
    _availableRowKeys.add(key);
    if (hiddenRows[key]) return;

    const row = document.createElement('div');
    row.className = 'usage-section stretch-bar';
    row.title = resets.applicable > 0
        ? `${resets.applicable} reset${resets.applicable > 1 ? 's' : ''} usable right now to clear a hit limit`
        : 'Banked limit resets — become usable when a limit is reached';
    const label = document.createElement('span');
    label.className = 'usage-label';
    label.textContent = 'Limit Resets';
    label.dataset.code = 'RST';
    label.dataset.abbr = 'Resets';
    label.style.setProperty('--row-col', CODE_COLORS.codex);
    row.appendChild(label);

    const barGroup = document.createElement('div');
    barGroup.className = 'usage-bar-group';
    const dotsWrap = document.createElement('div');
    const dotCount = Math.min(resets.available, 12);
    dotsWrap.className = 'reset-dots' + (dotCount === 1 ? ' single' : '');
    // Each banked reset expires on its own schedule, so each orb carries its
    // OWN credit and its own hover popup. Soonest first (main.js sorts them).
    const credits = Array.isArray(resets.credits) ? resets.credits : [];
    for (let i = 0; i < dotCount; i++) {
        const dot = document.createElement('span');
        dot.className = 'reset-dot';
        dot.style.animationDelay = `${(i * 1.3 + 0.4).toFixed(1)}s`;
        const credit = credits[i];
        if (credit) {
            dot.classList.add('has-detail');
            dot.title = describeResetCredit(credit, i, dotCount);
            // Green while there's time, amber inside a week, red inside a day.
            const left = credit.expiresAt ? credit.expiresAt - Date.now() : Infinity;
            if (left <= 24 * 60 * 60 * 1000) dot.classList.add('expiring-soon');
            else if (left <= 7 * 24 * 60 * 60 * 1000) dot.classList.add('expiring-week');
        }
        dotsWrap.appendChild(dot);
    }
    barGroup.appendChild(dotsWrap);
    row.appendChild(barGroup);

    // The row keeps the honest total. Expiries are per-orb — several banked
    // resets lapse at different times, so a single date in these columns
    // couldn't say which reset it referred to.
    const balPair = document.createElement('span');
    balPair.className = 'balance-pair';
    const balLabel = document.createElement('span');
    balLabel.className = 'timer-text extra-balance-label';
    balLabel.textContent = 'Resets:';
    balPair.appendChild(balLabel);
    const balAmount = document.createElement('span');
    balAmount.className = 'resets-at-text extra-balance-amount';
    balAmount.textContent = String(resets.available);
    balPair.appendChild(balAmount);
    row.appendChild(balPair);

    attachHideBtn(row, key, 'Limit Resets');
    container.appendChild(row);
}

function buildExtraRows(data) {
    // Don't clear existing rows if we don't have new data to replace them with
    // This preserves the last known state when expanding the panel
    const hasAnyExtendedData = Object.entries(EXTRA_ROW_CONFIG).some(([key, config]) => {
        const value = data[key];
        const hasUtilization = value && value.utilization !== undefined;
        const hasBalance = key === 'extra_usage' && value && value.balance_cents != null;
        return hasUtilization || hasBalance;
    });
    
    // Only rebuild if we have data, otherwise keep existing rows
    const existingRows = elements.extraRows.children.length + elements.scopedRows.children.length
        + elements.openaiRows.children.length + elements.openaiExtraRows.children.length + elements.googleRows.children.length
        + elements.anthropicCliRows.children.length + elements.openaiCliRows.children.length + elements.googleCliRows.children.length;
    if (!hasAnyExtendedData && existingRows > 0) {
        return; // Keep existing rows
    }

    _availableRowKeys.clear();
    elements.extraRows.innerHTML = '';
    elements.scopedRows.innerHTML = '';
    elements.openaiRows.innerHTML = '';
    elements.openaiExtraRows.innerHTML = '';
    elements.googleRows.innerHTML = '';
    elements.anthropicCliRows.innerHTML = '';
    elements.openaiCliRows.innerHTML = '';
    elements.googleCliRows.innerHTML = '';
    let count = 0;

    const hiddenRows = hiddenRowsMap();
    for (const [key, config] of Object.entries(EXTRA_ROW_CONFIG)) {
        const value = data[key];
        // extra_usage is valid with utilization OR balance_cents (prepaid only)
        const hasUtilization = value && value.utilization !== undefined;
        const hasBalance = key === 'extra_usage' && value && value.balance_cents != null;
        if (!hasUtilization && !hasBalance) continue;
        _availableRowKeys.add(key); // present in the data, hidden or not
        if (hiddenRows[key]) continue;

        const utilization = value.utilization || 0;
        const resetsAt = value.resets_at;
        const colorClass = config.color;

        const row = document.createElement('div');
        row.className = 'usage-section';

        // Build row using DOM methods (no innerHTML)
        const label = document.createElement('span');
        label.className = 'usage-label';
        
        // Narrow-width code chip (colour-matched to the bar) + abbreviated
        // form for the mid band
        label.dataset.code = rowCode(key, config.label);
        label.style.setProperty('--row-col', CODE_COLORS[config.color] || '#8b8fa3');
        label.dataset.abbr = config.label.replace(/^CLI /, '').replace(/\(daily\)/i, '(1D)');
        const tip = windowTip(config.label);
        if (tip) label.dataset.tip = tip;

        if (key === 'extra_usage') {
            // Extra usage: ON/OFF indicator goes next to label
            if (value.is_enabled === true) {
                const statusTag = document.createElement('span');
                statusTag.className = 'extra-status on';
                statusTag.textContent = 'ON';
                label.appendChild(statusTag);
            } else if (value.is_enabled === false) {
                const statusTag = document.createElement('span');
                statusTag.className = 'extra-status off';
                statusTag.textContent = 'OFF';
                label.appendChild(statusTag);
            }
            label.appendChild(document.createTextNode(' Extra Usage'));
        } else {
            label.textContent = config.label;
        }
        row.appendChild(label);

        if (key === 'extra_usage') {
            // Extra usage IS a meter when enabled (spend toward a monthly
            // cap) — the bar stays for that case. When OFF there is no cap
            // to fill against, so show a quiet note instead (same treatment
            // as OpenAI credits).
            const barGroup = document.createElement('div');
            barGroup.className = 'usage-bar-group';
            if (value.is_enabled === false) {
                const note = document.createElement('span');
                note.className = 'credits-approx dim';
                note.textContent = 'not enabled';
                barGroup.appendChild(note);
            } else {
                const progressBar = document.createElement('div');
                progressBar.className = 'progress-bar';
                const progressFill = document.createElement('div');
                progressFill.className = `progress-fill ${colorClass}`;
                progressFill.style.width = `${Math.min(utilization, 100)}%`;

                // Apply warning/danger thresholds to extra usage bar
                if (utilization >= dangerThreshold) {
                    progressFill.classList.add('danger');
                } else if (utilization >= warnThreshold) {
                    progressFill.classList.add('warning');
                }
                applyMaxedState(progressFill, utilization);

                progressBar.appendChild(progressFill);
                barGroup.appendChild(progressBar);

                const percentage = document.createElement('span');
                if (value.used_cents != null && value.limit_cents != null) {
                    percentage.className = 'usage-percentage extra-spending';
                    percentage.textContent = `${formatCurrency(value.used_cents, value.currency)}/${formatCurrency(value.limit_cents, value.currency)}`;
                } else {
                    percentage.className = 'usage-percentage';
                    percentage.textContent = `${Math.round(utilization)}%`;
                }
                barGroup.appendChild(percentage);
            }
            row.appendChild(barGroup);

            const elapsedGroup = document.createElement('div');
            elapsedGroup.className = 'usage-elapsed-group';
            row.appendChild(elapsedGroup);

            const balPair = document.createElement('span');
            balPair.className = 'balance-pair';
            const timerText = document.createElement('span');
            timerText.className = 'timer-text extra-balance-label';
            timerText.textContent = 'Credits:';
            balPair.appendChild(timerText);

            const resetsText = document.createElement('span');
            resetsText.className = 'resets-at-text extra-balance-amount';
            if (value.balance_cents != null) {
                resetsText.textContent = formatCurrency(value.balance_cents, value.currency);
            }
            balPair.appendChild(resetsText);
            row.appendChild(balPair);
        } else {
            // Prefer the window the backend states outright. Guessing it from
            // the key only works while a prefix implies one window length, and
            // Google broke that: classic Code Assist buckets are daily, but
            // Antigravity's pools reset roughly 5-hourly under the same
            // gemini_ prefix, which made the elapsed ring wrong by ~5x — it
            // showed a window as nearly over the moment it began.
            const totalMinutes = value.windowMinutes
                || (key.includes('seven_day') ? 7 * 24 * 60
                    : (key.includes('daily') || key.startsWith('gemini_')) ? 24 * 60 : 5 * 60);

            const barGroup = document.createElement('div');
            barGroup.className = 'usage-bar-group';
            const progressBar = document.createElement('div');
            progressBar.className = 'progress-bar';
            const progressFill = document.createElement('div');
            progressFill.className = `progress-fill ${colorClass}`;
            progressFill.style.width = `${Math.min(utilization, 100)}%`;
            // Burn-detector fire, tinted with this row's own bar colour
            if (_burningRowKeys.has(key)) {
                progressFill.classList.add('on-fire');
                progressFill.style.setProperty('--fire-col', CODE_COLORS[config.color] || '#8b8fa3');
            }
            applyMaxedState(progressFill, utilization);
            progressBar.appendChild(progressFill);
            barGroup.appendChild(progressBar);

            const percentage = document.createElement('span');
            percentage.className = 'usage-percentage';
            percentage.textContent = `${Math.round(utilization)}%`;
            barGroup.appendChild(percentage);
            row.appendChild(barGroup);

            const elapsedGroup = document.createElement('div');
            elapsedGroup.className = 'usage-elapsed-group';
            const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            svg.setAttribute('class', 'mini-timer');
            svg.setAttribute('width', '24');
            svg.setAttribute('height', '24');
            svg.setAttribute('viewBox', '0 0 24 24');
            const circleBg = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
            circleBg.setAttribute('class', 'timer-bg');
            circleBg.setAttribute('cx', '12');
            circleBg.setAttribute('cy', '12');
            circleBg.setAttribute('r', '10');
            svg.appendChild(circleBg);
            const circleProgress = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
            circleProgress.setAttribute('class', `timer-progress ${colorClass}`);
            circleProgress.setAttribute('cx', '12');
            circleProgress.setAttribute('cy', '12');
            circleProgress.setAttribute('r', '10');
            circleProgress.style.strokeDasharray = '63';
            circleProgress.style.strokeDashoffset = '63';
            svg.appendChild(circleProgress);
            elapsedGroup.appendChild(svg);
            row.appendChild(elapsedGroup);

            const timerText = document.createElement('div');
            timerText.className = 'timer-text';
            timerText.dataset.resets = resetsAt || '';
            timerText.dataset.total = totalMinutes;
            timerText.textContent = '--:--';
            row.appendChild(timerText);

            const resetsText = document.createElement('span');
            resetsText.className = 'resets-at-text';
            if (resetsAt) {
                const settings = window._cachedSettings || {};
                resetsText.textContent = formatResetsAt(resetsAt, true, settings.timeFormat || '12h', settings.weeklyDateFormat || 'date');
            }
            row.appendChild(resetsText);
        }

        attachHideBtn(row, key, config.label);

        // Route rows to their provider section. Anthropic scoped limits
        // (e.g. Fable) are pinned below the Weekly row; CLI + extra-usage rows
        // stay in Anthropic's expandable panel and count toward its tally;
        // Codex and Gemini rows go to the OpenAI / Google sections.
        if (key.startsWith('seven_day_scoped_')) {
            const slug = key.slice('seven_day_scoped_'.length);
            const forecastAt = data.forecasts?.scoped?.[slug];
            if (forecastAt) {
                const settings = window._cachedSettings || {};
                row.title = `At the current pace, 100% by ${formatResetsAt(forecastAt, true, settings.timeFormat || '12h', 'date-day-time')}`;
            }
            elements.scopedRows.appendChild(row);
        } else if (key.startsWith('cc_')) {
            // Second-account rows live under their own burnable "CLI"
            // subheading; the subheading names the group, so drop the
            // now-redundant "CLI " label prefix
            label.textContent = config.label.replace(/^CLI /, '');
            elements.anthropicCliRows.appendChild(row);
        } else if (key.startsWith('codex_cli_')) {
            label.textContent = config.label.replace(/^CLI /, '');
            elements.openaiCliRows.appendChild(row);
        } else if (key.startsWith('codex_')) {
            elements.openaiRows.appendChild(row);
        } else if (key.startsWith('gemini_cli_')) {
            label.textContent = config.label.replace(/^CLI /, '');
            elements.googleCliRows.appendChild(row);
            const cliShade = GEMINI_BLUES[(elements.googleCliRows.children.length - 1) % GEMINI_BLUES.length];
            const cliFill = row.querySelector('.progress-fill');
            if (cliFill) {
                cliFill.style.background = cliShade;
                if (cliFill.classList.contains('on-fire')) cliFill.style.setProperty('--fire-col', cliShade);
            }
            label.style.setProperty('--row-col', cliShade);
        } else if (key.startsWith('gemini_')) {
            elements.googleRows.appendChild(row);
            const shade = GEMINI_BLUES[(elements.googleRows.children.length - 1) % GEMINI_BLUES.length];
            const fill = row.querySelector('.progress-fill');
            if (fill) {
                fill.style.background = shade;
                if (fill.classList.contains('on-fire')) fill.style.setProperty('--fire-col', shade);
            }
            label.style.setProperty('--row-col', shade);
        } else {
            elements.extraRows.appendChild(row);
            count++;
        }
    }

    // Credits and banked weekly-limit resets are account-scoped. Render the
    // desktop and CLI values independently instead of silently dropping the
    // secondary account's fields from the same live payload.
    appendCodexCreditsRow(data.codex, 'codex_row_credits', elements.openaiExtraRows, hiddenRows);
    appendCodexResetsRow(data.codex, 'codex_row_resets', elements.openaiExtraRows, hiddenRows);
    appendCodexCreditsRow(data.codex?.cli, 'codex_cli_row_credits', elements.openaiCliRows, hiddenRows);
    appendCodexResetsRow(data.codex?.cli, 'codex_cli_row_resets', elements.openaiCliRows, hiddenRows);

    // Provider sections appear only when they have rows
    elements.sectionOpenai.style.display = '';
    elements.sectionGoogle.style.display = '';
    elements.openaiExpandToggle.style.display = elements.openaiExtraRows.children.length ? 'flex' : 'none';

    // Hide toggle if no extra rows
    elements.expandToggle.style.display = count > 0 ? 'flex' : 'none';
    if (count === 0 && isExpanded) {
        isExpanded = false;
        elements.expandArrow.classList.remove('expanded');
        elements.expandSection.style.display = 'none';
    }

    _sortRowsByUsage();
    applySubgroups();
    applyLabelMode();
    updateHiddenChips();

    return count;
}

function _intrinsicMainContentHeight() {
    const content = elements.mainContent;
    const contentRect = content.getBoundingClientRect();
    const style = getComputedStyle(content);
    let bottom = parseFloat(style.paddingTop) || 0;

    for (const child of content.children) {
        const childStyle = getComputedStyle(child);
        if (childStyle.display === 'none' || childStyle.position === 'absolute') continue;
        bottom = Math.max(bottom, child.getBoundingClientRect().bottom - contentRect.top);
    }

    return Math.ceil(bottom + (parseFloat(style.paddingBottom) || 0));
}

function _graphIsInline() {
    return graphVisible && !graphDetached && !isCompactMode;
}

function syncGraphLayoutState() {
    document.body.classList.toggle('graph-off', !_graphIsInline());
}

function _fitPresetHeight() {
    if (_activePreset === null) return;
    // Wait for Electron's preset bounds and the resulting CSS reflow before
    // measuring the provider rows. A preset switch (wide→tall) triggers a
    // large reflow that can outlast one timer, so the fit runs in several
    // idempotent passes — each re-checks state in case the chart was enabled
    // mid-transition. (One 120ms pass used to measure mid-reflow and leave
    // the reserved-graph void the tall preset was reported with.)
    //
    // This used to bail out entirely whenever the graph was inline, on the
    // assumption the chart would take up the slack. It can't: .graph-section
    // is a fixed 220px, so the preset's flat 1150px height left everything
    // below the chart as dead space.
    //
    // ALWAYS measure intrinsically, graph or no graph. #graphSection is a
    // child of #mainContent, so _intrinsicMainContentHeight() already counts
    // its band. scrollHeight cannot be used here: .content is flex-stretched
    // with overflow-y:auto, so when the content is SHORTER than the window
    // scrollHeight just reports the stretched box (~the preset height) and
    // the fit computes a target equal to what it already is — never shrinking.
    for (const delay of [150, 450, 900]) {
        setTimeout(() => {
            if (_activePreset === null) return;
            syncGraphLayoutState();
            _forceFitHeight({ fitPreset: true, intrinsic: true });
        }, delay);
    }
}

// Wide preset with the graph shown: the fixed 600px preset can't fit the
// three columns AND the whole chart, so the chart clips at the bottom. Size
// the window to the columns' natural height plus a moderate graph band — the
// full chart shows at a comfortable size without an oversized window. The
// band is a constant (not the live graph height, which fills 1fr and would
// feed back on itself).
const WIDE_GRAPH_BAND = 210;
function _fitWidePresetWithGraph() {
    if (_activePreset !== 'wide' || !_graphIsInline()) return;
    for (const delay of [200, 520]) {
        setTimeout(() => {
            if (_activePreset !== 'wide' || !_graphIsInline()) return;
            const mcTop = elements.mainContent.getBoundingClientRect().top;
            let colBottom = 0;
            for (const id of ['sectionAnthropic', 'sectionOpenai', 'sectionGoogle']) {
                const s = document.getElementById(id);
                if (s && getComputedStyle(s).display !== 'none') {
                    colBottom = Math.max(colBottom, s.getBoundingClientRect().bottom - mcTop);
                }
            }
            if (colBottom < 40) return;
            const target = _chromeHeight() + Math.ceil(colBottom) + WIDE_GRAPH_BAND + 12;
            window.electronAPI.resizeWindow(target, true, true);
        }, delay);
    }
}

// Combined height of any visible notice banners (update + stale-session)
function _bannersHeight() {
    let h = 0;
    if (elements.updateBanner && elements.updateBanner.style.display !== 'none') {
        h += elements.updateBanner.offsetHeight || 28;
    }
    const stale = document.getElementById('staleBanner');
    if (stale && stale.style.display !== 'none') h += stale.offsetHeight || 24;
    return h;
}

// Fit the window height to the content, keeping the user's width — used by
// toggles that change content height (graph, subgroup rolls) so the window
// grows and contracts with what it shows. A preset fit is a one-time explicit
// transition; the preset continues to own geometry after it completes.
// userAction marks a direct click (graph toggle, burn, hide): those may adopt
// the new content height even in a hand-sized window; background refits never.
function _forceFitHeight({ fitPreset = false, intrinsic = false, userAction = false } = {}) {
    if (isCompactMode) return;
    if (_windowUserSized && !fitPreset && !userAction) return;
    if (elements.settingsOverlay.style.display !== 'none') return;
    requestAnimationFrame(() => {
        const th = _chromeHeight();
        const ch = intrinsic ? _intrinsicMainContentHeight() : elements.mainContent.scrollHeight;
        const bh = _bannersHeight();
        if (th >= 10 && ch >= 40) {
            const target = th + bh + ch + 10;
            // Asymmetric fixed-point guard: GROWTH is always honoured (a
            // skipped grow leaves real overflow — an exposed scrollbar);
            // only shrinks inside the tolerance are ignored, which is what
            // keeps the old resize feedback loop dead.
            const delta = target - window.innerHeight;
            if (delta > 0 ? delta <= 2 : delta >= -12) return;
            window.electronAPI.resizeWindow(target, true, fitPreset, userAction);
        }
    });
}

// Title bar + bottom toolbar heights — every window-height computation
// includes both chrome strips
function _chromeHeight() {
    const th = elements.titleBar ? elements.titleBar.offsetHeight : 0;
    const bar = document.getElementById('bottomBar');
    return th + (bar ? bar.offsetHeight : 0);
}

// ---- Row identity codes + colours ----
// At narrow widths the row labels compress to short colour-matched codes;
// compact mode uses the same codes. Colour follows the row's bar colour.
const CODE_COLORS = {
    weekly: '#d97757', fable: '#d946ef', codex: '#2dd4bf', gemini: '#4285f4',
    cc: '#c8846a', opus: '#f59e0b', sonnet: '#f59e0b', cowork: '#22c55e',
    design: '#ec4899', extra: '#8b5cf6', oauth_apps: '#22c55e'
};
const COMPANY_COLORS = { anthropic: '#d97757', openai: '#10a37f', google: '#4285f4' };
// Google meters each model version separately — each pool gets its own
// shade of Google blue, darkest first
const GEMINI_BLUES = ['#1a5ce8', '#4285f4', '#7baaf7', '#a8c7fa', '#c9dcfc'];

// Tooltip text for abbreviated window suffixes (shown only when abbreviated)
function windowTip(label) {
    if (/daily/i.test(label) || /\(1D\)/i.test(label)) return 'Daily limit — resets every day';
    if (/\(7d\)/i.test(label)) return '7-day limit — resets weekly';
    if (/\(5h\)/i.test(label)) return '5-hour session limit';
    return '';
}

// Swap row-label verbosity by width band: full names, abbreviated windows
// ("(daily)" -> "(1D)"), or colour-coded chips. Tooltips carry the meaning
// only while abbreviated.
function applyLabelMode(effWidth) {
    // When called without an explicit width (e.g. from a data refresh) read the
    // band classes applySqueezeClasses already set — they account for landscape
    // column width, which window.innerWidth does not.
    let mode;
    if (effWidth != null) mode = effWidth <= 450 ? 'code' : effWidth <= 540 ? 'abbr' : 'full';
    else if (document.body.classList.contains('lbl-code')) mode = 'code';
    else if (document.body.classList.contains('lbl-abbr')) mode = 'abbr';
    else mode = 'full';
    document.querySelectorAll('.usage-label').forEach((el) => {
        const tip = el.dataset.tip || '';
        const full = (el.dataset.abbr || el.textContent || '').replace(/\(1D\)/, '(daily)');
        if (mode === 'code') el.title = tip ? `${full} — ${tip}` : full;
        else if (mode === 'abbr') el.title = tip;
        // Full mode: no tooltip unless the name is ellipsis-truncated, in
        // which case the hover carries the complete label.
        else el.title = (el.scrollWidth > el.clientWidth + 1) ? full : '';
    });
}

function rowCode(key, label) {
    const clean = String(label || '').replace(/^CLI /, '');
    if (key === 'cc_five_hour') return 'CLA 5H';
    if (key === 'cc_seven_day') return 'CLA 7D';
    if (/^(cc_)?seven_day_scoped_/.test(key)) return clean.slice(0, 3).toUpperCase();
    if (key === 'extra_usage') return 'EXT';
    if (key.startsWith('codex_')) {
        // OpenAI now runs several Codex windows at once (5h + 7d); a bare
        // CDX for all of them read as the same row repeated three times.
        if (/^Codex/i.test(clean)) {
            const win = (clean.match(/\(([^)]+)\)/) || [])[1];
            return win ? 'CDX ' + win.toUpperCase() : 'CDX';
        }
        if (/Spark/i.test(clean)) return 'SPK';
        if (/Code Review/i.test(clean)) return 'CRV';
        return clean.replace(/\s*\(.*\)$/, '').slice(0, 3).toUpperCase();
    }
    if (key.startsWith('gemini_')) {
        // "2.5 Flash Lite (daily)" -> "2.5FL"
        const base = clean.replace(/\s*\(.*\)$/, '').replace(/^Gemini\s*/i, '');
        const ver = (base.match(/^[\d.]+/) || [''])[0];
        const fam = base.replace(/^[\d.]+\s*/, '').split(/\s+/).map((w) => w[0] || '').join('').toUpperCase();
        return (ver + fam) || 'GEM';
    }
    return clean.replace(/\s*\(.*\)$/, '').slice(0, 3).toUpperCase();
}

// ---- Hide individual trackers ----
// Hover any pool row for a small minus; hiding persists in
// settings.hiddenRows. Each section footer shows an "N hidden" chip that
// restores that provider's rows.
function rowProvider(key) {
    if (key.startsWith('codex_')) return 'openai';
    if (key.startsWith('gemini_')) return 'google';
    return 'anthropic';
}

function hiddenRowsMap() {
    return (window._cachedSettings || {}).hiddenRows || {};
}

// Optional rank-by-use (settings.sortByUsage, default off): reorder each
// container's pool rows heaviest-first. Summary rows (credits / limit resets
// / extra-usage) keep the bottom; Anthropic's Session/Weekly are structural
// rows outside these containers and stay pinned by design.
const _SUMMARY_ROW_KEYS = /^(extra_usage|codex(_cli)?_row_(credits|resets))$/;
function _sortRowsByUsage() {
    if (!(window._cachedSettings || {}).sortByUsage) return;
    for (const container of [elements.scopedRows, elements.extraRows, elements.openaiRows,
        elements.googleRows, elements.anthropicCliRows, elements.openaiCliRows, elements.googleCliRows]) {
        if (!container || container.children.length < 2) continue;
        const rows = [...container.children].filter((el) => el.dataset && el.dataset.rowKey);
        if (rows.length < 2) continue;
        const pct = (row) => {
            if (_SUMMARY_ROW_KEYS.test(row.dataset.rowKey)) return -1;
            const value = latestUsageData?.[row.dataset.rowKey]?.utilization;
            return Number.isFinite(value) ? value : -1;
        };
        rows.sort((a, b) => pct(b) - pct(a));
        for (const row of rows) container.appendChild(row);
    }
}

function renderAccountEmails(data) {
    const hidden = (window._cachedSettings || {}).hideAccountEmails === true;
    const pick = {
        Anthropic: data.anthropic_email || null,
        Openai: (data.codex && (data.codex.email || (data.codex.cli && data.codex.cli.email))) || null,
        Google: (data.gemini && (data.gemini.email || (data.gemini.cli && data.gemini.cli.email))) || null
    };
    for (const prov of ['Anthropic', 'Openai', 'Google']) {
        const el = document.getElementById('email' + prov);
        if (!el) continue;
        const email = pick[prov];
        const txt = el.querySelector('.account-email-text');
        if (email && !hidden) {
            if (txt) txt.textContent = email;
            el.style.display = '';
        } else {
            el.style.display = 'none';
        }
    }
}

// ---- Provider permahide + CLI-account offers -------------------------------
const PERMAHIDE_SECTIONS = {
    anthropic: 'sectionAnthropic', openai: 'sectionOpenai', google: 'sectionGoogle'
};
const PERMAHIDE_TITLECASE = { anthropic: 'Anthropic', openai: 'Openai', google: 'Google' };

// Permahidden providers vanish entirely (header included) until the Settings
// Restore button brings them back. Display-only: fetching and history are
// untouched, exactly like the roll-up.
function applyProviderVisibility() {
    const hidden = (window._cachedSettings || {}).hiddenProviders || {};
    let visible = 0;
    for (const [prov, secId] of Object.entries(PERMAHIDE_SECTIONS)) {
        const sec = document.getElementById(secId);
        if (!sec) continue;
        const hide = hidden[prov] === true;
        sec.style.display = hide ? 'none' : '';
        if (!hide) visible++;
    }
    // Landscape lays providers out as grid columns — tell it how many remain.
    elements.mainContent.style.setProperty('--vis-providers', Math.max(visible, 1));
    // Settings buttons flip between Hide and Restore.
    for (const prov of Object.keys(PERMAHIDE_SECTIONS)) {
        const btn = document.getElementById('providerHide' + PERMAHIDE_TITLECASE[prov]);
        if (!btn) continue;
        const isHidden = hidden[prov] === true;
        btn.textContent = isHidden ? 'Restore provider' : '\u{1F9E8} Hide provider';
        btn.classList.toggle('restore', isHidden);
    }
}

async function setProviderHidden(prov, hide) {
    const hiddenProviders = { ...((window._cachedSettings || {}).hiddenProviders || {}) };
    hiddenProviders[prov] = hide === true;
    await _saveSettingsPatch({ hiddenProviders });
    applyProviderVisibility();
    if (typeof requestWindowFit === 'function') requestWindowFit();
}

// ---- Shockwave & Ash: the chosen detonation ----
// White flash, an expanding shockwave ring from the dynamite, the content
// charring and lifting away as embers — then the section collapses and the
// permahide lands. With pizazz off it is a plain hide.
function detonateProvider(prov) {
    const sec = document.getElementById(PERMAHIDE_SECTIONS[prov]);
    if (!sec) return;
    if (document.body.classList.contains('no-pizazz')) {
        setProviderHidden(prov, true);
        return;
    }
    const tnt = document.getElementById('tnt' + PERMAHIDE_TITLECASE[prov]);
    sec.classList.add('detonating');

    // origin: the dynamite button, in section coordinates
    const sr = sec.getBoundingClientRect();
    const tr = tnt ? tnt.getBoundingClientRect() : sr;
    const origin = { x: tr.left - sr.left + 9, y: tr.top - sr.top + 9 };

    // flash
    const flash = document.createElement('div');
    flash.className = 'detonate-flash';
    sec.appendChild(flash);
    requestAnimationFrame(() => requestAnimationFrame(() => { flash.style.opacity = '0'; }));

    // canvas: ring + ash + embers
    const PAD = 120;
    const canvas = document.createElement('canvas');
    canvas.className = 'detonate-fx';
    sec.appendChild(canvas);
    canvas.width = (sr.width + PAD * 2) * devicePixelRatio;
    canvas.height = (sr.height + PAD * 2) * devicePixelRatio;
    const ctx = canvas.getContext('2d');
    ctx.scale(devicePixelRatio, devicePixelRatio);

    const parts = [{ ring: true, x: PAD + origin.x, y: PAD + origin.y }];
    const count = Math.min(320, Math.round(sr.width * sr.height / 900));
    for (let i = 0; i < count; i++) {
        parts.push({
            x: PAD + Math.random() * sr.width,
            y: PAD + Math.random() * sr.height,
            vx: (Math.random() - 0.5) * 55,
            vy: -(40 + Math.random() * 130),
            sz: 1.5 + Math.random() * 3,
            ember: Math.random() < 0.3,
            delay: 0.1 + Math.random() * 0.45,
            life: 0.9 + Math.random()
        });
    }
    const t0 = performance.now();
    (function tick(now) {
        const t = (now - t0) / 1000;
        ctx.clearRect(0, 0, sr.width + PAD * 2, sr.height + PAD * 2);
        let alive = false;
        for (const p of parts) {
            if (p.ring) {
                if (t > 0.55) continue;
                alive = true;
                const rad = t * 620;
                const a = Math.max(0, 1 - t / 0.55);
                ctx.save();
                ctx.globalAlpha = a * 0.85;
                ctx.strokeStyle = '#e8f6ff';
                ctx.lineWidth = 3 + (1 - a) * 6;
                ctx.shadowColor = '#7fd8ff';
                ctx.shadowBlur = 18;
                ctx.beginPath(); ctx.arc(p.x, p.y, rad, 0, 7); ctx.stroke();
                ctx.restore();
                continue;
            }
            const tt = t - p.delay;
            if (tt < 0) { alive = true; continue; }
            if (tt > p.life) continue;
            alive = true;
            const a = Math.max(0, 1 - tt / p.life);
            const x = p.x + p.vx * tt + Math.sin((p.y + tt * 7) * 0.7) * 6 * tt;
            const y = p.y + p.vy * tt;
            ctx.globalAlpha = a * (p.ember ? 0.95 : 0.6);
            ctx.fillStyle = p.ember ? (tt < p.life * 0.4 ? '#ffb454' : '#d24b3f') : '#9aa0b4';
            ctx.fillRect(x, y, p.sz, p.sz);
            ctx.globalAlpha = 1;
        }
        if (alive && now - t0 < 2000) requestAnimationFrame(tick);
        else { canvas.remove(); flash.remove(); }
    })(t0);

    // shake the widget, char the section, collapse it, then commit the hide
    document.body.classList.add('detonate-shake');
    setTimeout(() => document.body.classList.remove('detonate-shake'), 520);
    sec.style.transition = 'filter 0.4s ease';
    sec.style.filter = 'brightness(0.35) saturate(0.4)';
    setTimeout(() => {
        const h = sec.offsetHeight;
        sec.style.height = h + 'px';
        sec.style.overflow = 'hidden';
        sec.style.transition = 'height 0.6s cubic-bezier(.5,0,.7,1), opacity 0.6s ease';
        requestAnimationFrame(() => { sec.style.height = '0px'; sec.style.opacity = '0'; });
        setTimeout(async () => {
            await setProviderHidden(prov, true);
            sec.classList.remove('detonating');
            // reset inline styles so a Settings restore comes back clean
            sec.style.height = sec.style.opacity = sec.style.filter =
                sec.style.overflow = sec.style.transition = '';
        }, 640);
    }, 320);
}

// Detected CLI logins the user has not adopted: a pulsing chip in the header.
// Clicking it pulls that account into the stats (the first empty slot — CLI
// data lands wherever the pipeline already puts it: primary if nothing else
// is signed in, second account otherwise).
function renderAccountOffers(data) {
    const offers = data.offers || {};
    for (const prov of Object.keys(PERMAHIDE_SECTIONS)) {
        const P = PERMAHIDE_TITLECASE[prov];
        const emailEl = document.getElementById('email' + P);
        if (!emailEl) continue;
        let chip = document.getElementById('offer' + P);
        const offer = offers[prov];
        if (!offer) { if (chip) chip.remove(); continue; }
        if (!chip) {
            chip = document.createElement('button');
            chip.id = 'offer' + P;
            chip.className = 'account-offer';
            chip.addEventListener('click', async (e) => {
                e.stopPropagation();
                chip.disabled = true;
                chip.textContent = 'Pulling\u2026';
                await window.electronAPI.setCliAdopted(prov, true);
                await fetchUsageData({ forceProviders: true, refreshLocalCredentials: true });
            });
            emailEl.insertAdjacentElement('afterend', chip);
        }
        const who = offer.email || offer.label || 'CLI login';
        chip.textContent = '\u26A1 ' + who + ' \u2014 click to track';
        chip.title = 'Found a ' + (offer.label || 'CLI login') + ' on this machine. '
            + 'Click to pull its usage into the widget \u2014 nothing is read until you do.';
    }
}

async function hideRow(key) {
    const hiddenRows = { ...hiddenRowsMap(), [key]: true };
    await _saveSettingsPatch({ hiddenRows });
    if (latestUsageData) updateUI(latestUsageData);
}

async function restoreRows(provider) {
    const hiddenRows = { ...hiddenRowsMap() };
    for (const k of Object.keys(hiddenRows)) {
        if (rowProvider(k) === provider) delete hiddenRows[k];
    }
    await _saveSettingsPatch({ hiddenRows });
    if (latestUsageData) updateUI(latestUsageData);
}

function attachHideBtn(row, key, label) {
    row.dataset.rowKey = key;
    // Rows being restored this render start collapsed so the window measures
    // small; revealMarkedRows() then grows them back in with sparkles.
    if (_revealRowKeys && _revealRowKeys.has(key)) row.classList.add('reveal-init');
    const btn = document.createElement('button');
    btn.className = 'row-hide-btn';
    btn.textContent = '–';
    btn.title = 'Hide "' + String(label || key).replace(/^CLI /, '') + '" — restore from the "hidden" chip at the section bottom';
    btn.addEventListener('click', (e) => { e.stopPropagation(); burnHideRow(row, key); });
    row.appendChild(btn);
}

function updateHiddenChips() {
    const hidden = Object.keys(hiddenRowsMap());
    for (const [provider, footerSel] of [
        ['anthropic', '#sectionAnthropic .section-footer'],
        ['openai', '#sectionOpenai .section-footer'],
        ['google', '#sectionGoogle .section-footer']
    ]) {
        const footer = document.querySelector(footerSel);
        if (!footer) continue;
        let chip = footer.querySelector('.hidden-rows-chip');
        // Only count hidden rows that actually exist this refresh, so the chip
        // never lingers for a pool the API stopped reporting.
        const count = hidden.filter((k) => rowProvider(k) === provider && _availableRowKeys.has(k)).length;
        if (!count) { if (chip) chip.remove(); continue; }
        if (!chip) {
            chip = document.createElement('button');
            chip.className = 'hidden-rows-chip';
            chip.addEventListener('click', () => restoreRowsSparkle(provider));
            footer.appendChild(chip);
        }
        chip.textContent = count + ' hidden';
        chip.title = 'Click to restore the ' + count + ' hidden tracker' + (count > 1 ? 's' : '') + ' in this section';
    }
}

// ---- Row hide/show choreography ----
// Hiding a row incinerates it with the pixel-fire sweep (leaving no ash) and
// then smoothly collapses the gap while the window shrinks to follow. Showing
// again is NOT the burn reversed: gold sparkles rain over the returning rows
// as they fade in and the window grows to fit them at the same time.
let _revealRowKeys = null;

async function burnHideRow(row, key) {
    if (row.dataset.animating) return;
    // Respect the pizazz kill-switch / compact mode: just hide instantly.
    if (document.body.classList.contains('no-pizazz') || isCompactMode) { return hideRow(key); }
    row.dataset.animating = '1';
    // A row is far wider than a subheading — stretch the sweep so the fire
    // front doesn't race across, but cap it so it never drags.
    const SWEEP = Math.round(Math.max(440, Math.min(760, row.offsetWidth * 1.5)));
    row.classList.add('row-burning');       // CSS fades the row's content (not the fire) to ash
    runPixelSweep(row, true, SWEEP, 900);    // shorter soot-and-sparks aftermath than a subheading
    const COLLAPSE = 340;
    setTimeout(() => {
        const h = row.offsetHeight;
        row.style.height = h + 'px';
        row.style.overflow = 'hidden';
        void row.offsetWidth;
        row.style.transition = `height ${COLLAPSE}ms cubic-bezier(0.4, 0, 0.2, 1), margin ${COLLAPSE}ms cubic-bezier(0.4, 0, 0.2, 1), opacity ${COLLAPSE}ms ease`;
        requestAnimationFrame(() => {
            row.style.height = '0px';
            row.style.marginTop = '0';
            row.style.marginBottom = '0';
            row.style.opacity = '0';
        });
        _followResize(COLLAPSE + 220);            // window shrinks in step with the gap closing
        setTimeout(() => { hideRow(key); }, COLLAPSE + 120); // persist + re-render without the row (no residue)
    }, Math.round(SWEEP * 0.72));
}

async function restoreRowsSparkle(provider) {
    if (document.body.classList.contains('no-pizazz') || isCompactMode) { return restoreRows(provider); }
    const map = hiddenRowsMap();
    _revealRowKeys = new Set(Object.keys(map).filter((k) => rowProvider(k) === provider && _availableRowKeys.has(k)));
    // revealMarkedRows() clears _revealRowKeys on its first line, so run it in
    // finally: if the settings save rejects we must NOT leave the flag set, or
    // every later render would re-collapse those rows (reveal-init) forever.
    try {
        await restoreRows(provider);   // re-renders; rows in _revealRowKeys get the reveal-init class (collapsed)
    } finally {
        revealMarkedRows();
    }
}

function revealMarkedRows() {
    _revealRowKeys = null;
    const rows = [...document.querySelectorAll('.usage-section.reveal-init')];
    if (!rows.length) return;
    rows.forEach((r) => {
        r.classList.remove('reveal-init');
        const h = r.offsetHeight;                 // natural height once un-collapsed
        r.style.height = '0px';
        r.style.overflow = 'hidden';
        r.style.opacity = '0';
        r.style.marginTop = '0';
        r.style.marginBottom = '0';
        void r.offsetWidth;
        // Slight overshoot on the way open — the row lands with a soft bounce
        r.style.transition = 'height .5s cubic-bezier(0.34, 1.26, 0.44, 1), opacity .5s ease .12s, margin .5s ease';
        requestAnimationFrame(() => {
            r.style.height = h + 'px';
            r.style.opacity = '1';
            r.style.marginTop = '';
            r.style.marginBottom = '';
        });
        sparkleRow(r);
        setTimeout(() => {
            r.style.height = ''; r.style.overflow = ''; r.style.transition = ''; r.style.opacity = '';
        }, 720);
    });
    _followResize(780);   // window grows in step with the rows fading in
}

// Gold sparkle sprinkle scattered across a whole row (reuses the reset-ring
// sparkleFly keyframe) for the magic-sparkle un-hide.
function sparkleRow(row) {
    if (document.body.classList.contains('no-pizazz')) return;
    if (document.visibilityState !== 'visible') return;
    const W = row.offsetWidth || 200, H = row.offsetHeight || 28;
    const burst = document.createElement('div');
    burst.className = 'sparkle-burst';
    const glyphs = ['✦', '✧', '✨', '⋆', '✦'];
    const colors = ['#ffd700', '#fff3b0', '#ffe066', '#ffffff', '#ffec99'];
    const count = Math.max(10, Math.min(26, Math.round(W / 16)));
    for (let i = 0; i < count; i++) {
        const s = document.createElement('span');
        s.className = 'sparkle';
        s.textContent = glyphs[i % glyphs.length];
        const sx = (Math.random() - 0.5) * (W - 20);
        const sy = (Math.random() - 0.5) * (H + 4);
        s.style.setProperty('--sx', sx.toFixed(1) + 'px');
        s.style.setProperty('--sy', sy.toFixed(1) + 'px');
        s.style.setProperty('--dx', (sx + (Math.random() - 0.5) * 12).toFixed(1) + 'px');
        s.style.setProperty('--dy', (sy - 6 - Math.random() * 12).toFixed(1) + 'px');
        s.style.color = colors[i % colors.length];
        s.style.animationDelay = (Math.random() * 0.5).toFixed(2) + 's';
        s.style.fontSize = Math.round(6 + Math.random() * 7) + 'px';
        burst.appendChild(s);
    }
    row.appendChild(burst);
    // A golden sheen crosses the row once beneath the sparkles
    const sheen = document.createElement('div');
    sheen.className = 'row-sheen';
    row.appendChild(sheen);
    setTimeout(() => { burst.remove(); sheen.remove(); }, 1900);
}

// ---- Desktop / CLI subgroups with burnable subheadings ----
// In dual mode (CLI logged into a different account than the primary sign-in)
// each provider splits into "Desktop" and "CLI" subgroups. Clicking a
// subheading ignites a fast pixel-fire sweep across it right-to-left — a hot
// front with a lit trail behind it and smoke rising — charring the letters
// and rolling the rows up; clicking again reverses the burn.
const SUBGROUP_SWEEP_MS = 380;

const _PXCOLS = { deep: '#e8590c', mid: '#ff922b', bright: '#ffd43b', core: '#fff3bf' };
const _rnd = (a, b) => a + Math.random() * (b - a);
const _pick = (arr) => arr[Math.floor(Math.random() * arr.length)];

// Derive a 4-tone flame palette (deep/mid/bright/core) from any base colour —
// this is how the burn-alert fire wears each bar's own colour.
const _paletteCache = new Map();
// Accept either form a caller might hand us. An inline style read back as
// `el.style.background` comes out as "rgb(r, g, b)", and silently falling back
// to the default orange made every compact flame the wrong colour.
function _toHex(col) {
    const s = String(col || '').trim();
    if (/^#?[0-9a-f]{6}$/i.test(s)) return s[0] === '#' ? s : '#' + s;
    const m = s.match(/^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i);
    if (m) return '#' + [1, 2, 3].map((i) => Number(m[i]).toString(16).padStart(2, '0')).join('');
    return null;
}
function _mixHex(hex, target, f) {
    const p = (h) => [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
    const m = _toHex(hex) || '#ff922b';
    const [r1, g1, b1] = p(m);
    const [r2, g2, b2] = p(target);
    const c = (a, b2v) => Math.round(a + (b2v - a) * f).toString(16).padStart(2, '0');
    return '#' + c(r1, r2) + c(g1, g2) + c(b1, b2);
}
function _firePalette(base) {
    const key = _toHex(base) || '#ff922b';
    if (!_paletteCache.has(key)) {
        _paletteCache.set(key, {
            deep: _mixHex(key, '#000000', 0.3),
            mid: key,
            bright: _mixHex(key, '#ffffff', 0.45),
            core: _mixHex(key, '#ffffff', 0.78)
        });
    }
    return _paletteCache.get(key);
}

// Chunky flame sprite out of 2px cells (box-shadow art, robot-logo palette).
// `tall` spawns an occasional taller lick near the front for variety;
// `palette` recolours the whole sprite (burn-alert fire wears the bar's hue).
function _spawnPixelFlame(layer, x, lifeMs, tall, palette) {
    if (layer.childElementCount > 110) return; // bounded — never floods the DOM
    const P = palette || _PXCOLS;
    const cells = [];
    // 1px cells over a 5-wide grid: the same flame size as the old 2px/3-wide
    // sprite but at twice the detail, so the tongue can actually taper and
    // flicker at its edges instead of stepping in chunky 2px blocks.
    const CELL = 1;
    const rows = Math.floor(tall ? _rnd(15, 23) : _rnd(9, 15));
    for (let r = 0; r < rows; r++) {
        const frac = r / rows;                    // 0 at the base, 1 at the tip
        for (const c of [-2, -1, 0, 1, 2]) {
            const a = Math.abs(c);
            // Narrow toward the tip so it comes to a point rather than a slab
            if (a === 2 && frac > 0.46) continue;
            if (a === 1 && frac > 0.84) continue;
            if (r === rows - 1 && a > 0) continue;
            if (a === 2 && Math.random() < 0.34) continue;
            if (a === 1 && Math.random() < 0.16) continue;
            if (r === rows - 1 && Math.random() < 0.35) continue;
            const col = frac > 0.74 ? (Math.random() < 0.5 ? P.mid : P.bright)
                : a === 2 ? (Math.random() < 0.6 ? P.deep : P.mid)
                    : a === 1 ? (Math.random() < 0.5 ? P.deep : P.mid)
                        : (Math.random() < 0.45 ? P.core : P.bright);
            cells.push(`${c * CELL}px ${-r * CELL}px 0 0 ${col}`);
        }
    }
    const f = document.createElement('div');
    f.className = 'fp';
    const life = Math.round(lifeMs);
    f.style.cssText = `left:${x}px; bottom:1px; width:${CELL}px; height:${CELL}px; background:transparent;
        box-shadow:${cells.join(',')};
        animation: pxFlame ${life}ms steps(10) forwards;`;
    layer.appendChild(f);
    // Sprites animate fill:forwards, so nothing retires them on their own.
    // A layer that never gets torn down — the reset orbs burn permanently —
    // fills to the cap above and spawning stops, freezing the flame mid-lick.
    setTimeout(() => f.remove(), life + 80);
}

function _spawnPixelSmoke(layer, x) {
    if (layer.childElementCount > 110) return;
    const s = document.createElement('div');
    s.className = 'fp';
    const life = Math.round(_rnd(650, 1100));
    s.style.cssText = `left:${x}px; bottom:${_rnd(8, 13)}px; width:3px; height:3px;
        background:${_pick(['#6a6a78', '#7c7c8a', '#585866'])};
        --dx:${Math.round(_rnd(-2, 3)) * 3}px; --ry:${-Math.round(_rnd(5, 11)) * 3}px;
        animation: pxSmoke ${life}ms steps(6) forwards;`;
    layer.appendChild(s);
    setTimeout(() => s.remove(), life + 80);
}

// Style 2 ("particle inferno") building block: micro fire-motes born at the
// fill that rise fast, drift, and gutter out — denser and more chaotic than
// the classic chunky sprites.
function _spawnFireParticle(layer, x, palette) {
    if (layer.childElementCount > 140) return;
    const p = document.createElement('div');
    p.className = 'fp';
    const size = Math.random() < 0.6 ? 2 : 1;
    const col = _pick([palette.core, palette.bright, palette.bright, palette.mid, palette.mid, palette.deep]);
    const life = Math.round(_rnd(320, 680));
    p.style.cssText = `left:${x.toFixed(1)}px; bottom:${_rnd(0, 3).toFixed(0)}px; width:${size}px; height:${size}px;
        background:${col}; box-shadow: 0 0 3px ${col};
        --dx:${_rnd(-7, 7).toFixed(0)}px; --ry:${(-_rnd(9, 22)).toFixed(0)}px;
        animation: fireRise ${life}ms ease-out forwards;`;
    layer.appendChild(p);
    setTimeout(() => p.remove(), life + 80);
}

// Charred-paper ash: tiny grey flakes tumbling DOWN off the burn line
function _spawnAshFlake(layer, x) {
    if (layer.childElementCount > 110) return;
    const a = document.createElement('div');
    a.className = 'fp';
    const life = Math.round(_rnd(700, 1250));
    a.style.cssText = `left:${x}px; bottom:${_rnd(5, 12)}px; width:2px; height:2px;
        background:${_pick(['#5c5c68', '#4a4a56', '#6e6e7a'])};
        --ax:${_rnd(-12, 12).toFixed(0)}px; --ay:${_rnd(16, 32).toFixed(0)}px; --ar:${_rnd(-160, 160).toFixed(0)}deg;
        animation: ashFall ${life}ms ease-in forwards;`;
    layer.appendChild(a);
    setTimeout(() => a.remove(), life + 80);
}

// The sweep itself: a hot front races across the word; every few px it leaves
// a trail flame that keeps burning until the front finishes, and smoke rises
// from both the front and random lit spots along the trail.
function runPixelSweep(btn, hide, sweepMs, aftermathMs) {
    if (document.body.classList.contains('no-pizazz')) return; // clown's in jail
    const SWEEP = sweepMs || SUBGROUP_SWEEP_MS; // wider targets (rows) pass a longer sweep
    const layer = document.createElement('div');
    layer.className = 'fx';
    btn.appendChild(layer);
    // The ember glow rides the front — one node, transform-only movement
    const glow = document.createElement('div');
    glow.className = 'fx-glow';
    layer.appendChild(glow);
    const W = Math.max(10, btn.offsetWidth - 14);
    const t0 = performance.now();
    let trailX = hide ? W : 0;
    let lastFront = 0, lastSmoke = 0;
    const lit = [];
    function frame(now) {
        // Tab hidden or clown jailed mid-sweep: stop cold, leave no debris
        if (document.hidden || document.body.classList.contains('no-pizazz')) {
            layer.remove();
            return;
        }
        const t = Math.min(1, (now - t0) / SWEEP);
        const x = hide ? W * (1 - t) : W * t;
        const remaining = SWEEP - (now - t0);
        glow.style.transform = `translateX(${(x - 3).toFixed(1)}px)`;
        if (now - lastFront > 22) { lastFront = now; _spawnPixelFlame(layer, x, _rnd(160, 300), Math.random() < 0.12); }
        while (hide ? trailX > x : trailX < x) {
            _spawnPixelFlame(layer, trailX + _rnd(-1, 1), remaining + _rnd(60, 220));
            lit.push(trailX);
            trailX += (hide ? -1 : 1) * 5;
        }
        if (now - lastSmoke > 45) {
            lastSmoke = now;
            const sx = (lit.length && Math.random() < 0.55) ? _pick(lit) : x + _rnd(-4, 6);
            _spawnPixelSmoke(layer, sx);
        }
        if (t < 1) requestAnimationFrame(frame);
        else if (hide) {
            glow.remove();
            // Soot & Sparks aftermath: lingering smoke, drifting ash and stray
            // sparks, all dying out on a decaying clock
            const a0 = performance.now(), ADUR = aftermathMs || 3200;
            (function aftermath() {
                const at = (performance.now() - a0) / ADUR;
                if (at >= 1 || document.hidden || document.body.classList.contains('no-pizazz')) {
                    setTimeout(() => layer.remove(), 1600);
                    return;
                }
                _spawnPixelSmoke(layer, _rnd(0, W));
                if (Math.random() < 0.45) _spawnAshFlake(layer, _rnd(0, W));
                if (Math.random() < 0.5) {
                    const s = document.createElement('div');
                    const col = _pick(['#ffd43b', '#ff922b']);
                    s.className = 'fp';
                    s.style.cssText = `left:${_rnd(0, W)}px; bottom:${_rnd(2, 10)}px; width:2px; height:2px;
                        background:${col}; box-shadow: 0 0 4px ${col};
                        --dx:${_rnd(-14, 14)}px; --ry:${-_rnd(8, 22)}px;
                        animation: bwSpark ${Math.round(_rnd(300, 600))}ms ease-out forwards;`;
                    layer.appendChild(s);
                }
                setTimeout(aftermath, 260 * (1 + at * 2.5));
            })();
        }
        else { glow.remove(); setTimeout(() => layer.remove(), 1200); }
    }
    requestAnimationFrame(frame);
}

// Brief floating confirmation when the flame style is switched by clicking a
// burning bar. position:fixed so it never disturbs layout.
function _flameStyleToast(anchor, style) {
    if (document.body.classList.contains('no-pizazz')) return;
    const rect = anchor.getBoundingClientRect();
    const toast = document.createElement('div');
    toast.className = 'flame-style-toast';
    toast.textContent = style === 'particle' ? '🔥 Inferno' : '🔥 Classic';
    toast.style.left = Math.round(rect.left + rect.width / 2) + 'px';
    toast.style.top = Math.round(rect.top - 4) + 'px';
    document.body.appendChild(toast);
    setTimeout(() => toast.remove(), 1100);
}

// ---- Ambient fire loop: burning bars + reset orbs wear LIVE pixel fire ----
// One shared scheduler drives every currently-burning element with the same
// flame engine the subheading burns use, recoloured per element. Each element
// gets its own spawn cadence; the loop idles to a slow poll when nothing
// burns, the tab is hidden, or the clown is jailed.
let _ambientFireTimer = null;
function _scheduleAmbientFire(delayMs) {
    if (_ambientFireTimer) return;
    _ambientFireTimer = setTimeout(() => {
        _ambientFireTimer = null;
        requestAnimationFrame(_ambientFireFrame);
    }, delayMs);
}

// ---- Frozen-provider ice -------------------------------------------------
// The block's chipped outline and the row of icicles are generated once from a
// fixed seed: irregular, but identical on every render and every launch rather
// than reshuffling underneath the user. Real elements, not one clip-path strip,
// because each icicle needs its own width, length, lean and opacity — and its
// own lit/shadowed gradient — to read as ice instead of as a sawtooth.
function _seededRandom(seed) {
    let a = seed;
    return function () {
        a |= 0; a = a + 0x6D2B79F5 | 0;
        let t = Math.imul(a ^ a >>> 15, 1 | a);
        t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
        return ((t ^ t >>> 14) >>> 0) / 4294967296;
    };
}

// Deep bites along the top and sides, near-flat along the bottom so the
// icicles stay attached to it.
const _CHIPPED_CLIP = (() => {
    const rnd = _seededRandom(1000);
    const steps = 7, ya = 12, xa = 2.6, by = 2;
    const pts = [];
    const px = (v) => v.toFixed(2) + '%';
    for (let i = 0; i <= steps; i++) pts.push([i / steps * 100, rnd() * ya]);
    for (let i = 1; i <= 3; i++) pts.push([100 - rnd() * xa, i / 4 * 100]);
    for (let i = steps; i >= 0; i--) pts.push([i / steps * 100, 100 - rnd() * by]);
    for (let i = 3; i >= 1; i--) pts.push([rnd() * xa, i / 4 * 100]);
    return 'polygon(' + pts.map((p) => px(p[0]) + ' ' + px(p[1])).join(', ') + ')';
})();

function _buildIcicles() {
    const wrap = document.createElement('span');
    wrap.className = 'icicles';
    const rnd = _seededRandom(20260726);
    let x = 0.4;
    while (x < 98.5) {
        const w = 2.1 + rnd() * 4.3;
        // rnd()*rnd() skews short, so a few long ones stand out among stubs
        const h = Math.min(0.22 + rnd() * rnd() * 1.75, 1.4);
        const lean = (rnd() - 0.5) * 7;
        const op = Math.min(0.36 + ((w - 2.1) / 4.3) * 0.28 + rnd() * 0.36, 1);
        const ic = document.createElement('i');
        ic.className = 'ic';
        ic.style.cssText = `left:${x.toFixed(2)}%; width:${w.toFixed(2)}px;`
            + ` height:calc(var(--ic) * ${h.toFixed(2)}); opacity:${op.toFixed(2)};`
            + ` transform:rotate(${lean.toFixed(1)}deg);`;
        wrap.appendChild(ic);
        x += 1.9 + rnd() * 3.6;
    }
    return wrap;
}

function applyFrozenIce(header, isFrozen) {
    const name = header.querySelector('.section-name');
    const wrap = name && name.parentElement;
    if (!wrap || !wrap.classList.contains('ice-wrap')) return;
    const built = !!wrap.querySelector('.ice-slab');
    if (!isFrozen) {
        if (built) wrap.querySelectorAll('.icicles, .ice-slab, .ice-drop').forEach((el) => el.remove());
        return;
    }
    if (built) return;
    // Order is the whole trick: icicles first so their tops bury under the ice,
    // then the slab, then the wordmark that is already sitting there.
    wrap.insertBefore(_buildIcicles(), name);
    const slab = document.createElement('span');
    slab.className = 'ice-slab';
    slab.style.clipPath = _CHIPPED_CLIP;
    wrap.insertBefore(slab, name);
    for (const [left, delay] of [['34%', '0s'], ['68%', '2.1s']]) {
        const drop = document.createElement('span');
        drop.className = 'ice-drop';
        drop.style.left = left;
        drop.style.animationDelay = delay;
        wrap.appendChild(drop);
    }
}

function _tickElementFire(el, now, minGapMs, isOrb, particleStyle) {
    let layer = el.__fireLayer;
    if (!layer || layer.parentNode !== el) {
        layer = document.createElement('div');
        layer.className = 'fx ambient-fire';
        el.appendChild(layer);
        el.__fireLayer = layer;
        layer.__lastSpawn = 0;
    }
    if (now - layer.__lastSpawn < minGapMs * (0.7 + Math.random() * 0.8)) return;
    layer.__lastSpawn = now;
    const width = Math.max(4, el.offsetWidth - 4);
    // Bars inset their spawn range by 4px so tongues don't spill past the
    // ends; an orb is only 8px wide, so that inset drags its flame 2px LEFT
    // of centre. The sprite is symmetric about x, so aim at the true middle.
    const centre = isOrb ? el.offsetWidth / 2 : width / 2;
    const palette = _firePalette(getComputedStyle(el).getPropertyValue('--fire-col') || '#ff922b');
    if (isOrb) {
        if (particleStyle) {
            _spawnFireParticle(layer, centre + _rnd(-3, 3), palette);
            if (Math.random() < 0.5) _spawnPixelFlame(layer, centre + _rnd(-2, 2), _rnd(240, 380), Math.random() < 0.2, palette);
        } else {
            // Overlapping licks with an occasional tall one: a single tongue
            // repeating on its own reads as a glow rather than a flame.
            _spawnPixelFlame(layer, centre + _rnd(-2.5, 2.5), _rnd(300, 520), Math.random() < 0.22, palette);
            if (Math.random() < 0.45) _spawnPixelFlame(layer, centre + _rnd(-3.5, 3.5), _rnd(200, 380), false, palette);
        }
    } else if (particleStyle) {
        // Style 2: a storm of rising motes with the occasional small tongue
        const count = 1 + Math.floor(Math.random() * 3);
        for (let i = 0; i < count; i++) _spawnFireParticle(layer, _rnd(0, width), palette);
        if (Math.random() < 0.3) _spawnPixelFlame(layer, _rnd(0, width), _rnd(180, 340), false, palette);
        if (Math.random() < 0.08) _spawnPixelSmoke(layer, _rnd(0, width));
    } else {
        // Style 1: classic chunky pixel flames
        _spawnPixelFlame(layer, _rnd(0, width), _rnd(240, 460), Math.random() < 0.1, palette);
        // A second tongue most ticks: one lick at a time read as sparse
        if (Math.random() < 0.55) _spawnPixelFlame(layer, _rnd(0, width), _rnd(200, 400), false, palette);
        if (Math.random() < 0.1) _spawnPixelSmoke(layer, _rnd(0, width));
    }
}

function _ambientFireFrame(now) {
    if (document.hidden || document.body.classList.contains('no-pizazz')) {
        _scheduleAmbientFire(500);
        return;
    }
    // A maxed bar is charred: it smokes via _tickMaxedSmoke and must not
    // ALSO wear live flames, even while the burn detector's hysteresis is
    // still holding the series in its burning state. Maxed wins.
    const bars = document.querySelectorAll('.progress-fill.on-fire:not(.maxed), .compact-bar-fill.on-fire:not(.maxed)');
    const orbs = document.querySelectorAll('.reset-dot');
    const maxed = document.querySelectorAll('.progress-fill.maxed, .compact-bar-fill.maxed');
    const particleStyle = (window._cachedSettings || {}).flameStyle === 'particle';
    for (const el of bars) _tickElementFire(el, now, particleStyle ? 20 : 42, false, particleStyle);
    for (const el of orbs) _tickElementFire(el, now, particleStyle ? 55 : 88, true, particleStyle);
    for (const el of maxed) _tickMaxedSmoke(el, now);
    if (bars.length || orbs.length) {
        requestAnimationFrame(_ambientFireFrame);
    } else if (maxed.length) {
        // Smoke smoulders on a slow clock. A spent bar can sit maxed for
        // DAYS, and keeping the 60fps rAF loop alive for it burned ~25%
        // CPU on an otherwise idle widget. Sprites spawn at ~110ms+
        // intervals, so a 150ms timer loses nothing visible.
        _scheduleAmbientFire(150);
    } else {
        _scheduleAmbientFire(600);
    }
}
_scheduleAmbientFire(900); // starts idling at module load, wakes when fire exists

// ---- Vertical squeeze classes ----
// Applied ONLY while the user has hand-sized/snapped the window (main tells
// us via window-user-sized). Auto-height mode never compresses, so the
// resize loop can't react to its own squeeze and spiral the window down.
let _windowUserSized = false;
let _activePreset = null; // 'wide' | 'tall' | null — tracked synchronously for reliable toggle-back
let graphDetached = false; // true while the graph is popped out into its own window

function applySqueezeClasses() {
    const w = window.innerWidth;
    const h = window.innerHeight;
    const on = _windowUserSized;

    // Landscape: wider than tall with room for three columns — the provider
    // sections sit side by side and every width band keys on COLUMN width
    const landscape = on && w > h && w >= 760;
    if (window._lastLandscapeMin !== landscape) {
        window._lastLandscapeMin = landscape;
        if (window.electronAPI.setMinHeight) window.electronAPI.setMinHeight(landscape ? 340 : 180);
    }
    document.body.classList.toggle('landscape', landscape);
    // Leaving wide mode drops the per-column width override (collapsed-CLI fit)
    if (!landscape && elements.mainContent.style.gridTemplateColumns) {
        elements.mainContent.style.gridTemplateColumns = '';
    }
    document.body.classList.toggle('vh-short', landscape && h < 420);
    const eff = landscape ? Math.floor(w / 3) - 10 : w;
    document.body.classList.toggle('sz1', eff <= 540);
    document.body.classList.toggle('lbl-abbr', eff <= 540 && eff > 450);
    document.body.classList.toggle('lbl-code', eff <= 450);
    document.body.classList.toggle('sz3', eff <= 330);
    document.body.classList.toggle('sz4', eff <= 288);
    // The shared dual-account table is a compact landscape treatment. Once a
    // company column is wide enough for full labels, use the normal provider
    // rows again (this most visibly affects Google's desktop + CLI accounts).
    document.body.classList.toggle('dual-compact', landscape && eff < 450);

    document.body.classList.toggle('vh-1', on && !landscape && h < 520);
    document.body.classList.toggle('vh-2', on && !landscape && h < 430);
    document.body.classList.toggle('vh-3', on && !landscape && h < 340);
    // Tall mode: scale 0→1 between 820px and 1600px of height — sections
    // spread, text grows, and the wordmarks eat the extra vertical space.
    // Not in landscape, where the three columns own the layout.
    // The enlarged "big" tall mode (giant wordmarks + text) only kicks in when
    // 1–2 companies are tracked. With all three it pushes content off-screen,
    // so keep the compact size no matter how tall the window is.
    const companyCount = ['sectionAnthropic', 'sectionOpenai', 'sectionGoogle']
        .filter((id) => { const s = document.getElementById(id); return s && s.querySelector('.usage-section'); }).length;
    const tall = (on && !landscape && companyCount > 0 && companyCount < 3)
        ? Math.min(Math.max((h - 600) / 780, 0), 1) : 0;
    document.body.classList.toggle('tall', tall > 0);
    document.body.style.setProperty('--tall', tall.toFixed(3));
    applyLabelMode(eff);
}

if (window.electronAPI.onWindowUserSized) {
    window.electronAPI.onWindowUserSized((userSized) => {
        _windowUserSized = userSized;
        applySqueezeClasses();
        // Resumed auto-height (e.g. a preset "reset", or the user dragging the
        // window back near default) — clear the preset and snap to content.
        if (!userSized) {
            _activePreset = null;
            if (elements.wideBtn) elements.wideBtn.classList.remove('active');
            if (elements.tallBtn) elements.tallBtn.classList.remove('active');
            // Broadcast-driven refit: measure INTRINSIC content (children
            // bottoms), never the flex-stretched scrollHeight — this is the
            // re-entry point of the resize feedback loop.
            _forceFitHeight({ intrinsic: true });
        }
    });
}
window.addEventListener('resize', applySqueezeClasses);

// Pay out sparkle bursts owed from resets that happened while hidden
document.addEventListener('visibilitychange', () => {
    if (document.visibilityState !== 'visible') return;
    document.querySelectorAll('[data-pending-sparkle]').forEach((el) => {
        delete el.dataset.pendingSparkle;
        sparkleRing(el);
    });
});

// Track the window height every few frames while a subgroup rolls, so the
// window shrinks/grows WITH the content instead of snapping at the end
function _followResize(durationMs) {
    if (isCompactMode) return;
    const end = performance.now() + durationMs;
    let tick = 0;
    (function step() {
        if (performance.now() >= end) {
            const b = document.body.classList;
            if (!isCompactMode && !b.contains('tall') && !b.contains('landscape')
                && elements.settingsOverlay.style.display === 'none') {
                const th = _chromeHeight();
                const ch = elements.mainContent.scrollHeight;
                const bh = _bannersHeight();
                if (th >= 10 && ch >= 40) window.electronAPI.resizeWindow(th + bh + ch + 10, true, false, true);
            } else {
                resizeWidget();
            }
            return;
        }
        if ((tick++ % 3) === 0 && elements.settingsOverlay.style.display === 'none') {
            const th = _chromeHeight();
            const ch = elements.mainContent.scrollHeight;
            if (th >= 10 && ch >= 40) {
                const bh = _bannersHeight();
                window.electronAPI.resizeWindow(th + bh + ch + 10);
            }
        }
        requestAnimationFrame(step);
    })();
}

const SUBGROUP_PROVIDERS = [
    { key: 'anthropic', cliRows: 'anthropicCliRows', desk: 'sgAnthropicDesktop', cli: 'sgAnthropicCli' },
    { key: 'openai', cliRows: 'openaiCliRows', desk: 'sgOpenaiDesktop', cli: 'sgOpenaiCli' },
    { key: 'google', cliRows: 'googleCliRows', desk: 'sgGoogleDesktop', cli: 'sgGoogleCli' }
];

function initSubheadings() {
    document.querySelectorAll('.subheading').forEach((btn) => {
        const letters = [...(btn.dataset.label || '')];
        btn.textContent = '';
        // Letters char in the order the flame front reaches them (right to
        // left); healing runs the opposite way
        const span = Math.max(SUBGROUP_SWEEP_MS - 80, 100);
        letters.forEach((ch, i) => {
            const s = document.createElement('span');
            s.className = 'sub-letter';
            s.textContent = ch === ' ' ? '\u00A0' : ch;
            s.style.setProperty('--burn-d', `${Math.round(((letters.length - 1 - i) / letters.length) * span)}ms`);
            s.style.setProperty('--heal-d', `${Math.round((i / letters.length) * span)}ms`);
            s.style.setProperty('--rot', `${((i * 37) % 7) - 3}deg`);
            btn.appendChild(s);
        });
        btn.addEventListener('click', () => onSubheadingClick(btn));
    });
}

async function onSubheadingClick(btn) {
    if (btn.dataset.animating) return;
    const group = btn.closest('.subgroup');
    if (!group) return;
    const nowHidden = !group.classList.contains('hidden-group');
    btn.dataset.animating = '1';
    btn.classList.remove('burnt', 'burning', 'unburning');
    void btn.offsetWidth; // restart letter animations from a clean slate
    btn.classList.add(nowHidden ? 'burning' : 'unburning');
    runPixelSweep(btn, nowHidden);
    group.classList.add('rolling');
    group.classList.toggle('hidden-group', nowHidden);
    _followResize(SUBGROUP_SWEEP_MS + 550);
    setTimeout(() => {
        btn.classList.remove('burning', 'unburning');
        btn.classList.toggle('burnt', nowHidden);
        delete btn.dataset.animating;
        group.classList.remove('rolling');
    }, SUBGROUP_SWEEP_MS + 300);
    const settings = window._cachedSettings || {};
    const subgroupHidden = { ...(settings.subgroupHidden || {}), [btn.dataset.key]: nowHidden };
    await _saveSettingsPatch({ subgroupHidden });
}

function applySubgroups() {
    const hiddenMap = (window._cachedSettings || {}).subgroupHidden || {};
    for (const p of SUBGROUP_PROVIDERS) {
        const dual = elements[p.cliRows] && elements[p.cliRows].children.length > 0;
        const desk = document.getElementById(p.desk);
        const cli = document.getElementById(p.cli);
        if (!desk || !cli) continue;
        cli.style.display = dual ? '' : 'none';
        const deskHead = desk.querySelector('.subheading-row');
        if (deskHead) deskHead.style.display = dual ? '' : 'none';
        // Without a CLI twin there are no subheadings to click, so nothing
        // may stay burnt away — force both groups visible (the stored state
        // is kept for when dual mode returns)
        setSubgroupState(desk, dual && !!hiddenMap[p.key + '_desktop']);
        setSubgroupState(cli, dual && !!hiddenMap[p.key + '_cli']);
    }
}

// ---- Landscape dual tables ----
// One shared label column; CLI trio (pct/ring/pie) immediately LEFT of the
// Desktop trio, each cluster under its own duplicated column labels.
function dualPairsFor(company, data) {
    const mk = (pct, resetsAt, total) => ({ pct, resetsAt, total });
    if (company === 'google') {
        const d = data.gemini;
        if (!d || !d.cli || !d.cli.limits || !d.cli.limits.length) return null;
        const byKey = {};
        for (const l of (d.limits || [])) byKey[l.key] = { label: l.label, desk: mk(l.percent, l.resetsAt, 1440) };
        for (const l of d.cli.limits) {
            byKey[l.key] = byKey[l.key] || { label: l.label };
            byKey[l.key].cli = mk(l.percent, l.resetsAt, 1440);
        }
        return Object.entries(byKey).map(([k, v], i) => ({
            code: rowCode('gemini_' + k, v.label), color: GEMINI_BLUES[i % GEMINI_BLUES.length],
            name: v.label, desk: v.desk, cli: v.cli
        }));
    }
    if (company === 'openai') {
        const d = data.codex;
        if (!d || !d.cli || !d.cli.limits || !d.cli.limits.length) return null;
        const total = (k) => k.includes('seven_day') ? 10080 : 300;
        const creditInfo = (credits) => credits ? {
            kind: 'summary',
            text: credits.unlimited ? '∞' : String(credits.balance ?? 0),
            title: credits.unlimited ? 'Unlimited account credits' : `${credits.balance ?? 0} account credits`
        } : null;
        const resetInfo = (resets) => {
            if (!resets) return null;
            const avail = resets.available ?? 0;
            // A banked reset is a glowing orb everywhere else it appears (tall
            // rows, compact rows); wide mode showed a bare "1" instead, which
            // read as a count, not the same live token. Render the orbs here
            // too so the three layouts agree — the pixel-fire loop picks up any
            // .reset-dot in the DOM, so the glow comes for free. Only fall back
            // to the number when there is nothing banked to draw.
            if (avail > 0) {
                const credits = Array.isArray(resets.credits) ? resets.credits : [];
                const tip = credits.length
                    ? 'Limit Resets\n' + credits.map((c, i) => c.expiresAt
                        ? `${i + 1}. expires in ${formatCountdown(Math.max(c.expiresAt - Date.now(), 0))}`
                        : `${i + 1}. no expiry reported`).join('\n')
                    : `${avail} banked limit reset${avail === 1 ? '' : 's'}`;
                return { kind: 'orbs', orbs: Math.min(avail, 12), title: tip };
            }
            return { kind: 'summary', text: String(avail), title: `${avail} banked limit resets` };
        };
        const hiddenRows = hiddenRowsMap();
        // Pair desktop and CLI rows by LABEL, not key. OpenAI renamed its
        // keys per surface ("secondary_seven_day" on desktop vs
        // "primary_seven_day" in the CLI for the same "Codex (7d)" pool), so
        // key-matching stopped merging the shared pool and drew two
        // half-empty rows. The label is the pool's stable identity.
        const poolId = (label) => String(label || '').trim().toLowerCase();
        const byPool = {};
        for (const l of (d.limits || [])) {
            byPool[poolId(l.label)] = { label: l.label, key: l.key, desk: mk(l.percent, l.resetsAt, total(l.key)) };
        }
        for (const l of d.cli.limits) {
            const id = poolId(l.label);
            byPool[id] = byPool[id] || { label: l.label, key: l.key };
            byPool[id].cli = mk(l.percent, l.resetsAt, total(l.key));
        }
        const pairs = Object.values(byPool).map((v) => ({
            code: rowCode('codex_' + v.key, v.label), color: CODE_COLORS.codex,
            name: v.label, desk: v.desk, cli: v.cli
        }));
        const deskCredits = hiddenRows.codex_row_credits ? null : creditInfo(d.credits);
        const cliCredits = hiddenRows.codex_cli_row_credits ? null : creditInfo(d.cli.credits);
        if (deskCredits || cliCredits) pairs.push({
            code: 'CR', color: CODE_COLORS.codex, name: 'Credits',
            desk: deskCredits, cli: cliCredits
        });
        const deskResets = hiddenRows.codex_row_resets ? null : resetInfo(d.resetCredits);
        const cliResets = hiddenRows.codex_cli_row_resets ? null : resetInfo(d.cli.resetCredits);
        if (deskResets || cliResets) pairs.push({
            code: 'RST', color: CODE_COLORS.codex, name: 'Limit Resets',
            desk: deskResets, cli: cliResets
        });
        return pairs;
    }
    if (company === 'anthropic') {
        const cc = data.claude_code_same_account === false ? data.claude_code : null;
        if (!cc) return null;
        const pairs = [];
        if (data.five_hour || cc.five_hour) pairs.push({
            code: 'CLA 5H', color: '#e0916f', name: 'Claude Session (5h)',
            desk: data.five_hour && mk(data.five_hour.utilization, data.five_hour.resets_at, 300),
            cli: cc.five_hour && cc.five_hour.utilization != null && mk(cc.five_hour.utilization, cc.five_hour.resets_at, 300)
        });
        if (data.seven_day || cc.seven_day) pairs.push({
            code: 'CLA 7D', color: CODE_COLORS.weekly, name: 'Claude Models (7d)',
            desk: data.seven_day && mk(data.seven_day.utilization, data.seven_day.resets_at, 10080),
            cli: cc.seven_day && cc.seven_day.utilization != null && mk(cc.seven_day.utilization, cc.seven_day.resets_at, 10080)
        });
        const deskScoped = (data.limits || []).filter((l) => l.kind === 'weekly_scoped' && l.percent != null);
        for (const l of deskScoped) {
            const nm = l.scope?.model?.display_name || 'Scoped';
            const twin = (cc.limits || []).find((x) => x.kind === 'weekly_scoped' && (x.scope?.model?.display_name || '') === nm);
            pairs.push({
                code: nm.slice(0, 3).toUpperCase(), color: CODE_COLORS.fable, name: nm + ' (7d)',
                desk: mk(l.percent, l.resets_at, 10080),
                cli: twin && mk(twin.percent, twin.resets_at, 10080)
            });
        }
        return pairs.length ? pairs : null;
    }
    return null;
}

function buildDualPair(info, cliSide) {
    const pair = document.createElement('div');
    pair.className = 'dual-pair' + (cliSide ? ' cli' : '');
    if (!info) {
        // This account simply doesn't have the pool (plans differ) — an
        // explicit dash reads as "not on this account", where an empty cell
        // read as missing data.
        pair.classList.add('special', 'absent');
        const dash = document.createElement('span');
        dash.className = 'dual-absent-mark';
        dash.textContent = '\u2013';
        pair.title = 'This account has no such pool';
        pair.appendChild(dash);
        return pair;
    }
    if (info.kind === 'summary') {
        pair.classList.add('special');
        pair.title = info.title || '';
        const value = document.createElement('span');
        value.className = 'dual-special-value';
        value.textContent = info.text;
        pair.appendChild(value);
        return pair;
    }
    if (info.kind === 'orbs') {
        pair.classList.add('special', 'dual-orbs');
        pair.title = info.title || '';
        const dots = document.createElement('div');
        dots.className = 'reset-dots' + (info.orbs === 1 ? ' single' : '');
        for (let i = 0; i < info.orbs; i++) {
            const dot = document.createElement('span');
            dot.className = 'reset-dot';
            dot.style.animationDelay = `${(i * 1.3 + 0.4).toFixed(1)}s`;
            dots.appendChild(dot);
        }
        pair.appendChild(dots);
        return pair;
    }
    const pct = document.createElement('span');
    pct.className = 'dual-pct';
    pct.textContent = Math.round(Math.min(Math.max(info.pct || 0, 0), 100)) + '%';
    pair.appendChild(pct);
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('class', 'mini-timer');
    svg.setAttribute('width', '22');
    svg.setAttribute('height', '22');
    svg.setAttribute('viewBox', '0 0 24 24');
    const bg = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    bg.setAttribute('class', 'timer-bg');
    bg.setAttribute('cx', '12'); bg.setAttribute('cy', '12'); bg.setAttribute('r', '10');
    svg.appendChild(bg);
    const prog = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    prog.setAttribute('class', 'timer-progress');
    prog.setAttribute('cx', '12'); prog.setAttribute('cy', '12'); prog.setAttribute('r', '10');
    prog.style.strokeDasharray = '63';
    prog.style.strokeDashoffset = '63';
    svg.appendChild(prog);
    pair.appendChild(svg);
    const pie = document.createElement('span');
    pie.className = 'timer-text pie-cell';
    pie.dataset.resets = info.resetsAt || '';
    pie.dataset.total = info.total;
    pair.appendChild(pie);
    return pair;
}

// Wide mode: a company whose second (CLI) account is collapsed no longer
// needs a full third of the window. Give each collapsed column a fixed narrow
// width and let the rest flex, then shrink the window by the reclaimed space
// so there's no empty gap. Expanding a cluster restores the full preset width.
const COLLAPSED_COL_W = 196;   // label + desktop trio + >_ restore + padding
const EXPANDED_COL_W = 284;    // a full desktop+CLI dual column (measured)
const WIDE_H_CHROME = 48;      // .content padding (20) + two 14px column gaps
function syncLandscapeCliWidth() {
    if (_activePreset !== 'wide' || !document.body.classList.contains('landscape')
        || !window.electronAPI.fitLandscapeWidth) return;
    const mc = elements.mainContent;
    const sections = ['sectionAnthropic', 'sectionOpenai', 'sectionGoogle']
        .map((id) => document.getElementById(id));
    const cols = [];
    let collapsed = 0;
    let target = WIDE_H_CHROME;
    for (const sec of sections) {
        const dual = sec && sec.querySelector('.dual-table');
        const isCollapsed = !!(dual && dual.classList.contains('hide-cli'));
        if (isCollapsed) { cols.push(COLLAPSED_COL_W + 'px'); target += COLLAPSED_COL_W; collapsed++; }
        else { cols.push('minmax(0, 1fr)'); target += EXPANDED_COL_W; }
    }
    if (collapsed === 0) {
        // All second accounts shown — hand geometry back to the full preset.
        mc.style.gridTemplateColumns = '';
        window.electronAPI.fitLandscapeWidth(0);
        return;
    }
    mc.style.gridTemplateColumns = cols.join(' ');
    // Never shrink below the landscape threshold (innerWidth >= 760) or the
    // wide layout would disengage back to portrait.
    window.electronAPI.fitLandscapeWidth(Math.max(786, target));
}

function renderDualTables(data) {
    const hiddenMap = (window._cachedSettings || {}).subgroupHidden || {};
    for (const [company, tableId, bodySel] of [
        ['anthropic', 'dualTableAnthropic', '#sectionAnthropic .section-body'],
        ['openai', 'dualTableOpenai', '#sectionOpenai .section-body'],
        ['google', 'dualTableGoogle', '#sectionGoogle .section-body']
    ]) {
        const table = document.getElementById(tableId);
        const body = document.querySelector(bodySel);
        if (!table || !body) continue;
        const pairs = dualPairsFor(company, data);
        // Rank-by-use also applies to the landscape dual tables: heaviest
        // side of each desktop/CLI pair wins; summary rows keep the bottom.
        if (pairs && (window._cachedSettings || {}).sortByUsage) {
            const sideScore = (side) => (side && typeof side.pct === 'number') ? side.pct : null;
            const pairScore = (p) => {
                const desk = sideScore(p.desk);
                const cli = sideScore(p.cli);
                if (desk == null && cli == null) return -1;
                return Math.max(desk ?? -1, cli ?? -1);
            };
            pairs.sort((a, b) => pairScore(b) - pairScore(a));
        }
        body.classList.toggle('has-dual', !!pairs);
        table.innerHTML = '';
        if (!pairs) continue;
        table.classList.toggle('hide-cli', !!hiddenMap[company + '_cli']);

        // group heading row: [ ] [CLI pill] [ ] [DESKTOP]
        const groupHead = document.createElement('div');
        groupHead.className = 'dual-head';
        groupHead.appendChild(document.createElement('span'));
        const deskHead = document.createElement('span');
        deskHead.textContent = 'Desktop';
        groupHead.appendChild(deskHead);
        groupHead.appendChild(Object.assign(document.createElement('span'), { className: 'dual-gap' }));
        const pill = document.createElement('button');
        pill.className = 'dual-pill cli-head';
        pill.title = 'Second account (CLI login) — click to hide/show its columns';
        {
            const chars = [...'CLI: 2ND ACCT'];
            const span = Math.max(SUBGROUP_SWEEP_MS - 80, 100);
            chars.forEach((ch, i) => {
                const s = document.createElement('span');
                s.className = 'sub-letter';
                s.textContent = ch === ' ' ? '\u00A0' : ch;
                s.style.setProperty('--burn-d', `${Math.round(((chars.length - 1 - i) / chars.length) * span)}ms`);
                s.style.setProperty('--heal-d', `${Math.round((i / chars.length) * span)}ms`);
                s.style.setProperty('--rot', `${((i * 37) % 7) - 3}deg`);
                pill.appendChild(s);
            });
        }
        if (hiddenMap[company + '_cli']) pill.classList.add('burnt');
        pill.addEventListener('click', async () => {
            if (pill.dataset.animating) return;
            const hidden = !table.classList.contains('hide-cli');
            pill.dataset.animating = '1';
            pill.classList.remove('burnt', 'burning', 'unburning');
            void pill.offsetWidth;
            // Both directions use the same burn flash. Revealing removes the
            // terminal state first, burns across the restored title, then
            // leaves the clean text behind when the animation finishes.
            pill.classList.add('burning');
            runPixelSweep(pill, true);
            if (!hidden) {
                table.classList.remove('hide-cli');
                syncLandscapeCliWidth();
            }
            setTimeout(() => {
                if (hidden) table.classList.add('hide-cli');
                syncLandscapeCliWidth();
            }, 160);
            setTimeout(() => {
                pill.classList.remove('burning', 'unburning');
                pill.classList.toggle('burnt', hidden);
                delete pill.dataset.animating;
            }, SUBGROUP_SWEEP_MS + 300);
            const subgroupHidden = { ...((window._cachedSettings || {}).subgroupHidden || {}), [company + '_cli']: hidden };
            await _saveSettingsPatch({ subgroupHidden });
            applySubgroups();
        });
        groupHead.appendChild(pill);
        table.appendChild(groupHead);

        // duplicated column labels for each cluster
        const colHead = document.createElement('div');
        colHead.className = 'dual-head';
        colHead.appendChild(document.createElement('span'));
        for (const cls of ['desk', 'cli-head']) {
            const grp = document.createElement('span');
            grp.className = 'dual-group-head ' + (cls === 'cli-head' ? 'cli-head' : '');
            for (const t of ['Used', 'Elap', 'In']) {
                const s = document.createElement('span');
                s.textContent = t;
                grp.appendChild(s);
            }
            if (cls === 'cli-head') colHead.appendChild(Object.assign(document.createElement('span'), { className: 'dual-gap' }));
            colHead.appendChild(grp);
        }
        table.appendChild(colHead);

        for (const p of pairs) {
            const row = document.createElement('div');
            row.className = 'dual-row';
            row.title = p.name;
            const label = document.createElement('span');
            label.className = 'dual-label';
            label.textContent = p.code;
            label.style.color = p.color;
            row.appendChild(label);
            row.appendChild(buildDualPair(p.desk, false));
            row.appendChild(Object.assign(document.createElement('span'), { className: 'dual-gap' }));
            row.appendChild(buildDualPair(p.cli, true));
            table.appendChild(row);
        }
    }
    requestAnimationFrame(syncLandscapeCliWidth);
}

function setSubgroupState(group, hidden) {
    const btn = group.querySelector('.subheading');
    if (btn && btn.dataset.animating) return; // never fight a live burn
    if (group.classList.contains('hidden-group') === hidden) return;
    group.classList.add('no-anim');
    group.classList.toggle('hidden-group', hidden);
    if (btn) {
        btn.classList.remove('burning', 'unburning');
        btn.classList.toggle('burnt', hidden);
    }
    requestAnimationFrame(() => requestAnimationFrame(() => group.classList.remove('no-anim')));
}

function refreshExtraTimers() {
    // Pair each row's timer text with its own circle. Pairing the two
    // querySelectorAll lists by index breaks as soon as one row has a text
    // but no circle (the extra_usage row), leaving every later row's timer
    // stuck at --:--.
    // Covers the pinned scoped rows, the expandable panel, and provider sections.
    document.querySelectorAll('.dual-pair').forEach((pair) => {
        const textEl = pair.querySelector('.timer-text');
        const circleEl = pair.querySelector('.timer-progress');
        if (!textEl || !circleEl) return;
        if (textEl.dataset.resets) updateTimer(circleEl, textEl, textEl.dataset.resets, parseInt(textEl.dataset.total));
    });
    for (const container of [elements.scopedRows, elements.extraRows, elements.openaiRows, elements.googleRows,
        elements.anthropicCliRows, elements.openaiCliRows, elements.googleCliRows]) {
        container.querySelectorAll('.usage-section').forEach((row) => {
            const textEl = row.querySelector('.timer-text');
            const circleEl = row.querySelector('.timer-progress');
            if (!textEl || !circleEl) return;
            const resetsAt = textEl.dataset.resets;
            const totalMinutes = parseInt(textEl.dataset.total);
            if (resetsAt) {
                updateTimer(circleEl, textEl, resetsAt, totalMinutes);
            }
        });
    }
}

const BANNER_HEIGHT = 28;
const EXPAND_OVERHEAD = 28; // margin-top(12) + padding-top(6) + bottom buffer(10)

function resizeWidget(bannerVisible) {
    // Measure the actual laid-out content instead of summing row constants —
    // with collapsible provider sections the arithmetic became unmaintainable.
    // bannerVisible is accepted for call-site compatibility; the measurement
    // sees the banner's real display state (set before any resize call).
    void bannerVisible;
    requestAnimationFrame(() => {
        if (isCompactMode) return;
        // Never re-measure while the settings overlay is up — a background
        // refresh would stretch the window underneath the panel
        if (elements.settingsOverlay.style.display !== 'none') return;
        const titleHeight = _chromeHeight();
        // Intrinsic children-bottom measurement: .content flex-stretches to
        // the window, so scrollHeight would track the window (not the
        // content) whenever the window is taller — the seed of the
        // grow-forever loop.
        const contentHeight = _intrinsicMainContentHeight();
        // While minimized/hidden every measurement reads ~0 — resizing then
        // would shrink the window to a sliver. Skip; the focus listener
        // re-measures on restore.
        if (titleHeight < 10 || contentHeight < 40) return;
        const bannerHeight = _bannersHeight();
        // +2 for the widget-container border, +8 bottom breathing room
        const target = titleHeight + bannerHeight + contentHeight + 10;
        // Grow whenever needed (skipping leaves a scrollbar); skip only
        // small shrinks so measurement noise can't thrash the window.
        const delta = target - window.innerHeight;
        if (delta > 0 ? delta <= 2 : delta >= -10) return;
        window.electronAPI.resizeWindow(target);
    });
}

// Colour classes for scoped weekly limits that have their own CSS identity;
// anything not listed falls back to the opus colour.
const SCOPED_COLOR_CLASSES = { fable: 'fable' };

function normalizeUsageData(data) {
    // claude.ai now reports per-model weekly limits (e.g. Fable) as entries in
    // the `limits` array with kind "weekly_scoped"; the legacy seven_day_<model>
    // fields arrive null for those models. Map each scoped weekly limit onto a
    // synthetic seven_day_* field so it renders like any other extra row.
    for (const limit of (data.limits || [])) {
        if (limit.kind !== 'weekly_scoped' || limit.percent == null) continue;
        const scopeName = limit.scope?.model?.display_name || limit.scope?.surface || 'Scoped';
        const slug = scopeName.toLowerCase().replace(/[^a-z0-9]+/g, '_');
        const key = 'seven_day_scoped_' + slug;
        if (!EXTRA_ROW_CONFIG[key]) {
            EXTRA_ROW_CONFIG[key] = { label: `${scopeName} (7d)`, color: SCOPED_COLOR_CLASSES[slug] || 'opus' };
            // Re-insert extra_usage so model rows stay grouped above it
            const extraUsage = EXTRA_ROW_CONFIG.extra_usage;
            delete EXTRA_ROW_CONFIG.extra_usage;
            EXTRA_ROW_CONFIG.extra_usage = extraUsage;
        }
        data[key] = { utilization: limit.percent, resets_at: limit.resets_at };
    }

    // Claude Code (CLI) account — fetched by the main process using the local
    // OAuth credentials; same response shape as the claude.ai endpoint. Rows
    // render in the expandable panel under CLI-prefixed labels — but only when
    // the CLI login is a DIFFERENT account than the web login; when the two
    // match, the rows would be pure duplication (merged mode).
    const cc = data.claude_code_same_account ? null : data.claude_code;
    if (cc) {
        const ccRows = [];
        if (cc.five_hour?.utilization != null) {
            ccRows.push(['cc_five_hour',
                { label: 'CLI Claude Session (5h)', color: 'cc' },
                { utilization: cc.five_hour.utilization, resets_at: cc.five_hour.resets_at }]);
        }
        if (cc.seven_day?.utilization != null) {
            ccRows.push(['cc_seven_day',
                { label: 'CLI Claude Models (7d)', color: 'weekly' },
                { utilization: cc.seven_day.utilization, resets_at: cc.seven_day.resets_at }]);
        }
        for (const limit of (cc.limits || [])) {
            if (limit.kind !== 'weekly_scoped' || limit.percent == null) continue;
            const scopeName = limit.scope?.model?.display_name || limit.scope?.surface || 'Scoped';
            const slug = scopeName.toLowerCase().replace(/[^a-z0-9]+/g, '_');
            ccRows.push(['cc_seven_day_scoped_' + slug,
                { label: `CLI ${scopeName} (7d)`, color: SCOPED_COLOR_CLASSES[slug] || 'opus' },
                { utilization: limit.percent, resets_at: limit.resets_at }]);
        }
        if (ccRows.length) {
            for (const [key, config, value] of ccRows) {
                if (!EXTRA_ROW_CONFIG[key]) EXTRA_ROW_CONFIG[key] = config;
                data[key] = value;
            }
            // Re-insert extra_usage so account rows stay grouped above it
            const extraUsage = EXTRA_ROW_CONFIG.extra_usage;
            delete EXTRA_ROW_CONFIG.extra_usage;
            EXTRA_ROW_CONFIG.extra_usage = extraUsage;
        }
    }

    // Codex (OpenAI) account — fetched by the main process from the codex
    // CLI's local login (live endpoint, or its session-log snapshot when the
    // token has expired). Keys embed the window so timers use the right span.
    const cx = data.codex;
    if (cx && Array.isArray(cx.limits) && cx.limits.length) {
        for (const lim of cx.limits) {
            const key = 'codex_' + lim.key;
            if (!EXTRA_ROW_CONFIG[key]) EXTRA_ROW_CONFIG[key] = { label: lim.label, color: 'codex' };
            data[key] = { utilization: lim.percent, resets_at: lim.resetsAt, windowMinutes: lim.windowMinutes };
        }
        // Dual mode: the codex CLI is a different account - tracked separately
        for (const lim of (cx.cli && cx.cli.limits || [])) {
            const key = 'codex_cli_' + lim.key;
            if (!EXTRA_ROW_CONFIG[key]) EXTRA_ROW_CONFIG[key] = { label: 'CLI ' + lim.label, color: 'codex' };
            data[key] = { utilization: lim.percent, resets_at: lim.resetsAt, windowMinutes: lim.windowMinutes };
        }
        const extraUsage = EXTRA_ROW_CONFIG.extra_usage;
        delete EXTRA_ROW_CONFIG.extra_usage;
        EXTRA_ROW_CONFIG.extra_usage = extraUsage;
    }

    // Gemini (Google) account — daily per-family quota from the gemini CLI's
    // backend, fetched by the main process with the CLI's own local login.
    const gm = data.gemini;
    if (gm && Array.isArray(gm.limits) && gm.limits.length) {
        for (const lim of gm.limits) {
            const key = 'gemini_' + lim.key;
            if (!EXTRA_ROW_CONFIG[key]) EXTRA_ROW_CONFIG[key] = { label: lim.label, color: 'gemini' };
            data[key] = { utilization: lim.percent, resets_at: lim.resetsAt, windowMinutes: lim.windowMinutes };
        }
        // Dual mode: the gemini CLI is a different account - tracked separately
        for (const lim of (gm.cli && gm.cli.limits || [])) {
            const key = 'gemini_cli_' + lim.key;
            if (!EXTRA_ROW_CONFIG[key]) EXTRA_ROW_CONFIG[key] = { label: 'CLI ' + lim.label, color: 'gemini' };
            data[key] = { utilization: lim.percent, resets_at: lim.resetsAt, windowMinutes: lim.windowMinutes };
        }
        const extraUsage = EXTRA_ROW_CONFIG.extra_usage;
        delete EXTRA_ROW_CONFIG.extra_usage;
        EXTRA_ROW_CONFIG.extra_usage = extraUsage;
    }
    return data;
}

// ---- Burn-alert fire: which rows wear flames right now ----
// main.js tracks per-series burning state with settle hysteresis (a series
// that tripped the anomaly detector stays lit until its pace halves);
// this maps that state onto concrete rows. Structural Session/Weekly bars
// use sentinels since they aren't dynamic rows.
let _burningRowKeys = new Set();
function computeBurningRowKeys(data) {
    const burning = data.burningSeries || {};
    const keys = new Set();
    if (burning.session) keys.add('STRUCT_SESSION');
    if (burning.weekly) keys.add('STRUCT_WEEKLY');
    for (const seriesKey of Object.keys(burning)) {
        if (seriesKey.startsWith('scoped_')) keys.add('seven_day_scoped_' + seriesKey.slice('scoped_'.length));
    }
    if (burning.claudeCli) keys.add('cc_seven_day');
    if (burning.codex && data.codex?.limits?.[0]) keys.add('codex_' + data.codex.limits[0].key);
    if (burning.codexCli && data.codex?.cli?.limits?.[0]) keys.add('codex_cli_' + data.codex.cli.limits[0].key);
    const worstOf = (limits) => (limits || []).reduce((worst, l) => (!worst || l.percent > worst.percent) ? l : worst, null);
    if (burning.gemini) {
        const worst = worstOf(data.gemini?.limits);
        if (worst) keys.add('gemini_' + worst.key);
    }
    if (burning.geminiCli) {
        const worst = worstOf(data.gemini?.cli?.limits);
        if (worst) keys.add('gemini_cli_' + worst.key);
    }
    return keys;
}

function updateUI(data) {
    latestUsageData = normalizeUsageData(data);
    _burningRowKeys = computeBurningRowKeys(data);
    checkBurnSpikeSound(_burningRowKeys);

    showMainContent();
    buildExtraRows(data);
    renderDualTables(data);
    refreshTimers();
    refreshExtraTimers(); // pinned scoped rows tick even when collapsed

    // Structural bars catch fire when their series tripped the burn detector.
    // The flame wears whatever colour the bar currently renders (threshold
    // recolours included) — so any colour source carries into the fire.
    const structuralFire = (el, on, pct, baseColor) => {
        if (!el) return;
        el.classList.toggle('on-fire', on);
        if (on) {
            const col = pct >= dangerThreshold ? '#ef4444' : pct >= warnThreshold ? '#f59e0b' : baseColor;
            el.style.setProperty('--fire-col', col);
        } else if (el.__fireLayer) {
            // Structural bars persist across renders — clear their flames
            el.__fireLayer.remove();
            el.__fireLayer = null;
        }
    };
    structuralFire(elements.sessionProgress, _burningRowKeys.has('STRUCT_SESSION'), data.five_hour?.utilization || 0, '#d97757');
    structuralFire(elements.weeklyProgress, _burningRowKeys.has('STRUCT_WEEKLY'), data.seven_day?.utilization || 0, '#b85c3c');

    // Burn-rate forecast tooltip on the Weekly row
    const weeklySection = elements.weeklyProgress.closest('.usage-section');
    if (weeklySection) {
        const settings = window._cachedSettings || {};
        weeklySection.title = data.forecasts?.weekly
            ? `At the current pace, 100% by ${formatResetsAt(data.forecasts.weekly, true, settings.timeFormat || '12h', 'date-day-time')}`
            : '';
    }

    // Account-status chips + connect rows per provider section
    const setChip = (el, mode) => {
        if (!el) return;
        if (!mode) { el.style.display = 'none'; return; }
        el.style.display = '';
        el.className = 'section-chip ' + mode.cls;
        el.textContent = mode.text;
        el.title = mode.title || '';
    };
    const cxStatus = data.codex;
    const gmStatus = data.gemini;
    setChip(elements.chipOpenai, cxStatus && !cxStatus.cli && !cxStatus.connected
        ? { cls: 'cli', text: 'via CLI login', title: 'Usage is being read from your Codex CLI login. A widget-owned connection is optional — sign in under Settings if you want one.' }
        : null);
    setChip(elements.chipGoogle, gmStatus && gmStatus.source === 'antigravity'
        ? { cls: 'cli', text: 'via Antigravity', title: 'Usage is being read from your Antigravity (agy) login — the only Google surface that meters agent usage. Switch to the classic Gemini quota under Settings → Google → Usage source.' }
        : (gmStatus && !gmStatus.cli && !gmStatus.connected
            ? { cls: 'cli', text: 'via CLI login', title: 'Usage is being read from your Gemini CLI login. A widget-owned connection is optional — sign in under Settings if you want one.' }
            : null));
    setChip(elements.chipAnthropic, (!data.claude_code || data.claude_code_same_account !== false) && data.anthropic_source === 'cli'
        ? { cls: 'cli anthropic-cli', text: 'via CLI login', title: 'Usage is being read from your Claude CLI login. Logging in to claude.ai under Settings additionally exposes Extra Usage and credits.' }
        : null);
    // Per-provider account email under the section header (landscape/tall only,
    // hideable). Anthropic's comes from the account endpoint; the others carry
    // their own email in the provider payload, whether signed in through the
    // app or read from a CLI login on disk.
    renderAccountEmails(data);
    renderAccountOffers(data);
    applyProviderVisibility();
    // The amber pill IS the CLI subheading now — give it the account detail
    const pillTitle = (sel, t) => { const b = document.querySelector(sel); if (b) b.title = t; };
    pillTitle('#sgOpenaiCli .subheading', cxStatus && cxStatus.cli
        ? 'Your codex CLI (' + (cxStatus.cli.email || 'other account') + ') differs from ' + (cxStatus.email || 'the connected account') + '. Click to hide these rows.'
        : '');
    pillTitle('#sgGoogleCli .subheading', gmStatus && gmStatus.cli
        ? 'Your gemini CLI (' + (gmStatus.cli.email || 'other account') + ') differs from ' + (gmStatus.email || 'the connected account') + '. Click to hide these rows.'
        : '');
    pillTitle('#sgAnthropicCli .subheading', (data.claude_code && data.claude_code_same_account === false)
        ? 'Your claude CLI is logged into a different account - tracked separately here. Click to hide these rows.'
        : '');
    if (elements.connectRowOpenai) elements.connectRowOpenai.style.display = cxStatus ? 'none' : '';
    if (elements.connectRowGoogle) elements.connectRowGoogle.style.display = gmStatus ? 'none' : '';

    // Frozen providers — logo goes on ice when an account has sat unused
    const frozen = data.frozenProviders || {};
    for (const [header, isFrozen] of [
        [elements.headerAnthropic, frozen.anthropic],
        [elements.headerOpenai, frozen.openai],
        [elements.headerGoogle, frozen.google]
    ]) {
        if (!header) continue;
        header.classList.toggle('frozen', !!isFrozen);
        applyFrozenIce(header, !!isFrozen);
        const name = header.querySelector('.section-name');
        if (name) name.title = isFrozen ? 'On ice — no usage here in a while. Send a prompt to thaw it out.' : '';
    }

    // Session-window planner hints, one per provider section. When there is
    // not enough burn history yet, show a learning placeholder rather than
    // nothing — the line stays put instead of appearing days later.
    const plans = data.sessionPlans || {};
    const PLAN_LEARNING = 'Planner: still learning this account’s rhythm — needs more usage history.';
    for (const [el, plan] of [
        [elements.planNote, plans.anthropic],
        [elements.planNoteOpenai, plans.openai],
        [elements.planNoteGoogle, plans.google]
    ]) {
        if (!el) continue;
        el.style.display = '';
        el.textContent = plan ? plan.text : PLAN_LEARNING;
        el.title = plan ? plan.text : PLAN_LEARNING;
        el.style.opacity = plan ? '' : '0.55';
    }

    if (!isCompactMode) resizeWidget();
    startCountdown();
    if (graphVisible) {
        loadChart();
    }

    // Update compact bars in parallel if compact mode is active
    if (isCompactMode) updateCompactBars(data);

    // On first load, seed alert flags so we don't fire for thresholds
    // the user can already see when the app starts
    if (isFirstDataLoad) {
        isFirstDataLoad = false;
        seedAlertFlags(data);
    }

    checkUsageAlerts(data);
    checkEarlyResets(data);
}

// ---- Early-reset fanfare -------------------------------------------------
// A limit dropping to zero is only worth celebrating when it happens BEFORE
// the provider said it would — an OpenAI reset (banked or applied straight
// away) or the Anthropic equivalent. A pool rolling over on schedule is just
// Tuesday. Both states are remembered per refresh so the two can be told
// apart; the bank count is watched too, since a banked reset arriving is its
// own good news even before it is spent.
const EARLY_RESET_FROM = 5;   // was at least this full…
const EARLY_RESET_TO = 1;     // …and came back essentially empty
let _resetWatch = null;       // null until seeded — never fires on first load
let _resetBank = null;
let _blockedKeys = null;      // pool keys currently at 100% (wall tracking)

function resetWatchPools(data) {
    const out = [];
    const add = (key, pct, resetsAt, label) => {
        if (pct == null || !isFinite(pct)) return;
        out.push({ key, pct, resetsAt: Date.parse(resetsAt || ''), label: label || key });
    };
    add('five_hour', data.five_hour?.utilization, data.five_hour?.resets_at, 'Claude Session (5h)');
    add('seven_day', data.seven_day?.utilization, data.seven_day?.resets_at, 'Claude Models (7d)');
    for (const key of Object.keys(EXTRA_ROW_CONFIG)) {
        if (data[key]) add(key, data[key].utilization, data[key].resets_at, EXTRA_ROW_CONFIG[key].label);
    }
    const feeds = [
        ['codex_', data.codex?.limits, 'OpenAI '], ['codex_cli_', data.codex?.cli?.limits, 'OpenAI CLI '],
        ['gemini_', data.gemini?.limits, 'Google '], ['gemini_cli_', data.gemini?.cli?.limits, 'Google CLI ']
    ];
    for (const [prefix, list, brand] of feeds) {
        for (const lim of (list || [])) {
            add(prefix + lim.key, lim.percent, lim.resetsAt || lim.resets_at, brand + (lim.label || lim.key));
        }
    }
    return out;
}

function checkEarlyResets(data) {
    const pools = resetWatchPools(data);
    const bank = data.codex?.resetCredits?.available ?? null;

    if (_resetWatch === null) {          // first load: just take the baseline
        _resetWatch = {};
        for (const p of pools) _resetWatch[p.key] = p;
        _resetBank = bank;
        // Seed the blocked set too — a pool already at 100% when the app
        // launches must not fire "hit the wall" on the first refresh.
        _blockedKeys = new Set(pools.filter((p) => p.pct >= 100).map((p) => p.key));
        return;
    }

    const now = Date.now();
    let freed = false;
    let wallPool = null;
    for (const p of pools) {
        const prev = _resetWatch[p.key];
        _resetWatch[p.key] = p;
        if (!prev) continue;
        // Hitting the wall: a pool crossing from usable to 100% between two
        // refreshes. Independent of the reset/banked events below — bad news
        // does not queue behind good news.
        if (prev.pct < 100 && p.pct >= 100 && !wallPool) wallPool = p;
        if (!(prev.pct >= EARLY_RESET_FROM && p.pct <= EARLY_RESET_TO)) continue;
        // Only early if the reset the provider promised was still in the future
        if (!isFinite(prev.resetsAt) || prev.resetsAt <= now) continue;
        freed = true;
    }

    const nowBlocked = new Set(pools.filter((p) => p.pct >= 100).map((p) => p.key));
    if (wallPool) {
        playAlertSound('wall');
        const when = isFinite(wallPool.resetsAt)
            ? ' Resets ' + formatResetsAt(new Date(wallPool.resetsAt).toISOString(), true,
                (window._cachedSettings || {}).timeFormat || '12h', 'date-day-time') + '.'
            : '';
        window.electronAPI.showNotification('Limit reached — ' + wallPool.label,
            wallPool.label + ' is at 100%.' + when);
    } else if (_blockedKeys && _blockedKeys.size > 0 && nowBlocked.size === 0) {
        // Every wall the user had hit has cleared — worth a heads-up (the
        // reset itself already made its sound; this is the notification).
        window.electronAPI.showNotification("I'm Burning!", 'Usage is available again.');
    }
    _blockedKeys = nowBlocked;

    const banked = bank != null && _resetBank != null && bank > _resetBank;
    if (bank != null) _resetBank = bank;

    // A limit clearing early and a banked reset landing are different events,
    // so they get different sounds. If somehow both happen on one refresh,
    // the banked one wins — it is the rarer, more notable event.
    if (banked) { playAlertSound('banked'); return; }
    if (freed) playAlertSound('reset');
}


// Fire OS desktop notifications when usage crosses warn/danger thresholds.
// Only fires once per threshold crossing per session window — not on every refresh.
function checkUsageAlerts(data) {
    const settings = window._cachedSettings || {};
    if (!settings.usageAlerts) return;

    const sessionPct = data.five_hour?.utilization || 0;
    const weeklyPct = data.seven_day?.utilization || 0;

    // Reset alert flags when a session window resets (utilization drops back low)
    if (sessionPct < warnThreshold) {
        alertFired.session_warn = false;
        alertFired.session_danger = false;
    }
    if (weeklyPct < warnThreshold) {
        alertFired.weekly_warn = false;
        alertFired.weekly_danger = false;
    }

    // Current Session — danger threshold (check first, higher priority)
    if (sessionPct >= dangerThreshold && !alertFired.session_danger) {
        alertFired.session_danger = true;
        alertFired.session_warn = true; // suppress warn if we jumped straight to danger
        window.electronAPI.showNotification(
            "I'm Burning!",
            `Anthropic Session usage is at ${Math.round(sessionPct)}% — running low`
        );
    // Current Session — warn threshold
    } else if (sessionPct >= warnThreshold && !alertFired.session_warn) {
        alertFired.session_warn = true;
        window.electronAPI.showNotification(
            "I'm Burning!",
            `Anthropic Session usage has reached ${Math.round(sessionPct)}%`
        );
    }

    // Weekly Limit — danger threshold
    if (weeklyPct >= dangerThreshold && !alertFired.weekly_danger) {
        alertFired.weekly_danger = true;
        alertFired.weekly_warn = true;
        window.electronAPI.showNotification(
            "I'm Burning!",
            `Anthropic Weekly (all models) usage is at ${Math.round(weeklyPct)}% — running low`
        );
        window.electronAPI.sendAlertWebhook('weekly_danger', 'Claude usage warning',
            `Anthropic Weekly (all models) usage is at ${Math.round(weeklyPct)}% — running low`);
    // Weekly Limit — warn threshold
    } else if (weeklyPct >= warnThreshold && !alertFired.weekly_warn) {
        alertFired.weekly_warn = true;
        window.electronAPI.showNotification(
            "I'm Burning!",
            `Anthropic Weekly (all models) usage has reached ${Math.round(weeklyPct)}%`
        );
    }

    // Scoped weekly limits (e.g. Fable) — same warn/danger pattern, plus a
    // maxed-out alert at 99%+ with the reset time
    for (const [key, config] of Object.entries(EXTRA_ROW_CONFIG)) {
        if (!key.startsWith('seven_day_scoped_')) continue;
        const value = data[key];
        if (!value || value.utilization == null) continue;
        const pct = value.utilization;
        const label = config.label;

        if (pct < warnThreshold) {
            alertFired[`${key}_warn`] = false;
            alertFired[`${key}_danger`] = false;
            alertFired[`${key}_maxed`] = false;
        }

        if (pct >= 99 && !alertFired[`${key}_maxed`]) {
            alertFired[`${key}_maxed`] = true;
            alertFired[`${key}_danger`] = true;
            alertFired[`${key}_warn`] = true;
            const settings = window._cachedSettings || {};
            const resetStr = value.resets_at
                ? ` — resets ${formatResetsAt(value.resets_at, true, settings.timeFormat || '12h', 'date-day-time')}`
                : '';
            window.electronAPI.showNotification(
                "I'm Burning!",
                `Anthropic ${label} limit is maxed out${resetStr}`
            );
            window.electronAPI.sendAlertWebhook('scoped_maxed', 'Claude limit maxed',
                `Anthropic ${label} limit is maxed out${resetStr}`);
        } else if (pct >= dangerThreshold && !alertFired[`${key}_danger`]) {
            alertFired[`${key}_danger`] = true;
            alertFired[`${key}_warn`] = true;
            window.electronAPI.showNotification(
                "I'm Burning!",
                `Anthropic ${label} usage is at ${Math.round(pct)}% — running low`
            );
            window.electronAPI.sendAlertWebhook('scoped_danger', 'Claude usage warning',
                `Anthropic ${label} usage is at ${Math.round(pct)}% — running low`);
        } else if (pct >= warnThreshold && !alertFired[`${key}_warn`]) {
            alertFired[`${key}_warn`] = true;
            window.electronAPI.showNotification(
                "I'm Burning!",
                `Anthropic ${label} usage has reached ${Math.round(pct)}%`
            );
        }
    }
}

// Apply or remove compact mode — switches view, resizes window, syncs all toggles
// Pizazz on: happy clown, everything animates. Off: clown in jail, sad and
// crying, and every animation/transition in the app goes dead still.
function applyPizazz(on) {
    document.body.classList.toggle('no-pizazz', !on);
    // The clown is an inline SVG now: jailing toggles a class and CSS slams
    // the bars down, flips the smile, drains his colour and starts the tears.
    const btn = document.getElementById('clownBtn');
    if (btn) {
        btn.classList.toggle('jailed', !on);
        btn.title = on
            ? 'Pizazz: ON — click to jail the clown and turn off all visual effects'
            : 'Pizazz: OFF — the clown weeps behind bars. Click to free him and the sparkles.';
    }
}

function applyCompactMode(compact) {
    isCompactMode = compact;

    // Press slams down while the widget is crushed
    const pressBtn = document.getElementById('compactPressBtn');
    if (pressBtn) pressBtn.classList.toggle('pressed', compact);

    // Add/remove compact-mode class from body for CSS styling
    if (compact) {
        document.body.classList.add('compact-mode');
    } else {
        document.body.classList.remove('compact-mode');
    }

    // Show/hide the correct content view
    elements.mainContent.style.display = compact ? 'none' : 'block';
    elements.compactContent.style.display = compact ? 'flex' : 'none';

    // Collapse extra rows when entering compact — prevents stale isExpanded state
    if (compact && isExpanded) {
        isExpanded = false;
        elements.expandArrow.classList.remove('expanded');
        elements.expandSection.style.display = 'none';
    }

    if (compact && graphVisible) {
        graphWasVisible = true;
        graphVisible = false;
        elements.graphBtn.classList.remove('active');
        elements.graphSection.style.display = 'none';
    } else if (!compact && graphWasVisible) {
        graphWasVisible = false;
        graphVisible = true;
        elements.graphBtn.classList.add('active');
        elements.graphSection.style.display = 'block';
        loadChart();
    }
    syncGraphLayoutState();

    // Keep refresh button visible in compact mode so users can see when data updates
    // Hide graph button in compact mode (not applicable)
    if (elements.graphBtn) {
        elements.graphBtn.style.display = compact ? 'none' : '';
    }

    // Tell main process to resize the window width
    window.electronAPI.setCompactMode(compact);

    // Sync both settings toggles
    if (elements.compactModeToggle) elements.compactModeToggle.checked = compact;
    if (elements.compactModeToggleCompact) elements.compactModeToggleCompact.checked = compact;

    // Update compact bars if we have data
    if (compact && latestUsageData) updateCompactBars(latestUsageData);
    if (!compact) resizeWidget();

    // Persist graph/expanded state changes caused by compact mode toggle
    _saveViewState();
}

// Compact mode: one slim [code][bar][%] row per active pool across ALL
// providers, grouped by a company-colour edge; CLI second-account rows carry
// the terminal-cursor underscore, matching the tray badge language.
function updateCompactBars(data) {
    const container = document.getElementById('compactRows');
    if (!container) return;
    const clamp = (v) => Math.min(Math.max(v || 0, 0), 100);
    const pools = [];

    pools.push({ co: 'anthropic', code: 'CLA 5H', name: 'Claude Session (5h)', pct: clamp(data.five_hour?.utilization), color: '#e0916f', burnKey: 'STRUCT_SESSION' });
    pools.push({ co: 'anthropic', code: 'CLA 7D', name: 'Claude Models (7d)', pct: clamp(data.seven_day?.utilization), color: CODE_COLORS.weekly, burnKey: 'STRUCT_WEEKLY' });
    const hiddenRows = hiddenRowsMap();
    for (const [key, config] of Object.entries(EXTRA_ROW_CONFIG)) {
        const value = data[key];
        if (!value || value.utilization == null) continue;
        if (hiddenRows[key]) continue;
        if (key.startsWith('seven_day_scoped_')) {
            pools.push({ co: 'anthropic', code: rowCode(key, config.label), name: config.label, pct: clamp(value.utilization), color: CODE_COLORS[config.color] || '#d946ef', burnKey: key });
        } else if (key.startsWith('cc_')) {
            pools.push({ co: 'anthropic', cli: true, code: rowCode(key, config.label), name: config.label, pct: clamp(value.utilization), color: CODE_COLORS[config.color] || CODE_COLORS.cc, burnKey: key });
        }
    }
    for (const lim of (data.codex?.limits || [])) {
        if (hiddenRows['codex_' + lim.key]) continue;
        pools.push({ co: 'openai', code: rowCode('codex_' + lim.key, lim.label), name: lim.label, pct: clamp(lim.percent), color: CODE_COLORS.codex, burnKey: 'codex_' + lim.key });
    }
    for (const lim of (data.codex?.cli?.limits || [])) {
        if (hiddenRows['codex_cli_' + lim.key]) continue;
        pools.push({ co: 'openai', cli: true, code: rowCode('codex_' + lim.key, lim.label), name: 'CLI ' + lim.label, pct: clamp(lim.percent), color: CODE_COLORS.codex, burnKey: 'codex_cli_' + lim.key });
    }
    // OpenAI banked limit-resets have no percentage to fill, so compact shows
    // the orbs themselves in place of a bar rather than omitting the pool.
    const compactResets = data.codex?.resetCredits;
    if (compactResets && compactResets.available > 0
        && (data.codex?.limits || []).length && !hiddenRows['codex_row_resets']) {
        // Compact has no room per orb — summarise every expiry in the tooltip
        const rstCredits = Array.isArray(compactResets.credits) ? compactResets.credits : [];
        const rstName = rstCredits.length
            ? 'Limit Resets\n' + rstCredits.map((c, i) => c.expiresAt
                ? `${i + 1}. expires in ${formatCountdown(Math.max(c.expiresAt - Date.now(), 0))}`
                : `${i + 1}. no expiry reported`).join('\n')
            : 'Limit Resets';
        pools.push({ co: 'openai', code: 'RST', name: rstName,
            pct: 0, color: CODE_COLORS.codex, orbs: Math.min(compactResets.available, 12) });
    }
    // The second (CLI) account banks its own resets — the wide view shows
    // both; compact only ever read the desktop account's.
    const cliResets = data.codex?.cli?.resetCredits;
    if (cliResets && cliResets.available > 0
        && (data.codex?.cli?.limits || []).length && !hiddenRows['codex_cli_row_resets']) {
        const credits = Array.isArray(cliResets.credits) ? cliResets.credits : [];
        const name = credits.length
            ? 'Limit Resets\n' + credits.map((c, i) => c.expiresAt
                ? `${i + 1}. expires in ${formatCountdown(Math.max(c.expiresAt - Date.now(), 0))}`
                : `${i + 1}. no expiry reported`).join('\n')
            : 'Limit Resets';
        pools.push({ co: 'openai', cli: true, code: 'RST', name: 'CLI ' + name,
            pct: 0, color: CODE_COLORS.codex, orbs: Math.min(cliResets.available, 12) });
    }
    (data.gemini?.limits || []).forEach((lim, i) => {
        if (hiddenRows['gemini_' + lim.key]) return;
        pools.push({ co: 'google', code: rowCode('gemini_' + lim.key, lim.label), name: lim.label, pct: clamp(lim.percent), color: GEMINI_BLUES[i % GEMINI_BLUES.length], burnKey: 'gemini_' + lim.key });
    });
    (data.gemini?.cli?.limits || []).forEach((lim, i) => {
        if (hiddenRows['gemini_cli_' + lim.key]) return;
        pools.push({ co: 'google', cli: true, code: rowCode('gemini_' + lim.key, lim.label), name: 'CLI ' + lim.label, pct: clamp(lim.percent), color: GEMINI_BLUES[i % GEMINI_BLUES.length], burnKey: 'gemini_cli_' + lim.key });
    });

    // ONE visibility model across every layout. Compact reads the same keys
    // the full/wide views write (sectionCollapsed, subgroupHidden,
    // hiddenProviders, hiddenRows) and its own chips write those same keys
    // back, so a roll-up or account-hide made in either layout is already
    // in effect when you switch to the other.
    const _settings = window._cachedSettings || {};
    const _sub = _settings.subgroupHidden || {};
    const _sec = _settings.sectionCollapsed || {};
    const _perma = _settings.hiddenProviders || {};
    const coOrder = { anthropic: 0, openai: 1, google: 2 };
    const livePools = pools.filter((p) => !_perma[p.co]);
    const visiblePools = livePools.filter((p) =>
        !_sec[p.co] && !(p.cli ? _sub[p.co + '_cli'] : _sub[p.co + '_desktop']));

    // Rank-by-use inside each company block (grouping preserved)
    if (_settings.sortByUsage) {
        visiblePools.sort((a, b) => (coOrder[a.co] - coOrder[b.co]) || (b.pct - a.pct));
    }

    // Option A — account ribbons. One thin label row per tracked account
    // (provider · email · CLI badge) with that account's pools beneath it.
    // Ribbons are also compact's hide/restore controls: clicking one toggles
    // the same setting the full view's chevron / DESKTOP-CLI pill writes, so
    // the two layouts never disagree. A hidden account keeps its ribbon
    // (dimmed) — that is the way back from compact.
    const emailOf = {
        anthropic: { desk: data.anthropic_email || null, cli: data.claude_code?.email || null },
        openai: { desk: data.codex?.email || null, cli: data.codex?.cli?.email || null },
        google: { desk: data.gemini?.email || null, cli: data.gemini?.cli?.email || null }
    };
    const providerName = { anthropic: 'Anthropic', openai: 'OpenAI', google: 'Google' };
    let ribbonCount = 0;
    const buildRibbon = (co, side, dual) => {
        const isCli = side === 'cli';
        const collapsed = !!_sec[co];
        const hidden = collapsed || !!_sub[co + '_' + side];
        const ribbon = document.createElement('button');
        ribbon.className = 'compact-ribbon' + (hidden ? ' off' : '');
        ribbon.style.setProperty('--co', COMPANY_COLORS[co]);
        const prov = document.createElement('span');
        prov.className = 'compact-ribbon-provider';
        prov.textContent = providerName[co];
        ribbon.appendChild(prov);
        const email = emailOf[co][isCli ? 'cli' : 'desk'];
        const who = document.createElement('span');
        who.className = 'compact-ribbon-email';
        who.textContent = email || (isCli ? 'CLI login' : 'signed-in account');
        ribbon.appendChild(who);
        if (isCli && dual) {
            const badge = document.createElement('span');
            badge.className = 'compact-ribbon-badge';
            badge.textContent = 'CLI';
            ribbon.appendChild(badge);
        }
        ribbon.title = hidden ? 'Hidden here and in the full view \u2014 click to show'
            : 'Click to hide this account, here and in the full view';
        ribbon.addEventListener('click', async (e) => {
            e.stopPropagation();
            const settings = window._cachedSettings || await window.electronAPI.getSettings();
            if (collapsed) {
                await _saveSettingsPatch({ sectionCollapsed: { ...(settings.sectionCollapsed || {}), [co]: false } });
            } else if (dual) {
                const key = co + '_' + side;
                await _saveSettingsPatch({ subgroupHidden: { ...(settings.subgroupHidden || {}), [key]: !_sub[key] } });
            } else {
                await _saveSettingsPatch({ sectionCollapsed: { ...(settings.sectionCollapsed || {}), [co]: true } });
            }
            applySectionStates(window._cachedSettings);
            applySubgroups();
            if (latestUsageData) updateUI(latestUsageData);
        });
        ribbonCount++;
        return ribbon;
    };

    container.innerHTML = '';
    for (const co of Object.keys(coOrder)) {
        const hasDesk = livePools.some((p) => p.co === co && !p.cli);
        const hasCli = livePools.some((p) => p.co === co && p.cli);
        const dual = hasDesk && hasCli;
        for (const side of ['desktop', 'cli']) {
            if (side === 'desktop' ? !hasDesk : !hasCli) continue;
            container.appendChild(buildRibbon(co, side, dual));
            for (const p of visiblePools.filter((q) => q.co === co && (side === 'cli') === !!q.cli)) {
        const row = document.createElement('div');
        row.className = 'compact-row';
        row.style.setProperty('--co', COMPANY_COLORS[p.co]);
        row.title = p.name.replace(/^CLI /, '');

        const labelEl = document.createElement('span');
        labelEl.className = 'compact-label';
        labelEl.style.color = p.color;
        labelEl.textContent = p.code;
        row.appendChild(labelEl);

        const wrap = document.createElement('div');
        wrap.className = 'compact-bar-wrap';

        // The reset bank renders as orbs, not a bar — same markup as the full
        // view, so it inherits the glimmer and the pixel fire for free.
        if (p.orbs) {
            row.classList.add('compact-orbs');
            const dots = document.createElement('div');
            dots.className = 'reset-dots' + (p.orbs === 1 ? ' single' : '');
            for (let i = 0; i < p.orbs; i++) {
                const dot = document.createElement('span');
                dot.className = 'reset-dot';
                dot.style.animationDelay = `${(i * 1.3 + 0.4).toFixed(1)}s`;
                dots.appendChild(dot);
            }
            wrap.appendChild(dots);
            row.appendChild(wrap);
            container.appendChild(row);
            continue;
        }

        const bg = document.createElement('div');
        bg.className = 'compact-bar-bg';
        const fill = document.createElement('div');
        fill.className = 'compact-bar-fill';
        fill.style.width = `${p.pct}%`;
        const barCol = p.pct >= dangerThreshold ? '#ef4444'
            : p.pct >= warnThreshold ? '#f59e0b' : p.color;
        fill.style.background = barCol;
        // Compact bars carry the same burn-detector fire, tinted with the
        // exact colour the fill just received (threshold recolours included).
        // Pass the hex we chose, not fill.style.background — reading that back
        // yields "rgb(r, g, b)", which the palette used to reject outright.
        if (p.burnKey && _burningRowKeys.has(p.burnKey)) {
            fill.classList.add('on-fire');
            fill.style.setProperty('--fire-col', barCol);
        }
        applyMaxedState(fill, p.pct);
        bg.appendChild(fill);
        const pctEl = document.createElement('span');
        pctEl.className = 'compact-pct';
        pctEl.textContent = `${Math.round(p.pct)}%`;
        bg.appendChild(pctEl);
        wrap.appendChild(bg);
        row.appendChild(wrap);
        container.appendChild(row);
            }
        }
    }

    // the rows container stretches to fill the window (so bars can expand
    // when the user makes it bigger), which makes measuring it circular.
    if (isCompactMode) {
        window.electronAPI.resizeWindow(_chromeHeight() + visiblePools.length * 26 + ribbonCount * 15 + 18);
    }
}
// Persist compact mode setting without touching the rest of settings — debounced
let _saveCompactTimer = null;
async function _saveCompactSetting(compact) {
    if (_saveCompactTimer) clearTimeout(_saveCompactTimer);
    _saveCompactTimer = setTimeout(async () => {
        const settings = window._cachedSettings || await window.electronAPI.getSettings();
        settings.compactMode = compact;
        window._cachedSettings = settings;
        await window.electronAPI.saveSettings(settings);
    }, 300);
}

// Persist graph/expanded visibility state — debounced to avoid hammering disk on rapid toggles
let _saveViewStateTimer = null;
async function _saveViewState() {
    if (appInitializing) return;
    if (_saveViewStateTimer) clearTimeout(_saveViewStateTimer);
    _saveViewStateTimer = setTimeout(async () => {
        const settings = window._cachedSettings || await window.electronAPI.getSettings();
        settings.graphVisible = graphVisible;
        settings.expandedOpen = isExpanded;
        window._cachedSettings = settings;
        await window.electronAPI.saveSettings(settings);
    }, 300);
}

let sessionResetTriggered = false;
let weeklyResetTriggered = false;
let isFirstDataLoad = true; // used to seed alert flags on startup

// Track which usage alert thresholds have already fired this window
// Prevents repeat notifications on every refresh cycle
// Keys: 'session_warn', 'session_danger', 'weekly_warn', 'weekly_danger'
// Seeded on startup so thresholds already exceeded at launch don't fire immediately
const alertFired = {
    session_warn: false,
    session_danger: false,
    weekly_warn: false,
    weekly_danger: false
};

// Seed alertFired flags based on current utilization at startup.
// Any threshold already exceeded when the app launches is treated as already fired,
// so the user doesn't get a notification for something they can already see.
function seedAlertFlags(data) {
    const sessionPct = data.five_hour?.utilization || 0;
    const weeklyPct = data.seven_day?.utilization || 0;

    if (sessionPct >= dangerThreshold) {
        alertFired.session_danger = true;
        alertFired.session_warn = true;
    } else if (sessionPct >= warnThreshold) {
        alertFired.session_warn = true;
    }

    if (weeklyPct >= dangerThreshold) {
        alertFired.weekly_danger = true;
        alertFired.weekly_warn = true;
    } else if (weeklyPct >= warnThreshold) {
        alertFired.weekly_warn = true;
    }

    // Scoped weekly limits (e.g. Fable): thresholds already exceeded at launch
    // are treated as fired so startup doesn't notify about visible state
    for (const key of Object.keys(EXTRA_ROW_CONFIG)) {
        if (!key.startsWith('seven_day_scoped_')) continue;
        const pct = data[key]?.utilization;
        if (pct == null) continue;
        if (pct >= 99) alertFired[`${key}_maxed`] = true;
        if (pct >= dangerThreshold) alertFired[`${key}_danger`] = true;
        if (pct >= warnThreshold) alertFired[`${key}_warn`] = true;
    }
}

function refreshTimers() {
    if (!latestUsageData) return;

    const settings = window._cachedSettings || {};
    const timeFormat = settings.timeFormat || '12h';
    const weeklyDateFormat = settings.weeklyDateFormat || 'date';

    // Session data
    const sessionUtilization = latestUsageData.five_hour?.utilization || 0;
    const sessionResetsAt = latestUsageData.five_hour?.resets_at;

    // Check if session timer has expired and we need to refresh
    if (sessionResetsAt) {
        const sessionDiff = new Date(sessionResetsAt) - new Date();
        if (sessionDiff <= 0 && !sessionResetTriggered) {
            sessionResetTriggered = true;
            debugLog('Session timer expired, triggering refresh...');
            // Wait a few seconds for the server to update, then refresh
            setTimeout(() => {
                fetchUsageData();
                checkForUpdate();
            }, 3000);
        } else if (sessionDiff > 0) {
            sessionResetTriggered = false; // Reset flag when timer is active again
        }
    }

    updateProgressBar(
        elements.sessionProgress,
        elements.sessionPercentage,
        sessionUtilization
    );

    updateTimer(
        elements.sessionTimer,
        elements.sessionTimeText,
        sessionResetsAt,
        5 * 60 // 5 hours in minutes
    );
    elements.sessionResetsAt.textContent = formatResetsAt(sessionResetsAt, false, timeFormat, weeklyDateFormat);
    elements.sessionResetsAt.style.opacity = sessionResetsAt ? '1' : '0.4';

    // Weekly data
    const weeklyUtilization = latestUsageData.seven_day?.utilization || 0;
    const weeklyResetsAt = latestUsageData.seven_day?.resets_at;

    // Check if weekly timer has expired and we need to refresh
    if (weeklyResetsAt) {
        const weeklyDiff = new Date(weeklyResetsAt) - new Date();
        if (weeklyDiff <= 0 && !weeklyResetTriggered) {
            weeklyResetTriggered = true;
            debugLog('Weekly timer expired, triggering refresh...');
            setTimeout(() => {
                fetchUsageData();
            }, 3000);
        } else if (weeklyDiff > 0) {
            weeklyResetTriggered = false;
        }
    }

    updateProgressBar(
        elements.weeklyProgress,
        elements.weeklyPercentage,
        weeklyUtilization,
        true
    );

    updateTimer(
        elements.weeklyTimer,
        elements.weeklyTimeText,
        weeklyResetsAt,
        7 * 24 * 60 // 7 days in minutes
    );
    elements.weeklyResetsAt.textContent = formatResetsAt(weeklyResetsAt, true, timeFormat, weeklyDateFormat);
    elements.weeklyResetsAt.style.opacity = weeklyResetsAt ? '1' : '0.4';
}

function startCountdown() {
    if (countdownInterval) clearInterval(countdownInterval);
    countdownInterval = setInterval(() => {
        refreshTimers();
        refreshExtraTimers(); // pinned scoped rows tick even when collapsed
    }, 30000);
}

// Update progress bar
function updateProgressBar(progressElement, percentageElement, value, isWeekly = false) {
    const percentage = Math.min(Math.max(value, 0), 100);

    progressElement.style.width = `${percentage}%`;
    percentageElement.textContent = `${Math.round(percentage)}%`;

    progressElement.classList.remove('warning', 'danger');
    if (percentage >= dangerThreshold) {
        progressElement.classList.add('danger');
    } else if (percentage >= warnThreshold) {
        progressElement.classList.add('warning');
    }
    applyMaxedState(progressElement, percentage);
}

// A pool at 100% is spent: the bar goes black and smoulders. Two offset
// puff layers are mounted inside the fill so the smoke spans its full
// width; they're torn down again the moment it drops below 100.
function applyMaxedState(fillElement, percentage) {
    if (!fillElement) return;
    const maxed = percentage >= 100;
    if (maxed === fillElement.classList.contains('maxed')) return;
    fillElement.classList.toggle('maxed', maxed);
    if (!maxed) {
        fillElement.querySelectorAll('.maxed-smoke').forEach((el) => el.remove());
        fillElement.__smokeLayer = null;
    }
}

// Charred-bar smoke, drawn with the same pixel sprites the fire uses so it
// matches the app's pixel-art language. Greys are flipped per theme — pale
// smoke disappears against the light theme's near-white panel.
function _spawnMaxedSmoke(layer, x) {
    if (layer.childElementCount > 55) return;
    const light = document.body.classList.contains('theme-light');
    const palette = light
        ? ['#4a4a56', '#5c5c6a', '#38384a', '#6a6a78']
        : ['#8e8e9e', '#a2a2b2', '#76768a', '#b4b4c2'];
    const size = Math.random() < 0.55 ? 3 : 2;
    const s = document.createElement('div');
    s.className = 'fp';
    const life = Math.round(_rnd(900, 1500));
    s.style.cssText = `left:${x}px; bottom:${_rnd(0, 4)}px; width:${size}px; height:${size}px;
        background:${_pick(palette)}; opacity:${(0.55 + Math.random() * 0.4).toFixed(2)};
        --dx:${Math.round(_rnd(-3, 4)) * 3}px; --ry:${-Math.round(_rnd(6, 12)) * 3}px;
        animation: pxSmoke ${life}ms steps(7) forwards;`;
    layer.appendChild(s);
    setTimeout(() => s.remove(), life + 90);
}

function _tickMaxedSmoke(el, now) {
    let layer = el.__smokeLayer;
    if (!layer || layer.parentNode !== el) {
        layer = document.createElement('div');
        layer.className = 'maxed-smoke';
        el.appendChild(layer);
        el.__smokeLayer = layer;
        layer.__lastSpawn = 0;
    }
    if (now - layer.__lastSpawn < 110 * (0.7 + Math.random() * 0.8)) return;
    layer.__lastSpawn = now;
    const width = Math.max(4, el.offsetWidth - 4);
    // Spread across the whole width — a spent bar smoulders end to end
    const count = 1 + Math.floor(Math.random() * 2);
    for (let i = 0; i < count; i++) _spawnMaxedSmoke(layer, _rnd(0, width));
}

// "6d 22h" / "4h 29m" / "18m" — the Resets In column's house style, shared so
// the banked-reset expiry countdown reads identically to every other row.
function formatCountdown(diffMs) {
    const hours = Math.floor(diffMs / (1000 * 60 * 60));
    const minutes = Math.floor((diffMs % (1000 * 60 * 60)) / (1000 * 60));
    if (hours >= 24) return `${Math.floor(hours / 24)}d ${hours % 24}h`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
}

// Format reset date for the "Resets At" column
// Session: shows time like "3:59 PM" or "15:59"
// Weekly: shows date like "Mar 13", "Fri Mar 13", or "Fri Mar 13 3:59 PM"
function formatResetsAt(resetsAt, isWeekly, timeFormat, weeklyDateFormat) {
    if (!resetsAt) return '—';
    const date = new Date(resetsAt);
    const days = ['Sun','Mon','Tue','Wed','Thu','Fri','Sat'];
    const months = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];

    const formatTime = (d) => {
        if (timeFormat === '24h') {
            return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
        } else {
            let hours = d.getHours();
            const minutes = d.getMinutes().toString().padStart(2, '0');
            const ampm = hours >= 12 ? 'PM' : 'AM';
            hours = hours % 12 || 12;
            return `${hours}:${minutes} ${ampm}`;
        }
    };

    if (isWeekly) {
        const dayStr = days[date.getDay()];
        const monthStr = months[date.getMonth()];
        const dayNum = date.getDate();
        const fmt = weeklyDateFormat || 'date';
        if (fmt === 'date-day') return `${dayStr} ${monthStr} ${dayNum}`;
        if (fmt === 'date-day-time') return `${dayStr} ${monthStr} ${dayNum} ${formatTime(date)}`;
        return `${monthStr} ${dayNum}`; // default: 'date'
    } else {
        return formatTime(date);
    }
}

// A whimsical gold sparkle burst around an elapsed ring — fired when its
// window completes, as if an unseen Tinkerbell tapped it with her wand
function sparkleRing(timerElement) {
    if (document.visibilityState !== 'visible') {
        // Reset happened while we weren't being looked at — owe a burst,
        // paid out when the window comes back into view
        timerElement.dataset.pendingSparkle = '1';
        return;
    }
    if (document.body.classList.contains('no-pizazz')) return; // clown's in jail
    const anchor = timerElement.closest('.usage-elapsed-group') || timerElement.parentElement;
    if (!anchor || anchor.querySelector('.sparkle-burst')) return;
    anchor.style.position = 'relative';

    const burst = document.createElement('div');
    burst.className = 'sparkle-burst';
    const glyphs = ['✦', '✧', '✨', '⋆', '✦'];
    const colors = ['#ffd700', '#fff3b0', '#ffe066', '#ffffff', '#ffec99'];
    // A window reset is a rare event — make the burst lavish: double the
    // sparkles, born all around the ring's edge, flying further out
    const count = 28;
    const ringRadius = 10;
    for (let i = 0; i < count; i++) {
        const s = document.createElement('span');
        s.className = 'sparkle';
        s.textContent = glyphs[i % glyphs.length];
        const angle = (Math.PI * 2 * i) / count + Math.random() * 0.4;
        const dist = ringRadius + 12 + Math.random() * 24;
        s.style.setProperty('--sx', `${(Math.cos(angle) * ringRadius).toFixed(1)}px`);
        s.style.setProperty('--sy', `${(Math.sin(angle) * ringRadius).toFixed(1)}px`);
        s.style.setProperty('--dx', `${(Math.cos(angle) * dist).toFixed(1)}px`);
        s.style.setProperty('--dy', `${(Math.sin(angle) * dist).toFixed(1)}px`);
        s.style.color = colors[i % colors.length];
        s.style.animationDelay = `${(Math.random() * 0.45).toFixed(2)}s`;
        s.style.fontSize = `${Math.round(5 + Math.random() * 6)}px`;
        burst.appendChild(s);
    }
    anchor.appendChild(burst);
    setTimeout(() => burst.remove(), 1900);
}

// Swap the psychic between idle (staring) and trance (beaming) states
function applyPsychicState() {
    if (!elements.psychicBtn) return;
    elements.psychicBtn.classList.toggle('on', projectionsVisible);
    elements.psychicImg.src = projectionsVisible
        ? '../../assets/psychic-on.png'
        : '../../assets/psychic-idle.png';
    elements.psychicBtn.title = projectionsVisible
        ? 'Forecast projections: ON — the psychic sees your future burn'
        : 'Forecast projections: OFF — the psychic awaits your command';
}

// Update circular timer
function updateTimer(timerElement, textElement, resetsAt, totalMinutes) {
    if (!resetsAt) {
        textElement.textContent = 'Not started';
        textElement.style.opacity = '0.4';
        textElement.style.fontSize = '10px';
        textElement.title = 'Starts when a message is sent';
        timerElement.style.strokeDashoffset = 63;
        timerElement.style.stroke = '';
        return;
    }

    // Clear the greyed out styling when timer is active
    textElement.style.opacity = '1';
    textElement.style.fontSize = '';
    textElement.title = '';

    // A genuinely NEW window (reset time jumped forward) — sparkle once.
    // Strict inequality is not enough: the API stamps resets_at with
    // sub-second jitter on every response, so require a real forward jump.
    const prevResets = Date.parse(timerElement.dataset.lastResets || '');
    const curResets = Date.parse(resetsAt);
    if (!isNaN(prevResets) && !isNaN(curResets) && curResets - prevResets > 60000) {
        sparkleRing(timerElement);
    }
    timerElement.dataset.lastResets = resetsAt;

    const resetDate = new Date(resetsAt);
    const now = new Date();
    const diff = resetDate - now;

    if (diff <= 0) {
        textElement.textContent = 'Resetting...';
        timerElement.style.strokeDashoffset = 0;
        timerElement.style.stroke = 'hsl(142, 70%, 47%)'; // full green — reset imminent
        // The circle just completed — one wand-tap per window. Tolerant
        // dedupe: resets_at jitters sub-second between responses.
        const sparkled = Date.parse(timerElement.dataset.sparkledFor || '');
        if (isNaN(sparkled) || Math.abs(curResets - sparkled) > 60000) {
            timerElement.dataset.sparkledFor = resetsAt;
            sparkleRing(timerElement);
        }
        return;
    }

    // Calculate remaining time
    const hours = Math.floor(diff / (1000 * 60 * 60));
    const minutes = Math.floor((diff % (1000 * 60 * 60)) / (1000 * 60));
    // const seconds = Math.floor((diff % (1000 * 60)) / 1000); // Optional seconds

    textElement.textContent = formatCountdown(diff);

    // Calculate progress (elapsed percentage)
    const totalMs = totalMinutes * 60 * 1000;
    const elapsedMs = totalMs - diff;
    const elapsedPercentage = (elapsedMs / totalMs) * 100;

    // Update circle (63 is ~2*pi*10)
    const circumference = 63;
    const offset = circumference - (elapsedPercentage / 100) * circumference;
    timerElement.style.strokeDashoffset = offset;

    // Colour brightens toward green as the window nears its reset — elapsed
    // time is GOOD news (reset means fresh usage), so no red alarm here.
    // Slate → teal → green as the circle closes.
    const f = Math.min(Math.max(elapsedPercentage / 100, 0), 1);
    const hue = 222 - (222 - 142) * f;
    const sat = 12 + (70 - 12) * f;
    const light = 55 - (55 - 47) * f;
    timerElement.classList.remove('warning', 'danger');
    timerElement.style.stroke = `hsl(${Math.round(hue)}, ${Math.round(sat)}%, ${Math.round(light)}%)`;

    // At very narrow widths CSS swaps this text for a remaining-time ring —
    // feed it the fraction and colour, and keep the words in the tooltip
    const remFrac = Math.min(Math.max(diff / totalMs, 0), 1);
    textElement.style.setProperty('--rem-deg', `${Math.round(remFrac * 360)}deg`);
    textElement.style.setProperty('--rem-col', `hsl(${Math.round(hue)}, ${Math.round(sat)}%, ${Math.round(light)}%)`);
    textElement.title = `Resets in ${textElement.textContent}`;
}

// UI State Management
function showLoginRequired() {
    elements.loadingContainer.style.display = 'none';
    elements.noUsageContainer.style.display = 'none';
    elements.mainContent.style.display = isCompactMode ? 'none' : 'block';
    // Deliberately do NOT close the settings overlay here: logging out lands
    // in this path when no fallback exists, and yanking the user out of
    // Settings made the logout look like a silent malfunction.
    // Logged out — the stale-session notice no longer applies
    const staleBanner = document.getElementById('staleBanner');
    if (staleBanner) staleBanner.style.display = 'none';
    // Keep recovery and configuration controls available while logged out.
    elements.settingsBtn.style.display = 'flex';
    elements.refreshBtn.style.display = 'flex';
    elements.graphBtn.style.display = isCompactMode ? 'none' : 'flex';
    stopAutoUpdate();
    if (countdownInterval) {
        clearInterval(countdownInterval);
        countdownInterval = null;
    }
    // Reset fetch guard so it can't get permanently stuck across login/logout
    isFetching = false;
    // Reset alert state so a new session doesn't inherit suppressed alerts
    isFirstDataLoad = true;
    alertFired.session_warn = false;
    alertFired.session_danger = false;
    alertFired.weekly_warn = false;
    alertFired.weekly_danger = false;
    // Keep the provider dashboard and Settings entry visible while logged out.
    if (!isCompactMode) resizeWidget();
}

function showMainContent() {
    elements.loadingContainer.style.display = 'none';
    elements.noUsageContainer.style.display = 'none';
    // Respect compact mode — don't force mainContent visible if we're in compact
    if (!isCompactMode) {
        elements.mainContent.style.display = 'block';
    }
    elements.compactContent.style.display = isCompactMode ? 'flex' : 'none';
    // Restore header buttons after login - but respect compact mode for graph button
    elements.settingsBtn.style.display = 'flex';
    elements.refreshBtn.style.display = 'flex';
    elements.graphBtn.style.display = isCompactMode ? 'none' : 'flex';
}

// Auto-update management
function startAutoUpdate() {
    stopAutoUpdate();
    const settings = window._cachedSettings || {};
    const intervalSecs = parseInt(settings.refreshInterval) || 300;
    updateInterval = setInterval(async () => {
        if (elements.refreshBtn) elements.refreshBtn.classList.add('spinning');
        await fetchUsageData();
        if (elements.refreshBtn) elements.refreshBtn.classList.remove('spinning');
    }, intervalSecs * 1000);
}

function stopAutoUpdate() {
    if (updateInterval) {
        clearInterval(updateInterval);
        updateInterval = null;
    }
}

async function loadChart() {
    const history = await window.electronAPI.getUsageHistory();
    if (!history.length) return;
    renderChart(history);
}

function renderChart(history) {
    if (usageChart) usageChart.destroy();

    const showSonnet = isExpanded && !!latestUsageData?.seven_day_sonnet;
    const showOpus = isExpanded && !!latestUsageData?.seven_day_opus;
    const showCowork = isExpanded && !!latestUsageData?.seven_day_cowork;
    const showDesign = isExpanded && !!latestUsageData?.seven_day_omelette;
    const showOAuthApps = isExpanded && !!latestUsageData?.seven_day_oauth_apps;
    const showExtraUsage = isExpanded && !!latestUsageData?.extra_usage;
    // Scoped weekly series (e.g. Fable) recorded by main.js under entry.scoped.
    // Not gated on isExpanded — the scoped bar is pinned to the main view.
    const scopedKeys = [];
    {
        const seen = new Set();
        for (const entry of history) {
            for (const key of Object.keys(entry.scoped || {})) {
                if (!seen.has(key)) { seen.add(key); scopedKeys.push(key); }
            }
        }
    }
    const allValues = history.flatMap((entry) => {
        const values = [entry.session, entry.weekly];
        if (showSonnet) values.push(entry.sonnet);
        if (showOpus) values.push(entry.opus);
        if (showCowork) values.push(entry.cowork);
        if (showDesign) values.push(entry.design);
        if (showOAuthApps) values.push(entry.oauthApps);
        if (showExtraUsage) values.push(entry.extraUsage);
        for (const key of scopedKeys) values.push(entry.scoped?.[key]);
        values.push(entry.codex, entry.gemini, entry.codexCli, entry.geminiCli, entry.claudeCli);
        return values.map(chartUtils.finiteOrNull).filter((value) => value != null);
    });
    let yMax = Math.max(10, Math.ceil(Math.max(0, ...allValues) / 10) * 10);

    const datasets = [
        {
            seriesId: 'claude:session',
            label: 'CLA 5H',
            data: history.map((entry) => chartUtils.point(entry.timestamp, entry.session)),
            borderColor: '#8b5cf6',
            backgroundColor: 'transparent',
            borderWidth: 2,
            stepped: true,
            pointRadius: 0,
            pointHoverRadius: 3,
            pointHitRadius: 10
        },
        {
            seriesId: 'claude:weekly',
            label: 'CLA 7D',
            data: history.map((entry) => chartUtils.point(entry.timestamp, entry.weekly)),
            borderColor: '#3b82f6',
            backgroundColor: 'transparent',
            borderWidth: 2,
            stepped: true,
            pointRadius: 0,
            pointHoverRadius: 3,
            pointHitRadius: 10
        }
    ];

    if (showSonnet) {
        const sonnetData = history.map((entry) => chartUtils.finiteOrNull(entry.sonnet));
        if (chartUtils.hasPositive(sonnetData)) {
            datasets.push({
                seriesId: 'anthropic:sonnet',
                label: 'Sonnet',
                data: history.map((entry) => chartUtils.point(entry.timestamp, entry.sonnet)),
                borderColor: '#f43f5e',
                backgroundColor: 'transparent',
                borderWidth: 2,
                stepped: true,
                pointRadius: 0,
                pointHoverRadius: 3,
                pointHitRadius: 10
            });
        }
    }

    if (showOpus) {
        const opusData = history.map((entry) => chartUtils.finiteOrNull(entry.opus));
        if (chartUtils.hasPositive(opusData)) {
            datasets.push({
                seriesId: 'anthropic:opus',
                label: 'Opus',
                data: history.map((entry) => chartUtils.point(entry.timestamp, entry.opus)),
                borderColor: '#f59e0b',
                backgroundColor: 'transparent',
                borderWidth: 2,
                stepped: true,
                pointRadius: 0,
                pointHoverRadius: 3,
                pointHitRadius: 10
            });
        }
    }

    if (showCowork) {
        const coworkData = history.map((entry) => chartUtils.finiteOrNull(entry.cowork));
        if (chartUtils.hasPositive(coworkData)) {
            datasets.push({
                seriesId: 'anthropic:cowork',
                label: 'Cowork',
                data: history.map((entry) => chartUtils.point(entry.timestamp, entry.cowork)),
                borderColor: '#06b6d4',
                backgroundColor: 'transparent',
                borderWidth: 2,
                stepped: true,
                pointRadius: 0,
                pointHoverRadius: 3,
                pointHitRadius: 10
            });
        }
    }

    if (showDesign) {
        const designData = history.map((entry) => chartUtils.finiteOrNull(entry.design));
        if (chartUtils.hasPositive(designData)) {
            datasets.push({
                seriesId: 'anthropic:design',
                label: 'Design',
                data: history.map((entry) => chartUtils.point(entry.timestamp, entry.design)),
                borderColor: '#92400e',
                backgroundColor: 'transparent',
                borderWidth: 2,
                stepped: true,
                pointRadius: 0,
                pointHoverRadius: 3,
                pointHitRadius: 10
            });
        }
    }

    if (showOAuthApps) {
        const oauthAppsData = history.map((entry) => chartUtils.finiteOrNull(entry.oauthApps));
        if (chartUtils.hasPositive(oauthAppsData)) {
            datasets.push({
                seriesId: 'anthropic:oauth-apps',
                label: 'OAuth Apps',
                data: history.map((entry) => chartUtils.point(entry.timestamp, entry.oauthApps)),
                borderColor: '#f97316',
                backgroundColor: 'transparent',
                borderWidth: 2,
                stepped: true,
                pointRadius: 0,
                pointHoverRadius: 3,
                pointHitRadius: 10
            });
        }
    }

    if (showExtraUsage) {
        const extraUsageData = history.map((entry) => chartUtils.finiteOrNull(entry.extraUsage));
        if (chartUtils.hasPositive(extraUsageData)) {
            datasets.push({
            seriesId: 'anthropic:extra-usage',
            label: 'Extra Usage',
            data: history.map((entry) => chartUtils.point(entry.timestamp, entry.extraUsage)),
            borderColor: '#f59e0b',
            backgroundColor: 'transparent',
            borderWidth: 2,
            stepped: true,
            pointRadius: 0,
            pointHoverRadius: 3,
            pointHitRadius: 10
            });
        }
    }

    // Dynamic series for scoped weekly limits (e.g. Fable), matching the
    // per-model rows built by normalizeUsageData()
    const SCOPED_CHART_COLORS = { fable: '#d946ef' };
    const SCOPED_FALLBACK_COLORS = ['#84cc16', '#14b8a6', '#a855f7', '#64748b'];
    let scopedColorIndex = 0;
    for (const key of scopedKeys) {
        const scopedData = history.map((entry) => chartUtils.finiteOrNull(entry.scoped?.[key]));
        if (!chartUtils.hasPositive(scopedData)) continue;
        const label = key.split('_')
            .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
            .join(' ');
        const borderColor = SCOPED_CHART_COLORS[key]
            || SCOPED_FALLBACK_COLORS[scopedColorIndex++ % SCOPED_FALLBACK_COLORS.length];
        datasets.push({
            seriesId: `scoped:${key}`,
            label,
            data: history.map((entry) => chartUtils.point(entry.timestamp, entry.scoped?.[key])),
            borderColor,
            backgroundColor: 'transparent',
            borderWidth: 2,
            stepped: true,
            pointRadius: 0,
            pointHoverRadius: 3,
            pointHitRadius: 10
        });
    }

    // Cross-provider comparison lines — OpenAI (Codex) and Google (Gemini),
    // plus their CLI second accounts and the Claude CLI. All are already
    // 0-100% utilization so they share the y-axis with no normalization.
    // Toggle any of them on/off from the chart legend.
    const PROVIDER_SERIES = [
        { key: 'codex', label: 'Codex', color: CODE_COLORS.codex, dash: null },
        { key: 'gemini', label: 'Gemini', color: COMPANY_COLORS.google, dash: null },
        { key: 'claudeCli', label: 'Claude CLI', color: COMPANY_COLORS.anthropic, dash: [5, 3] },
        { key: 'codexCli', label: 'Codex CLI', color: CODE_COLORS.codex, dash: [5, 3] },
        { key: 'geminiCli', label: 'Gemini CLI', color: COMPANY_COLORS.google, dash: [5, 3] }
    ];
    for (const s of PROVIDER_SERIES) {
        const vals = history.map((entry) => chartUtils.finiteOrNull(entry[s.key]));
        if (!chartUtils.hasPositive(vals)) continue;
        datasets.push({
            seriesId: `provider:${s.key}`,
            label: s.label,
            data: history.map((entry) => chartUtils.point(entry.timestamp, entry[s.key])),
            borderColor: s.color,
            backgroundColor: 'transparent',
            borderWidth: 2,
            borderDash: s.dash || undefined,
            stepped: true,
            pointRadius: 0,
            pointHoverRadius: 3,
            pointHitRadius: 10
        });
    }

    // Burn-rate projections: dotted line from the newest sample to the
    // forecast 100% crossing, plus a grey marker at the weekly reset — the
    // visual race between "when I max out" and "when the window resets".
    const lastEntry = history[history.length - 1];
    let chartXMax = lastEntry.timestamp;
    const forecasts = latestUsageData?.forecasts || {};
    const weeklyResetMs = latestUsageData?.seven_day?.resets_at
        ? new Date(latestUsageData.seven_day.resets_at).getTime() : null;
    const projections = [];
    const addProjection = (seriesId, label, color, lastVal, etaIso, resetMs) => {
        if (!etaIso || lastVal == null) return;
        const eta = new Date(etaIso).getTime();
        if (eta <= lastEntry.timestamp) return;
        if (resetMs && eta > resetMs) return; // window resets first — no cap hit
        projections.push({
            seriesId: `projection:${seriesId}`,
            baseSeriesId: seriesId,
            label: `${label} → 100%`,
            data: [{ x: lastEntry.timestamp, y: lastVal }, { x: eta, y: 100 }],
            borderColor: color,
            backgroundColor: 'transparent',
            borderWidth: 1.5,
            borderDash: [4, 4],
            pointRadius: 0,
            pointHoverRadius: 3,
            pointHitRadius: 10
        });
        chartXMax = Math.max(chartXMax, eta);
    };
    if (projectionsVisible) {
        // A series hidden from the legend hides its dotted projection too (the
        // projection is not a legend item, so there's no other way to hide it).
        const chartHiddenP = (window._cachedSettings || {}).chartHiddenSeries || {};
        const proj = (id, lbl, col, lv, eta, resetMs) => {
            if (!chartHiddenP[id]) addProjection(id, lbl, col, lv, eta, resetMs);
        };
        // Anthropic session (5h) + weekly (7d) + per-model/surface pools.
        proj('claude:session', 'CLA 5H', '#8b5cf6', lastEntry.session, forecasts.session, null);
        proj('claude:weekly', 'CLA 7D', '#3b82f6', lastEntry.weekly, forecasts.weekly, weeklyResetMs);
        proj('anthropic:sonnet', 'Sonnet', '#f43f5e', lastEntry.sonnet, forecasts.sonnet, weeklyResetMs);
        proj('anthropic:opus', 'Opus', '#f59e0b', lastEntry.opus, forecasts.opus, weeklyResetMs);
        proj('anthropic:cowork', 'Cowork', '#06b6d4', lastEntry.cowork, forecasts.cowork, weeklyResetMs);
        proj('anthropic:design', 'Design', '#92400e', lastEntry.design, forecasts.design, weeklyResetMs);
        proj('anthropic:oauth-apps', 'OAuth Apps', '#f97316', lastEntry.oauthApps, forecasts.oauthApps, weeklyResetMs);
        // Scoped weekly pools (e.g. Fable) — label matches the base series.
        const SCOPED_CHART_COLORS = { fable: '#d946ef' };
        for (const key of scopedKeys) {
            const scopedLabel = key.split('_').map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
            const scopedResetIso = latestUsageData?.['seven_day_scoped_' + key]?.resets_at;
            proj(`scoped:${key}`, scopedLabel, SCOPED_CHART_COLORS[key] || '#84cc16', lastEntry.scoped?.[key],
                forecasts.scoped?.[key], scopedResetIso ? new Date(scopedResetIso).getTime() : null);
        }
        // Cross-provider pools have no Claude-style reset window → resetMs=null.
        proj('provider:codex', 'Codex', CODE_COLORS.codex, lastEntry.codex, forecasts.codex, null);
        proj('provider:gemini', 'Gemini', COMPANY_COLORS.google, lastEntry.gemini, forecasts.gemini, null);
        proj('provider:claudeCli', 'Claude CLI', COMPANY_COLORS.anthropic, lastEntry.claudeCli, forecasts.claudeCli, null);
        proj('provider:codexCli', 'Codex CLI', CODE_COLORS.codex, lastEntry.codexCli, forecasts.codexCli, null);
        proj('provider:geminiCli', 'Gemini CLI', COMPANY_COLORS.google, lastEntry.geminiCli, forecasts.geminiCli, null);
    }
    if (projections.length) {
        datasets.push(...projections);
        yMax = Math.max(yMax, 100);
        // Reset marker, if the weekly reset falls inside the projected span
        if (weeklyResetMs && weeklyResetMs > lastEntry.timestamp && weeklyResetMs <= chartXMax + 6 * 60 * 60 * 1000) {
            chartXMax = Math.max(chartXMax, weeklyResetMs);
            datasets.push({
                seriesId: 'marker:weekly-reset',
                label: 'Weekly reset',
                data: [{ x: weeklyResetMs, y: 0 }, { x: weeklyResetMs, y: 100 }],
                borderColor: '#9ca3af',
                backgroundColor: 'transparent',
                borderWidth: 1,
                borderDash: [2, 3],
                pointRadius: 0,
                pointHoverRadius: 3,
                pointHitRadius: 10
            });
        }
        chartXMax = Math.min(chartXMax, Date.now() + 3 * 24 * 60 * 60 * 1000);
    }

    const firstDayMidnight = new Date(history[0].timestamp);
    firstDayMidnight.setHours(0, 0, 0, 0);

    // Apply persisted per-series show/hide (toggled from the chart legend).
    const chartHidden = (window._cachedSettings || {}).chartHiddenSeries || {};
    datasets.forEach((d) => {
        if (chartHidden[d.seriesId] || (d.baseSeriesId && chartHidden[d.baseSeriesId])) d.hidden = true;
    });

    usageChart = new Chart(elements.usageChart.getContext('2d'), {
        type: 'line',
        data: { datasets },
        options: {
            animation: false,
            responsive: true,
            maintainAspectRatio: false,
            spanGaps: false,
            interaction: {
                intersect: false,
                mode: 'nearest'
            },
            scales: {
                x: {
                    type: 'linear',
                    min: firstDayMidnight.getTime(),
                    max: chartXMax,
                    // Multi-day spans get one tick per local midnight — Chart.js's
                    // "nice" numeric steps land at arbitrary times in ms-space and
                    // repeat the same date label. Short spans keep default ticks.
                    afterBuildTicks(axis) {
                        const spanMs = axis.max - axis.min;
                        if (spanMs < 48 * 60 * 60 * 1000) return;
                        const d = new Date(firstDayMidnight.getTime());
                        const ticks = [];
                        while (d.getTime() <= axis.max) {
                            if (d.getTime() >= axis.min) ticks.push({ value: d.getTime() });
                            d.setDate(d.getDate() + 1);
                        }
                        if (ticks.length >= 2) axis.ticks = ticks;
                    },
                    ticks: {
                        maxRotation: 0,
                        minRotation: 0,
                        font: {
                            size: 10
                        },
                        callback(value) {
                            const tf = (window._cachedSettings || {}).timeFormat || '12h';
                            const spanMs = history.length > 1
                                ? history[history.length - 1].timestamp - history[0].timestamp
                                : 0;
                            return formatTimestampTick(value, spanMs, tf);
                        }
                    },
                    grid: {
                        display: false
                    }
                },
                y: {
                    min: 0,
                    max: yMax,
                    ticks: {
                        font: {
                            size: 10
                        },
                        callback: (value) => `${value}%`
                    },
                    grid: {
                        color: 'rgba(255, 255, 255, 0.05)'
                    }
                }
            },
            plugins: {
                legend: {
                    display: true,
                    position: 'bottom',
                    labels: {
                        boxWidth: 8,
                        boxHeight: 8,
                        padding: 6,
                        font: { size: 9 },
                        // Keep the legend to real series — hide the dotted
                        // "→ 100%" projections and the reset marker.
                        filter: (item) => !/→ 100%|reset/i.test(item.text)
                    },
                    // Click a series to show/hide it; the choice persists.
                    onClick: (e, item, legend) => {
                        const ci = legend.chart;
                        const willHide = ci.isDatasetVisible(item.datasetIndex);
                        ci.getDatasetMeta(item.datasetIndex).hidden = willHide ? true : null;
                        ci.update();
                        const key = ci.data.datasets[item.datasetIndex].seriesId;
                        if (!key) return;
                        const chartHiddenSeries = { ...((window._cachedSettings || {}).chartHiddenSeries || {}) };
                        if (willHide) chartHiddenSeries[key] = true; else delete chartHiddenSeries[key];
                        _saveSettingsPatch({ chartHiddenSeries });
                    }
                },
                tooltip: {
                    callbacks: {
                        title(items) {
                            return new Date(items[0].parsed.x).toLocaleString([], {
                                month: 'short',
                                day: 'numeric',
                                hour: 'numeric',
                                minute: '2-digit'
                            });
                        },
                        label(item) {
                            return `${item.dataset.label}: ${Math.round(item.parsed.y)}%`;
                        }
                    }
                }
            }
        }
    });

    explainProjectionState(projections.length);
}

function formatTimestampTick(timestamp, spanMs, timeFormat) {
    return chartUtils.formatTimestampTick(timestamp, spanMs, timeFormat);
}

// An empty trance must explain itself: projections need a recent stretch of
// RISING usage, and a login change orphans the learned history until enough
// fresh samples accumulate. Without this hint, "no dotted lines" reads as
// "projections are broken".
function explainProjectionState(projectionCount) {
    if (!elements.psychicBtn || !projectionsVisible) return;
    const idle = 'Forecast projections: ON — nothing to project yet. Lines appear after ~30 min of rising usage (login changes reset the learning).';
    elements.psychicBtn.title = projectionCount > 0
        ? 'Forecast projections: ON — the psychic sees your future burn'
        : idle;
    if (elements.usageChart) {
        elements.usageChart.title = projectionCount > 0
            ? ''
            : 'No projections yet — they appear after ~30 minutes of rising usage';
    }
}

// Add spinning animation for refresh button
const style = document.createElement('style');
style.textContent = `
    @keyframes spin-refresh {
        from { transform: rotate(0deg); }
        to { transform: rotate(360deg); }
    }
    
    .refresh-btn.spinning svg,
    .local-login-refresh-btn.spinning svg {
        animation: spin-refresh 1s linear infinite;
    }
`;
document.head.appendChild(style);

// Measure the settings panel's full intrinsic height and grow the window to
// it, so no setting is ever cut off at the bottom. `.settings-content` is
// normally height:100% (stretched to the window) with an internal scroll, so
// we briefly un-stretch it to read its true content height, then hand that to
// the main process (which sizes the window and locks resizing).
function fitSettingsWindow() {
    const content = elements.settingsOverlay
        && elements.settingsOverlay.querySelector('.settings-content');
    if (!content) return;
    content.classList.add('measuring');
    const needed = Math.ceil(content.getBoundingClientRect().height) + 2;
    content.classList.remove('measuring');
    if (needed >= 120) window.electronAPI.settingsFit(needed);
}

// Settings management
let warnThreshold = 75;
let dangerThreshold = 90;

async function loadSettings() {
    // Credential state can change outside the renderer (logout, CLI login, or
    // an expired web session), so derive the Anthropic action fresh each time.
    credentials = await window.electronAPI.getCredentials();
    syncAnthropicAuthControls();
    // Warn when the OS keychain is unavailable — stored logins would be
    // sitting in plaintext on disk.
    const plainWarn = document.getElementById('plainStorageWarning');
    if (plainWarn) plainWarn.style.display = credentials.encryptionAvailable === false ? '' : 'none';
    const settings = await window.electronAPI.getSettings();
    window._cachedSettings = settings;
    const isLinux = window.electronAPI.platform === 'linux';
    const isPortable = window.electronAPI.isPortable;
    const autoStartUnsupported = isLinux || isPortable;

    elements.autoStartToggle.checked = autoStartUnsupported ? false : settings.autoStart;
    elements.autoStartToggle.disabled = autoStartUnsupported;
    if (elements.autoStartCol) {
        elements.autoStartCol.classList.toggle('settings-col-disabled', autoStartUnsupported);
    }
    if (elements.autoStartHint) {
        elements.autoStartHint.style.display = autoStartUnsupported ? 'inline' : 'none';
        elements.autoStartHint.textContent = isPortable
            ? 'Not supported in portable mode!'
            : 'Not supported on Linux';
    }
    elements.minimizeToTrayToggle.checked = settings.minimizeToTray;
    elements.alwaysOnTopToggle.checked = settings.alwaysOnTop;
    applySectionStates(settings);
    elements.warnThreshold.value = settings.warnThreshold;
    elements.dangerThreshold.value = settings.dangerThreshold;
    elements.timeFormat.value = settings.timeFormat || '12h';
    elements.weeklyDateFormat.value = settings.weeklyDateFormat || 'date';
    if (elements.refreshInterval) elements.refreshInterval.value = settings.refreshInterval || '300';
    elements.usageAlertsToggle.checked = settings.usageAlerts !== false;
    if (elements.compactModeToggle) elements.compactModeToggle.checked = !!settings.compactMode;
    if (elements.showClaudeCodeToggle) elements.showClaudeCodeToggle.checked = settings.showClaudeCode !== false;

    // Tray colours + critical outline + burn alerts
    const trayColors = settings.trayColors || {};
    const colorDefaults = {
        session: { bg: '#3b82f6', text: '#000000' },
        weekly: { bg: '#3b82f6', text: '#ffffff' },
        fable: { bg: '#ef4444', text: '#000000' },
        codex: { bg: '#10a37f', text: '#ffffff' },
        gemini: { bg: '#f4b400', text: '#000000' }
    };
    for (const [key, ids] of [
        ['session', ['traySessionBg', 'traySessionText']],
        ['weekly', ['trayWeeklyBg', 'trayWeeklyText']],
        ['fable', ['trayFableBg', 'trayFableText']],
        ['codex', ['trayOpenaiBg', 'trayOpenaiText']],
        ['gemini', ['trayGoogleBg', 'trayGoogleText']]
    ]) {
        if (elements[ids[0]]) elements[ids[0]].value = trayColors[key]?.bg || colorDefaults[key].bg;
        if (elements[ids[1]]) elements[ids[1]].value = trayColors[key]?.text || colorDefaults[key].text;
    }
    if (elements.trayOutlineToggle) elements.trayOutlineToggle.checked = settings.trayOutline?.enabled !== false;
    if (elements.trayOutlineColor) elements.trayOutlineColor.value = settings.trayOutline?.color || '#facc15';
    if (elements.burnAlertsToggle) elements.burnAlertsToggle.checked = settings.burnAlerts !== false;
    for (const [kind, ids] of [['reset', ['soundResetToggle', 'soundResetVolume']],
                               ['burn', ['soundBurnToggle', 'soundBurnVolume']],
                               ['banked', ['soundBankedToggle', 'soundBankedVolume']],
                               ['wall', ['soundWallToggle', 'soundWallVolume']]]) {
        const cfg = { enabled: true, volume: 0.85, ...((settings.sounds || {})[kind] || {}) };
        const t = document.getElementById(ids[0]); if (t) t.checked = cfg.enabled !== false;
        const v = document.getElementById(ids[1]); if (v) v.value = Math.round(cfg.volume * 100);
    }
    if (elements.fontColorToggle) elements.fontColorToggle.checked = settings.fontColor?.enabled === true;
    if (elements.fontColorPicker) elements.fontColorPicker.value = settings.fontColor?.color || '#e0e0e0';
    if (elements.webhookToggle) elements.webhookToggle.checked = settings.webhook?.enabled === true;
    if (elements.webhookUrl) elements.webhookUrl.value = settings.webhook?.url || '';
    if (elements.dailyDigestToggle) elements.dailyDigestToggle.checked = settings.dailyDigest !== false;
    if (elements.sortByUsageToggle) elements.sortByUsageToggle.checked = settings.sortByUsage === true;
    if (elements.showAccountEmailsToggle) elements.showAccountEmailsToggle.checked = settings.hideAccountEmails !== true;
    for (const prov of Object.keys(PERMAHIDE_SECTIONS)) {
        const t = document.getElementById('cliAdopt' + PERMAHIDE_TITLECASE[prov] + 'Toggle');
        if (t) t.checked = (settings.cliAdopted || {})[prov] === true;
    }
    applyProviderVisibility();
    if (elements.showCodexToggle) elements.showCodexToggle.checked = settings.showCodex !== false;
    if (elements.showCodexCliToggle) elements.showCodexCliToggle.checked = settings.showCodexCli !== false;
    if (elements.showGeminiToggle) elements.showGeminiToggle.checked = settings.showGemini !== false;
    if (elements.showGeminiCliToggle) elements.showGeminiCliToggle.checked = settings.showGeminiCli !== false;
    if (elements.googleSource) elements.googleSource.value = settings.googleSource || 'auto';
    if (elements.openaiLoginStatus) {
        const cxNow = latestUsageData && latestUsageData.codex;
        const cxConn = !!(cxNow && cxNow.connected);
        elements.openaiLoginStatus.textContent = cxConn ? (cxNow.email || 'Connected')
            : (cxNow ? 'Not connected (using CLI login)' : 'Not connected');
        elements.openaiLoginStatus.classList.remove('login-status-error');
        elements.disconnectOpenaiBtn.style.display = cxConn ? '' : 'none';
        elements.settingsConnectOpenaiBtn.style.display = cxConn ? 'none' : '';
    }
    if (elements.googleLoginStatus) {
        const gmNow = latestUsageData && latestUsageData.gemini;
        const gmConn = !!(gmNow && gmNow.connected);
        elements.googleLoginStatus.textContent = gmConn ? (gmNow.email || 'Connected')
            : (gmNow ? 'Not connected (using CLI login)' : 'Not connected');
        elements.googleLoginStatus.classList.remove('login-status-error');
        elements.disconnectGoogleBtn.style.display = gmConn ? '' : 'none';
        elements.settingsConnectGoogleBtn.style.display = gmConn ? 'none' : '';
    }

    // Populate org selector if user has organizations
    if (credentials.organizations && credentials.organizations.length > 0) {
        populateOrgSelector(credentials.organizations, credentials.organizationId);
    }

    warnThreshold = settings.warnThreshold;
    dangerThreshold = settings.dangerThreshold;

    syncThemeCycleBtn(settings.theme);
    applyTheme(settings.theme);
    if (window.electronAPI.platform === 'darwin') {
        document.getElementById('trayLabel').textContent = 'Hide from Dock';
    }
}

async function saveSettings() {
    const warn = parseInt(elements.warnThreshold.value) || 75;
    const danger = parseInt(elements.dangerThreshold.value) || 90;

    warnThreshold = warn;
    dangerThreshold = danger;

    // Apply compact mode change first, then include in saved settings
    const compactToggleValue = elements.compactModeToggle.checked;
    if (compactToggleValue !== isCompactMode) {
        applyCompactMode(compactToggleValue);
    }

    const settings = {
        autoStart: (window.electronAPI.platform === 'linux' || window.electronAPI.isPortable) ? false : elements.autoStartToggle.checked,
        minimizeToTray: elements.minimizeToTrayToggle.checked,
        alwaysOnTop: elements.alwaysOnTopToggle.checked,
        showTrayStats: elements.eyeAnthropic.classList.contains('on'),
        trayOpenai: elements.eyeOpenai.classList.contains('on'),
        trayGoogle: elements.eyeGoogle.classList.contains('on'),
        sectionCollapsed: window._cachedSettings?.sectionCollapsed || {},
        // Theme lives on the toolbar cycle button now, not in this form
        theme: (window._cachedSettings && window._cachedSettings.theme) || 'dark',
        warnThreshold: warn,
        dangerThreshold: danger,
        timeFormat: elements.timeFormat.value || '12h',
        weeklyDateFormat: elements.weeklyDateFormat.value || 'date',
        refreshInterval: elements.refreshInterval ? (elements.refreshInterval.value || '300') : '300',
        usageAlerts: elements.usageAlertsToggle.checked,
        compactMode: isCompactMode,
        graphVisible: graphVisible,
        expandedOpen: isExpanded,
        showClaudeCode: elements.showClaudeCodeToggle ? elements.showClaudeCodeToggle.checked : true,
        trayColors: {
            session: { bg: elements.traySessionBg.value, text: elements.traySessionText.value },
            weekly: { bg: elements.trayWeeklyBg.value, text: elements.trayWeeklyText.value },
            fable: { bg: elements.trayFableBg.value, text: elements.trayFableText.value },
            codex: { bg: elements.trayOpenaiBg.value, text: elements.trayOpenaiText.value },
            gemini: { bg: elements.trayGoogleBg.value, text: elements.trayGoogleText.value }
        },
        trayOutline: {
            enabled: elements.trayOutlineToggle.checked,
            color: elements.trayOutlineColor.value
        },
        burnAlerts: elements.burnAlertsToggle.checked,
        fontColor: {
            enabled: elements.fontColorToggle.checked,
            color: elements.fontColorPicker.value
        },
        webhook: {
            enabled: elements.webhookToggle.checked,
            url: elements.webhookUrl.value.trim()
        },
        dailyDigest: elements.dailyDigestToggle.checked,
        sortByUsage: elements.sortByUsageToggle ? elements.sortByUsageToggle.checked : false,
        hideAccountEmails: elements.showAccountEmailsToggle ? !elements.showAccountEmailsToggle.checked : false,
        showCodex: elements.showCodexToggle.checked,
        showCodexCli: elements.showCodexCliToggle ? elements.showCodexCliToggle.checked : true,
        showGemini: elements.showGeminiToggle ? elements.showGeminiToggle.checked : true,
        showGeminiCli: elements.showGeminiCliToggle ? elements.showGeminiCliToggle.checked : true,
        googleSource: elements.googleSource ? elements.googleSource.value : 'auto',
        openaiExtrasOpen: isOpenaiExtrasOpen,
        projectionsOn: projectionsVisible
    };
    // Merge over the existing cache so keys this form doesn't manage
    // (hiddenRows, subgroupHidden, pizazz, …) survive closing Settings
    const merged = { ...(window._cachedSettings || {}), ...settings };
    await window.electronAPI.saveSettings(merged);
    window._cachedSettings = merged;
    applyTheme(settings.theme);
    applyFontColor(settings);
    if (window.electronAPI.platform === 'darwin') {
        document.getElementById('trayLabel').textContent = 'Hide from Dock';
    }

    // Re-render resets-at values immediately with new format
    if (latestUsageData) {
        refreshTimers();
        // Rebuild extra rows to apply new threshold colors
        if (isExpanded) {
            buildExtraRows(latestUsageData);
            refreshExtraTimers();
        }
    }
    // Restart auto-update with new interval if it changed
    startAutoUpdate();
}

function applyTheme(theme) {
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    const useDark = theme === 'dark' || (theme === 'system' && prefersDark);
    document.body.classList.toggle('theme-light', !useDark);
}

// Keep the toolbar cycle button's icon and tooltip on the current mode
function syncThemeCycleBtn(theme) {
    const btn = elements.themeCycleBtn;
    if (!btn) return;
    const mode = ['dark', 'light', 'system'].includes(theme) ? theme : 'dark';
    btn.dataset.mode = mode;
    const nextName = { dark: 'light', light: 'system', system: 'dark' }[mode];
    btn.title = `Theme: ${mode} — click for ${nextName}`;
}

// Apply (or clear) the custom widget font colour from settings
function applyFontColor(settings) {
    const fc = settings?.fontColor || {};
    const enabled = fc.enabled === true && fc.color;
    document.body.classList.toggle('custom-font', !!enabled);
    if (enabled) {
        document.documentElement.style.setProperty('--custom-font', fc.color);
    }
}

// Update check
let _updateCheckRetries = 0;
let _lastUpdateCheckAt = 0;
async function checkForUpdate() {
    _lastUpdateCheckAt = Date.now();
    try {
        const result = await window.electronAPI.checkForUpdate();
        // A failed check (no network yet, GitHub rate-limit/5xx) must not be
        // mistaken for "up to date" — retry with backoff so the banner still
        // appears once the check succeeds, instead of waiting for the next
        // scheduled poll. (This is why a Mac/Windows user could sit on an old
        // version and never see the notice.)
        if (result.error) {
            if (_updateCheckRetries < 5) {
                _updateCheckRetries++;
                setTimeout(checkForUpdate, _updateCheckRetries * 20000);
            }
            return;
        }
        _updateCheckRetries = 0;
        if (!result.hasUpdate) return;

        const version = result.version;
        // A macOS source install rebuilds itself, so it offers "update" rather
        // than sending the user off to a releases page with no Mac asset on it.
        _canSelfUpdate = !!result.canSelfUpdate;

        // Show banner and expand window to compensate
        const action = _canSelfUpdate ? 'update' : 'download';
        elements.updateBannerText.textContent = `▲  Version ${version} available — click to ${action}`;
        elements.updateBanner.style.display = 'flex';
        resizeWidget(true);

        // Populate settings panel link if already visible
        if (elements.settingsUpdateLink) {
            elements.settingsUpdateLink.textContent = `→ v${version} available`;
            elements.settingsUpdateLink.style.display = 'inline';
        }

        debugLog(`Update available: v${version}`);
    } catch (e) {
        debugLog('Update check failed silently', e);
    }
}

// Re-measure the window after a restore from minimized (measurements read ~0
// while minimized, so any resize during that state was skipped)
window.addEventListener('focus', () => {
    if (!isCompactMode && elements.mainContent.style.display !== 'none') resizeWidget();
    // Re-check for a new version when the user comes back to the widget, at
    // most every 30 min — this is the reliable moment the update banner shows
    // even if the app has been open (or was offline) for a while.
    if (Date.now() - _lastUpdateCheckAt > 30 * 60 * 1000) checkForUpdate();
});

// Start the application. A renderer initialization error must never leave the
// widget displaying an unexplained permanent spinner.
init().catch((error) => {
    console.error("I'm Burning! initialization failed:", error);
    const message = elements.loadingContainer?.querySelector('p');
    if (message) message.textContent = `I'm Burning! could not start: ${error.message || 'Unknown error'}`;
});
window.addEventListener('beforeunload', () => {
    stopAutoUpdate();
    if (countdownInterval) clearInterval(countdownInterval);
});
