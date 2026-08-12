// Detached graph window renderer. Self-contained (does not share app.js
// state): it pulls the usage history + latest usage (for forecasts) over IPC,
// draws a multi-provider Chart.js line chart, follows the user's theme, and
// re-renders whenever the main process signals new data. Owns its own pin.
(async function () {
    const api = window.electronAPI;
    const chartUtils = window.BurnwatchChartUtils;
    const COMPANY = { anthropic: '#d97757', openai: '#10a37f', google: '#4285f4' };
    const CODE = { codex: '#2dd4bf', gemini: '#4285f4' };
    const SCOPED_COLORS = { fable: '#d946ef' };
    const SCOPED_FALLBACK = ['#84cc16', '#14b8a6', '#a855f7', '#64748b'];
    const canvas = document.getElementById('usageChart');
    const empty = document.getElementById('gEmpty');
    let chart = null;

    // ---- Theme (mirrors the main window's Dark / Light / System setting) ----
    const THEMES = {
        dark: { bg: '#16161e', text: '#cfcfe0', title: '#e6e6f0', border: '#2a2a38', tick: '#8a8aa0', grid: 'rgba(255,255,255,0.05)', legend: '#b9b9cc' },
        light: { bg: '#f4f4f8', text: '#2a2a3a', title: '#1a1a2e', border: '#d8d8e2', tick: '#5a5a70', grid: 'rgba(0,0,0,0.08)', legend: '#4a4a60' }
    };
    let T = THEMES.dark;
    async function applyTheme(settings) {
        let dark = true;
        let pizazzOff = false;
        try {
            const s = settings || await api.getSettings();
            const theme = (s && s.theme) || 'dark';
            const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            dark = theme === 'dark' || (theme === 'system' && prefersDark);
            pizazzOff = !!(s && s.pizazz === false);
        } catch (e) { /* default dark */ }
        T = dark ? THEMES.dark : THEMES.light;
        const root = document.documentElement.style;
        root.setProperty('--g-bg', T.bg);
        root.setProperty('--g-text', T.text);
        root.setProperty('--g-title', T.title);
        root.setProperty('--g-border', T.border);
        document.body.classList.toggle('light', !dark);
        // The clown-jail reaches the detached window too
        document.body.classList.toggle('no-pizazz', pizazzOff);
    }

    function build(history, latest, settings = {}) {
        if (chart) { chart.destroy(); chart = null; }
        if (!history || !history.length) {
            canvas.style.display = 'none';
            empty.style.display = 'flex';
            return;
        }
        canvas.style.display = '';
        empty.style.display = 'none';
        const forecasts = (latest && latest.forecasts) || {};

        const scopedKeys = [];
        const seen = new Set();
        for (const e of history) {
            for (const k of Object.keys(e.scoped || {})) { if (!seen.has(k)) { seen.add(k); scopedKeys.push(k); } }
        }

        const line = (seriesId, label, color, pick, dash) => ({
            seriesId,
            label,
            data: history.map((e) => chartUtils.point(e.timestamp, pick(e))),
            borderColor: color,
            backgroundColor: 'transparent',
            borderWidth: 2,
            borderDash: dash || undefined,
            stepped: true,
            pointRadius: 0,
            pointHoverRadius: 3,
            pointHitRadius: 10
        });

        const datasets = [
            line('claude:session', 'CLA 5H', '#8b5cf6', (e) => e.session),
            line('claude:weekly', 'CLA 7D', '#3b82f6', (e) => e.weekly)
        ];
        // Anthropic per-model/surface pools, when present.
        for (const [lbl, color, key] of [
            ['Sonnet', '#f43f5e', 'sonnet'], ['Opus', '#f59e0b', 'opus'], ['Cowork', '#06b6d4', 'cowork'],
            ['Design', '#92400e', 'design'], ['OAuth Apps', '#f97316', 'oauthApps'],
            ['Extra Usage', '#f59e0b', 'extraUsage']
        ]) {
            const vals = history.map((e) => chartUtils.finiteOrNull(e[key]));
            const id = key === 'oauthApps' ? 'oauth-apps' : key === 'extraUsage' ? 'extra-usage' : key;
            if (chartUtils.hasPositive(vals)) datasets.push(line(`anthropic:${id}`, lbl, color, (e) => e[key]));
        }
        let ci = 0;
        for (const k of scopedKeys) {
            const vals = history.map((e) => chartUtils.finiteOrNull(e.scoped?.[k]));
            if (!chartUtils.hasPositive(vals)) continue;
            const lbl = k.split('_').map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
            const color = SCOPED_COLORS[k] || SCOPED_FALLBACK[ci++ % SCOPED_FALLBACK.length];
            datasets.push(line(`scoped:${k}`, lbl, color, (e) => e.scoped?.[k]));
        }
        // Cross-provider comparison lines (already 0-100%). CLI second accounts dashed.
        const PROVIDERS = [
            ['Codex', CODE.codex, 'codex', null],
            ['Gemini', COMPANY.google, 'gemini', null],
            ['Claude CLI', COMPANY.anthropic, 'claudeCli', [5, 3]],
            ['Codex CLI', CODE.codex, 'codexCli', [5, 3]],
            ['Gemini CLI', COMPANY.google, 'geminiCli', [5, 3]]
        ];
        for (const [lbl, color, key, dash] of PROVIDERS) {
            const vals = history.map((e) => chartUtils.finiteOrNull(e[key]));
            if (!chartUtils.hasPositive(vals)) continue;
            datasets.push(line(`provider:${key}`, lbl, color, (e) => e[key], dash));
        }

        // Forecast projection lines (dotted, to the 100% crossing).
        const last = history[history.length - 1];
        let xMax = last.timestamp;
        // Window-reset clamps: a projection that lands after its own window
        // reset is a race the reset wins — don't draw it (matches the inline
        // chart). Scoped pools' resets come from the latest limits[] payload.
        const weeklyResetMs = latest?.seven_day?.resets_at
            ? new Date(latest.seven_day.resets_at).getTime() : null;
        const scopedResetMs = {};
        for (const l of ((latest && latest.limits) || [])) {
            if (l.kind !== 'weekly_scoped' || !l.resets_at) continue;
            const nm = String(l.scope?.model?.display_name || l.scope?.surface || 'Scoped');
            scopedResetMs[nm.toLowerCase().replace(/[^a-z0-9]+/g, '_')] = new Date(l.resets_at).getTime();
        }
        let hadProjection = false;
        const addProj = (seriesId, label, color, lastVal, etaIso, resetMs) => {
            if (etaIso == null || lastVal == null) return;
            const t = new Date(etaIso).getTime();
            if (!(t > last.timestamp)) return;
            if (resetMs && t > resetMs) return; // the window resets first — no cap hit
            hadProjection = true;
            datasets.push({
                seriesId: `projection:${seriesId}`,
                baseSeriesId: seriesId,
                label: label + ' → 100%',
                data: [{ x: last.timestamp, y: lastVal }, { x: t, y: 100 }],
                borderColor: color,
                backgroundColor: 'transparent',
                borderWidth: 1.5,
                borderDash: [4, 4],
                pointRadius: 0,
                pointHoverRadius: 3,
                pointHitRadius: 10
            });
            xMax = Math.max(xMax, t);
        };
        if (settings.projectionsOn !== false) {
            addProj('claude:session', 'CLA 5H', '#8b5cf6', last.session, forecasts.session, null);
            addProj('claude:weekly', 'CLA 7D', '#3b82f6', last.weekly, forecasts.weekly, weeklyResetMs);
            for (const [lbl, color, key] of [
                ['Sonnet', '#f43f5e', 'sonnet'], ['Opus', '#f59e0b', 'opus'], ['Cowork', '#06b6d4', 'cowork'],
                ['Design', '#92400e', 'design'], ['OAuth Apps', '#f97316', 'oauthApps']
            ]) {
                const id = key === 'oauthApps' ? 'oauth-apps' : key;
                addProj(`anthropic:${id}`, lbl, color, last[key], forecasts[key], weeklyResetMs);
            }
            for (const k of scopedKeys) {
                addProj(`scoped:${k}`, k.split('_').map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(' '),
                    SCOPED_COLORS[k] || '#84cc16', last.scoped?.[k], forecasts.scoped?.[k], scopedResetMs[k] || null);
            }
            for (const [lbl, color, key] of [
                ['Codex', CODE.codex, 'codex'], ['Gemini', COMPANY.google, 'gemini'],
                ['Claude CLI', COMPANY.anthropic, 'claudeCli'], ['Codex CLI', CODE.codex, 'codexCli'],
                ['Gemini CLI', COMPANY.google, 'geminiCli']
            ]) {
                addProj(`provider:${key}`, lbl, color, last[key], forecasts[key], null);
            }
        }
        // Grey reset marker when the weekly reset falls inside the projected
        // span — the visual race between "cap hit" and "window resets".
        if (hadProjection && weeklyResetMs && weeklyResetMs > last.timestamp
            && weeklyResetMs <= xMax + 6 * 60 * 60 * 1000) {
            xMax = Math.max(xMax, weeklyResetMs);
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
        xMax = Math.min(xMax, Date.now() + 3 * 24 * 60 * 60 * 1000);

        const hiddenSeries = settings.chartHiddenSeries || {};
        datasets.forEach((dataset) => {
            if (hiddenSeries[dataset.seriesId]
                || (dataset.baseSeriesId && hiddenSeries[dataset.baseSeriesId])) {
                dataset.hidden = true;
            }
        });

        let maxV = 0;
        for (const d of datasets) for (const pt of d.data) if (pt.y > maxV) maxV = pt.y;
        const yMax = Math.max(10, Math.ceil(maxV / 10) * 10);
        const first = new Date(history[0].timestamp); first.setHours(0, 0, 0, 0);
        const spanMs = Math.max(0, xMax - first.getTime());

        chart = new Chart(canvas.getContext('2d'), {
            type: 'line',
            data: { datasets },
            options: {
                animation: false,
                responsive: true,
                maintainAspectRatio: false,
                spanGaps: false,
                interaction: { intersect: false, mode: 'nearest' },
                scales: {
                    x: {
                        type: 'linear',
                        min: first.getTime(),
                        max: xMax,
                        // Day-boundary ticks on multi-day spans (matches the
                        // inline chart) — default numeric steps repeat labels.
                        afterBuildTicks(axis) {
                            const span = axis.max - axis.min;
                            if (span < 48 * 60 * 60 * 1000) return;
                            const d = new Date(first.getTime());
                            const ticks = [];
                            while (d.getTime() <= axis.max) {
                                if (d.getTime() >= axis.min) ticks.push({ value: d.getTime() });
                                d.setDate(d.getDate() + 1);
                            }
                            if (ticks.length >= 2) axis.ticks = ticks;
                        },
                        ticks: {
                            font: { size: 10 }, color: T.tick, maxRotation: 0,
                            callback: (value) => chartUtils.formatTimestampTick(value, spanMs, settings.timeFormat)
                        },
                        grid: { display: false }
                    },
                    y: {
                        min: 0,
                        max: yMax,
                        ticks: { font: { size: 10 }, color: T.tick, callback: (v) => v + '%' },
                        grid: { color: T.grid }
                    }
                },
                plugins: {
                    legend: {
                        display: true,
                        position: 'bottom',
                        labels: {
                            boxWidth: 8, boxHeight: 8, padding: 6,
                            font: { size: 9 }, color: T.legend,
                            filter: (item) => !/→ 100%|reset/i.test(item.text)
                        },
                        onClick: async (event, item, legend) => {
                            const target = legend.chart;
                            const willHide = target.isDatasetVisible(item.datasetIndex);
                            target.getDatasetMeta(item.datasetIndex).hidden = willHide ? true : null;
                            target.update();
                            const seriesId = target.data.datasets[item.datasetIndex].seriesId;
                            if (!seriesId) return;
                            const nextSettings = await api.getSettings();
                            const hidden = { ...(nextSettings.chartHiddenSeries || {}) };
                            if (willHide) hidden[seriesId] = true; else delete hidden[seriesId];
                            nextSettings.chartHiddenSeries = hidden;
                            await api.saveSettings(nextSettings);
                        }
                    }
                }
            }
        });
        // An empty trance explains itself (matches the inline chart's hint)
        canvas.title = (settings.projectionsOn !== false && !hadProjection)
            ? 'No projections yet — they appear after ~30 minutes of rising usage'
            : '';
    }

    let lastHistory = null, lastLatest = null, lastSettings = null;
    async function refresh() {
        try {
            const [history, latest, settings] = await Promise.all([
                api.getUsageHistory(), api.getLatestUsage(), api.getSettings()
            ]);
            lastHistory = history; lastLatest = latest; lastSettings = settings;
            await applyTheme(settings);
            applyPsychicState(settings.projectionsOn !== false);
            build(history, latest, settings);
        } catch (err) { /* window may be closing */ }
    }

    // Forecast-projection toggle (the psychic) — mirrors the inline graph's
    // button and shares the same settings.projectionsOn, so toggling here or
    // in the main window keeps both in sync.
    const psychic = document.getElementById('psychicBtn');
    const psychicImg = document.getElementById('psychicImg');
    function applyPsychicState(on) {
        if (!psychic) return;
        psychic.classList.toggle('on', on);
        psychicImg.src = on ? '../../assets/psychic-on.png' : '../../assets/psychic-idle.png';
        psychic.title = on
            ? 'Forecast projections: ON — the psychic sees your future burn'
            : 'Forecast projections: OFF — the psychic awaits your command';
    }
    if (psychic) {
        psychic.addEventListener('click', async () => {
            const next = !(lastSettings ? lastSettings.projectionsOn !== false : true);
            applyPsychicState(next);
            try {
                const s = await api.getSettings();
                s.projectionsOn = next;
                await api.saveSettings(s);
                lastSettings = s;
            } catch (e) { /* ignore */ }
            if (lastHistory) build(lastHistory, lastLatest, lastSettings || {});
        });
    }

    // Always-on-top pin (persisted in main via settings.graphAlwaysOnTop).
    const pin = document.getElementById('pinBtn');
    const pinLabel = document.getElementById('pinLabel');
    let onTop = true;
    try { onTop = await api.graphGetAlwaysOnTop(); } catch (e) { /* default true */ }
    const paintPin = () => { pin.classList.toggle('on', onTop); pinLabel.textContent = onTop ? 'On top' : 'Not pinned'; };
    paintPin();
    pin.addEventListener('click', () => { onTop = !onTop; api.graphSetAlwaysOnTop(onTop); paintPin(); });

    // React to OS light/dark flips when the user's theme is "System".
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', async () => {
        await applyTheme(lastSettings);
        if (lastHistory) build(lastHistory, lastLatest, lastSettings || {});
    });

    if (api.onUsageUpdated) api.onUsageUpdated(() => refresh());
    if (api.onGraphSettingsUpdated) api.onGraphSettingsUpdated(() => refresh());
    await refresh();
})();
