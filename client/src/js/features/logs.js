import { invoke } from '../core/tauri.js';

const MAX_LINES = 2000;
export let currentLogServerId = null;
let lines = [];
let pending = [];
let flushQueued = false;

function queueFlush() {
    if (flushQueued) return;
    flushQueued = true;
    requestAnimationFrame(flush);
}

function flush() {
    flushQueued = false;
    if (pending.length === 0) return;

    lines.push(...pending);
    pending = [];
    if (lines.length > MAX_LINES) {
        lines = lines.slice(lines.length - MAX_LINES);
    }
    const container = document.getElementById('log-container');
    if (container && currentLogServerId) {
        container.textContent = lines.join('\n') + '\n';
        container.scrollTop = container.scrollHeight;
    }
}

export function appendLine(text) {
    pending.push(text);
    queueFlush();
}

export async function openLogs(id) {
    currentLogServerId = id;
    lines = [];
    pending = [];
    const container = document.getElementById('log-container');
    if (container) container.textContent = '';

    await invoke('subscribe_logs', { id });
}

export async function closeLogs() {
    if (currentLogServerId) {
        await invoke('unsubscribe_logs', { id: currentLogServerId });
        currentLogServerId = null;
    }
    lines = [];
    pending = [];
}

export function initLogs() {

}