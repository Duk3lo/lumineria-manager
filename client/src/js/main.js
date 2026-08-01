import { listen } from './core/tauri.js';
import { initTabs } from './ui/tabs.js';
import { initConnection, restoreLastConnection } from './features/connection.js';
import { initCreator } from './features/creator.js';
import { initServerDetail, currentServerId, appendPackwizLog, renderPackwizMods } from './ui/serverDetail.js';
import { appendLine } from './features/logs.js';
import { renderServers, updateStatus } from './ui/serverList.js';
import { invoke_ws_action, listPackwizMods } from './features/actions.js';
import { initConfirmModal } from './ui/confirmModal.js';
import { initPublishSettings } from './ui/publishSettings.js';

document.addEventListener("DOMContentLoaded", async () => {
    initTabs();
    initCreator();
    initConnection();
    initServerDetail();
    initConfirmModal();
    initPublishSettings();

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
            if (currentServerId) listPackwizMods(currentServerId);
        } else if (data.type === "error") {
            updateStatus("Error: " + data.message, "#f38ba8");
            alert("Error: " + data.message);
        } else if (data.type === "log_line") {
            if (data.id === currentServerId) {
                appendLine(data.line);
            }
        } else if (data.type === "packwiz_log") {
            if (data.id === currentServerId) {
                appendPackwizLog(data.line);
            }
        } else if (data.type === "packwiz_mods_list") {
            if (data.id === currentServerId) {
                renderPackwizMods(data.mods);
            }
        }
    });

    await restoreLastConnection();
});