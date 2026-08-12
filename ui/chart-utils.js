(function (root, factory) {
    const api = factory();
    if (typeof module === 'object' && module.exports) module.exports = api;
    if (root) root.BurnwatchChartUtils = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    function finiteOrNull(value) {
        return typeof value === 'number' && Number.isFinite(value) ? value : null;
    }

    function hasPositive(values) {
        return values.some((value) => finiteOrNull(value) != null && value > 0);
    }

    function point(timestamp, value) {
        return { x: timestamp, y: finiteOrNull(value) };
    }

    function formatTimestampTick(timestamp, spanMs, timeFormat) {
        const date = new Date(timestamp);
        const hour12 = (timeFormat || '12h') !== '24h';
        if (spanMs < 12 * 60 * 60 * 1000) {
            return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit', hour12 });
        }
        if (spanMs < 48 * 60 * 60 * 1000) {
            return date.toLocaleString([], { weekday: 'short', hour: 'numeric', hour12 });
        }
        return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
    }

    return { finiteOrNull, hasPositive, point, formatTimestampTick };
});
