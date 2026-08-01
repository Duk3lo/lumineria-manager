import { invoke } from '../core/tauri.js';
import { updateStatus } from '../ui/serverList.js';
import { STATE } from '../core/state.js';
import { showConfirm } from '../ui/confirmModal.js';

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
    const confirmed = await showConfirm(
        `¿Estás seguro de eliminar el servidor '${id}'?\nEsta acción BORRARÁ TODO (Mundos, Plugins, Logs y el Contenedor) y no se puede revertir.`,
        'Eliminar servidor'
    );
    if (!confirmed) return false;

    updateStatus("Eliminando servidor...", "#f38ba8");
    try {
        await invoke('delete_server', { id });
        return true;
    } catch (e) {
        alert("Error: " + e);
        return false;
    }
}

export async function openServerFolder(id) {
    if (STATE.selectedFolder) {
        // Corregido para que reconozca correctamente la barra invertida en Windows
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

export async function addMod(id, query) {
    updateStatus("Añadiendo mod...", "#f9e2af");
    await invoke('add_mod_packwiz', { id, query });
}
export async function removeMod(id, query) {
    updateStatus("Eliminando mod...", "#f9e2af");
    await invoke('remove_mod_packwiz', { id, query });
}
export async function uploadMod(id, filename, dataBase64, folder) {
    updateStatus("Subiendo archivo...", "#f9e2af");
    await invoke('upload_mod_packwiz', { id, filename, dataBase64, folder });
}
export async function publishModpack(id, packKey, image = null) {
    updateStatus("Publicando en VPS...", "#f9e2af");
    await invoke('publish_packwiz', { id, packKey, image });
}

export async function unpublishModpack(id, packKey) {
    updateStatus("Quitando publicación...", "#f9e2af");
    await invoke('unpublish_packwiz', { id, packKey });
}

export async function listPackwizMods(id) {
    await invoke('list_packwiz_mods', { id });
}

export async function sendConsoleCommand(id, command) {
    if (!command) return;
    await invoke('send_console_command', { id, command });
}

export async function listPackwizFiles(id) {
    await invoke('list_packwiz_files', { id });
}

export async function readFile(id, path) { await invoke('read_packwiz_file', { id, path }); }
export async function writeFile(id, path, content) { await invoke('write_packwiz_file', { id, path, content }); }
export async function deleteFile(id, path) { await invoke('delete_packwiz_file', { id, path }); }

export async function createDirectory(id, path) { await invoke('create_packwiz_directory', { id, path }); }

export async function updateAllServer(id, loaderVersion) {
    updateStatus("Actualizando servidor...", "#f9e2af");
    try {
        await invoke('update_server', { id, loaderVersion });
    } catch (e) {
        alert("Error: " + e);
    }
}

window.sendAction = sendAction;
window.confirmDelete = confirmDelete;
window.openServerFolder = openServerFolder;