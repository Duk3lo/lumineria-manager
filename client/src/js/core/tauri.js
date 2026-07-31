export function invoke(cmd, args) {
    return window.__TAURI__.core.invoke(cmd, args);
}

export function listen(event, handler) {
    return window.__TAURI__.event.listen(event, handler);
}