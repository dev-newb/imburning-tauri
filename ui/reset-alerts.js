(function (root, factory) {
    const api = factory();
    if (typeof module === 'object' && module.exports) module.exports = api;
    else root.BurnwatchResetAlerts = api;
})(typeof window === 'object' ? window : globalThis, function () {
    'use strict';
    const MAX_GAP = 30 * 60 * 1000;
    const CLOCK_MARGIN = 60 * 1000;
    const FIVE_HOUR_WINDOW_MINUTES = 5 * 60;
    const valid = (n) => typeof n === 'number' && Number.isFinite(n);
    const isFiveHourPool = (pool) => {
        if (valid(pool.windowMinutes) && Math.round(pool.windowMinutes) === FIVE_HOUR_WINDOW_MINUTES) return true;
        const key = String(pool.key || '');
        return key === 'five_hour' || key.endsWith('_five_hour')
            || key.includes('_five_hour_') || /\(5h\)/i.test(String(pool.label || ''));
    };

    function createTracker() {
        const limits = new Map(), credits = new Map();
        function synchronize(states, readings) {
            const present = new Map(readings.map(p => [p.key, p]));
            for (const [key, state] of states) {
                if (!present.has(key) || present.get(key).identity !== state.current.identity) states.delete(key);
            }
        }
        function advance(states, p, usable, maxGap) {
            const state = states.get(p.key);
            if (!usable || !valid(p.observedAt) || p.observedAt <= 0) {
                // Unknown is not zero. Forget the sample so recovery seeds quietly.
                states.delete(p.key);
                return null;
            }
            if (!state || p.observedAt - state.seen > maxGap) {
                states.set(p.key, { current: p, seen: p.observedAt, pending: null });
                return null;
            }
            // Redraws, cached responses and out-of-order deliveries cannot confirm anything.
            if (p.observedAt <= state.seen) return null;
            state.seen = p.observedAt;
            return state;
        }
        function event(kind, before, after, reason) {
            return {
                // The backend hashes this before saving; never log account identifiers.
                key: JSON.stringify([kind, after.account, after.pool,
                    kind === 'banked' ? after.count : Math.round(before.resetsAt / CLOCK_MARGIN)]),
                shared: !!after.account?.[1], pool: after.key, reason,
                from: kind === 'banked' ? before.count : before.pct,
                to: kind === 'banked' ? after.count : after.pct,
                previousResetAt: valid(before.resetsAt) ? before.resetsAt : null,
                resetAt: valid(after.resetsAt) ? after.resetsAt : null,
                observedAt: after.observedAt
            };
        }
        function observe(pools, banks, maxGap = MAX_GAP) {
            maxGap = valid(maxGap) ? Math.max(MAX_GAP, maxGap) : MAX_GAP;
            synchronize(limits, pools);
            synchronize(credits, banks);
            const blocked = [...limits.values()].filter(s => s.current.pct >= 100).map(s => s.current.key);
            const result = { reset: [], suppressed: [], banked: [], wall: [], recovered: false };
            for (const p of pools) {
                const state = advance(limits, p, valid(p.pct) && p.pct >= 0, maxGap);
                if (!state) continue;
                const before = state.current;
                const pending = state.pending;
                if (pending) {
                    const sameWindow = !valid(pending.reading.resetsAt)
                        || (valid(p.resetsAt) && p.resetsAt >= pending.reading.resetsAt - CLOCK_MARGIN);
                    const stillReset = pending.reason === 'scheduled' && valid(pending.reading.resetsAt)
                        ? sameWindow
                        : sameWindow && p.pct <= Math.max(1, before.pct / 2);
                    state.pending = null;
                    if (stillReset) {
                        const resetEvent = event('reset', before, p, pending.reason);
                        if (isFiveHourPool(p)) {
                            result.suppressed.push(resetEvent);
                        } else {
                            result.reset.push(resetEvent);
                        }
                        state.current = p;
                        continue;
                    }
                    // The apparent reset bounced back. Retain the pre-drop baseline
                    // for wall/recovery checks, so 100 -> 0 -> 100 stays silent.
                }
                const deadline = valid(before.resetsAt);
                const nearSchedule = deadline && before.resetsAt <= p.observedAt + CLOCK_MARGIN;
                const nextWindow = deadline && valid(p.resetsAt)
                    && p.resetsAt > before.resetsAt + CLOCK_MARGIN;
                const cleared = before.pct >= 5 && p.pct <= 1;
                if ((nearSchedule && nextWindow)
                    || (nearSchedule && before.pct > 0 && p.pct < before.pct && p.pct <= 1)
                    || (cleared && deadline && valid(p.resetsAt))) {
                    state.pending = { reading: p, reason: nearSchedule ? 'scheduled' : 'early' };
                    continue;
                }
                if (before.pct < 100 && p.pct >= 100) result.wall.push(p);
                state.current = p;
            }
            result.recovered = blocked.length > 0
                && blocked.every(key => limits.has(key))
                && ![...limits.values()].some(s => s.current.pct >= 100);
            for (const p of banks) {
                const state = advance(credits, p, Number.isInteger(p.count) && p.count >= 0, maxGap);
                if (!state) continue;
                if (p.count === state.current.count) { state.pending = null; state.current = p; continue; }
                if (state.pending?.count === p.count) {
                    if (p.count > state.current.count) result.banked.push(event('banked', state.current, p, 'credit-increase'));
                    state.current = p;
                    state.pending = null;
                } else {
                    // Confirm decreases too: a spurious 1 -> 0 -> 1 is not a grant.
                    state.pending = p;
                }
            }
            return result;
        }
        return { observe, forget(keys) { for (const key of keys) { limits.delete(key); credits.delete(key); } } };
    }
    return { createTracker };
});
