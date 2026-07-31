import { listen } from './core/tauri.js';
import { initTabs } from './ui/tabs.js';
import { initConnection, restoreLastConnection } from './features/connection.js';
import { initCreator } from './features/creator.js';
import { initServerDetail, currentServerId } from './ui/serverDetail.js';
import { appendLine } from './features/logs.js';
import { renderServers, updateStatus } from './ui/serverList.js';
import { invoke_ws_action } from './features/actions.js';
import { initConfirmModal } from './ui/confirmModal.js';

document.addEventListener("DOMContentLoaded", async () => {
    // Inicializar toda la UI y los eventos
    initTabs();
    initCreator();
    initConnection();
    initServerDetail();
    initConfirmModal();

    // Escuchar los eventos del WebSockets (Agente)
    await listen("server-event", (event) => {
        const data = event.payload;

        if (data.type === "servers") {
            renderServers(data.servers);
        } else if (data.type === "install_progress") {
            updateStatus(`[${data.percentage}%] ${data.step}`, "#fab387");
        } else if (data.type === "ack") {
            updateStatus("Conectado", "#a6e3a1");
            if (data.message) alert("Operación completada: " + data.message);
            invoke_ws_action({ type: "list_servers" });
        } else if (data.type === "error") {
            updateStatus("Error: " + data.message, "#f38ba8");
            alert("Error: " + data.message);
        } else if (data.type === "log_line") {
            if (data.id === currentServerId) {
                appendLine(data.line);
            }
        }
    });


    await restoreLastConnection();
});