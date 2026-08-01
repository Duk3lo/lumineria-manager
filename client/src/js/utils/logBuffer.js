// js/utils/logBuffer.js (archivo nuevo)
export function createLogBuffer(getEl, maxLines = 400) {
    let pending = [];
    let scheduled = false;

    function flush() {
        scheduled = false;
        const el = getEl();
        if (!el || pending.length === 0) return;

        el.textContent += pending.join('\n') + '\n';
        pending = [];

        const lines = el.textContent.split('\n');
        if (lines.length > maxLines) {
            el.textContent = lines.slice(-maxLines).join('\n');
        }
        el.scrollTop = el.scrollHeight;
    }

    return function appendLine(line) {
        pending.push(line);
        if (!scheduled) {
            scheduled = true;
            requestAnimationFrame(flush);
        }
    };
}