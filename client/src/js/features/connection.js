import { invoke } from '../core/tauri.js';
import { STATE } from '../core/state.js';
import { updateStatus } from '../ui/serverList.js';
import { invoke_ws_action } from './actions.js';
import { switchTab } from '../ui/tabs.js';
import { showConfirm } from '../ui/confirmModal.js';

// 👇 NUEVO: si la URL no es wss:// (cifrada) y tampoco apunta a localhost/127.0.0.1,
// el token y todo el tráfico (archivos, comandos de consola, credenciales RCON)
// viajarían en texto plano por la red. Avisamos antes de conectar.
function isSecureOrLocal(rawUrl) {
    let u;
    try {
        u = new URL(rawUrl.trim());
    } catch {
        return false;
    }
    if (u.protocol === 'wss:') return true;
    return u.hostname === 'localhost' || u.hostname === '127.0.0.1' || u.hostname === '::1';
}

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
            STATE.mode = 'local';
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
            STATE.mode = 'remote';
            switchTab('remote');
            document.getElementById('input-url').value = last.url;
            updateStatus("Reconectando...", "#f9e2af");
            // No volvemos a preguntar acá: si ya se guardó antes, el usuario ya confirmó.
            await connectAgent(last.url);
        }
    } catch (e) { }
}

export function initConnection() {
    document.getElementById('btn-pick-folder').onclick = async () => {
        const picked = await invoke('pick_folder');
        if (picked) {
            STATE.selectedFolder = picked;
            document.getElementById('folder-path').innerText = STATE.selectedFolder;
            document.getElementById('btn-start-local').disabled = false;
        }
    };

    document.getElementById('btn-start-local').onclick = async () => {
        updateStatus("Iniciando agente local...", "#f9e2af");
        try {
            const url = await invoke('start_local_agent', { rootPath: STATE.selectedFolder });
            const ok = await connectAgent(url);
            if (ok) {
                STATE.mode = 'local';
                await invoke('save_last_connection', { mode: 'local', folder: STATE.selectedFolder, url: null });
            }
        } catch (e) {
            updateStatus("Error: " + e, "#f38ba8");
        }
    };

    document.getElementById('btn-connect-remote').onclick = async () => {
        const url = document.getElementById('input-url').value;

        if (!isSecureOrLocal(url)) {
            const proceed = await showConfirm(
                "Esta URL no usa wss:// (cifrado) y no apunta a localhost.\n" +
                "El token y todo el tráfico (archivos, consola, credenciales RCON) viajarían " +
                "sin cifrar por la red.\n\nSolo continúa si estás conectando a través de un túnel SSH.\n\n" +
                "¿Conectar de todas formas?",
                "⚠️ Conexión sin cifrar"
            );
            if (!proceed) return;
        }

        const ok = await connectAgent(url);
        if (ok) {
            STATE.mode = 'remote';
            await invoke('save_last_connection', { mode: 'remote', folder: null, url });
        }
    };

    document.getElementById('btn-refresh-list').onclick = () => invoke_ws_action({ type: "list_servers" });
}