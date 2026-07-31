import { invoke } from '../core/tauri.js';
import { STATE } from '../core/state.js';
import { updateStatus } from '../ui/serverList.js';
import { invoke_ws_action } from './actions.js';
import { switchTab } from '../ui/tabs.js';

export async function connectAgent(url) {
    try {
        await invoke('connect_agent', { url });
        updateStatus("Conectado", "#a6e3a1");
        invoke_ws_action({ type: "list_servers" });


        document.getElementById('view-connection').classList.add('hidden');
        document.getElementById('view-grid').classList.remove('hidden');

        return true;
    } catch (e) {
        updateStatus("Fallo de conexión: " + e, "#f38ba8");
        return false;
    }
}

export async function restoreLastConnection() {
    try {
        const last = await invoke('load_last_connection');
        if (!last) return;

        if (last.mode === 'local' && last.folder) {
            switchTab('local');
            STATE.selectedFolder = last.folder;
            document.getElementById('folder-path').innerText = STATE.selectedFolder;
            document.getElementById('btn-start-local').disabled = false;
            updateStatus("Reconectando...", "#f9e2af");
            try {
                const url = await invoke('start_local_agent', { rootPath: STATE.selectedFolder });
                await connectAgent(url);
            } catch (e) {
                updateStatus("Error: " + e, "#f38ba8");
            }
        } else if (last.mode === 'remote' && last.url) {
            switchTab('remote');
            document.getElementById('input-url').value = last.url;
            updateStatus("Reconectando...", "#f9e2af");
            await connectAgent(last.url);
        }
    } catch (e) { }
}

export function initConnection() {
    document.getElementById('btn-pick-folder').onclick = async () => {
        STATE.selectedFolder = await invoke('pick_folder');
        if (STATE.selectedFolder) {
            document.getElementById('folder-path').innerText = STATE.selectedFolder;
            document.getElementById('btn-start-local').disabled = false;
        }
    };

    document.getElementById('btn-start-local').onclick = async () => {
        updateStatus("Iniciando agente local...", "#f9e2af");
        try {
            const url = await invoke('start_local_agent', { rootPath: STATE.selectedFolder });
            const ok = await connectAgent(url);
            if (ok) await invoke('save_last_connection', { mode: 'local', folder: STATE.selectedFolder, url: null });
        } catch (e) {
            updateStatus("Error: " + e, "#f38ba8");
        }
    };

    document.getElementById('btn-connect-remote').onclick = async () => {
        const url = document.getElementById('input-url').value;
        const ok = await connectAgent(url);
        if (ok) await invoke('save_last_connection', { mode: 'remote', folder: null, url });
    };

    document.getElementById('btn-refresh-list').onclick = () => invoke_ws_action({ type: "list_servers" });
}