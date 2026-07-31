import { listen } from './core/tauri.js';
import { initTabs } from './ui/tabs.js';
import { initConnection, restoreLastConnection } from './features/connection.js';
import { initCreator } from './features/creator.js';
import { initLogs, appendLine, currentLogServerId } from './features/logs.js';
import { invoke_ws_action } from './features/actions.js';
import { renderServers, updateStatus } from './ui/serverList.js';

document.addEventListener("DOMContentLoaded", async () => {
    initTabs();
    initCreator();
    initLogs();
    initConnection();

    await listen("server-event", (event) => {
        const data = event.payload;
        if (data.type === "servers") {
            renderServers(data.servers);
        } else if (data.type === "install_progress") {
            document.getElementById('install-progress-lbl').innerText = `[${data.percentage}%] ${data.step}`;
        } else if (data.type === "ack") {
            updateStatus("Conectado", "#a6e3a1");
            document.getElementById('install-progress-lbl').innerText = "";
            alert("Operación completada: " + (data.message || "OK"));
            invoke_ws_action({ type: "list_servers" });
        } else if (data.type === "error") {
            updateStatus("Conectado", "#a6e3a1");
            document.getElementById('install-progress-lbl').innerText = "";
            alert("Error: " + data.message);
        } else if (data.type === "log_line") {
            if (data.id === currentLogServerId) {
                appendLine(data.line);
            }
        }
    });

    await restoreLastConnection();
});