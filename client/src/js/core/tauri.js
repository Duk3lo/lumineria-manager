export function invoke(cmd, args) {
    if (!window.__TAURI__) {
        throw new Error("window.__TAURI__ no está listo todavía.");
    }
    return window.__TAURI__.core.invoke(cmd, args);
}

export function listen(event, handler) {
    if (!window.__TAURI__) {
        throw new Error("window.__TAURI__ no está listo todavía.");
    }
    return window.__TAURI__.event.listen(event, handler);
}