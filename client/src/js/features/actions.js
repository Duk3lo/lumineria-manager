import { invoke } from '../core/tauri.js';
import { updateStatus } from '../ui/serverList.js';
import { STATE } from '../core/state.js';

export async function invoke_ws_action(payload) {
    if (payload.type === "list_servers") await invoke('list_servers');
    if (payload.type === "create_server") {
        await invoke('create_server', { id: payload.id, config: payload.config });
    }
}

export async function sendAction(type, id) {
    updateStatus("Enviando comando " + type + "...", "#f9e2af");
    try {
        if (type === "start_server") await invoke('start_server', { id });
        if (type === "stop_server") await invoke('stop_server', { id });
        if (type === "restart_server") await invoke('restart_server', { id });
        if (type === "recreate_container") await invoke('recreate_container', { id });
        if (type === "auto_update") {
            updateStatus("Actualizando compilación del motor...", "#fab387");
            await invoke('auto_update_server', { id });
        }
    } catch (e) {
        alert("Error: " + e);
    }
}

export async function confirmDelete(id) {
    if (confirm(`¿Estás seguro de eliminar el servidor '${id}'?\nEsta acción BORRARÁ TODO (Mundos, Plugins, Logs y el Contenedor) y no se puede revertir.`)) {
        updateStatus("Eliminando servidor...", "#f38ba8");
        try {
            await invoke('delete_server', { id });
        } catch (e) {
            alert("Error: " + e);
        }
    }
}

export async function openServerFolder(id) {
    if (STATE.selectedFolder) {
        const sep = STATE.selectedFolder.includes('\\') ? '\\' : '/';
        const fullPath = STATE.selectedFolder + sep + id;
        try {
            await invoke('open_folder_in_os', { path: fullPath });
        } catch (e) {
            alert("Error al abrir la carpeta: " + e);
        }
    } else {
        alert("Para ver la carpeta debes estar conectado en 'Modo Local' con la ruta seleccionada.");
    }
}


window.sendAction = sendAction;
window.confirmDelete = confirmDelete;
window.openServerFolder = openServerFolder;