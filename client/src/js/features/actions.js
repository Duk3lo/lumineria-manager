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
export async function uploadMod(id, filename, dataBase64, folder, scope = "packwiz") {
    updateStatus("Subiendo archivo...", "#f9e2af");
    await invoke('upload_mod_packwiz', { id, filename, dataBase64, folder, scope });
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

export async function listPackwizFiles(id, scope = "packwiz") {
    await invoke('list_packwiz_files', { id, scope });
}

export async function readFile(id, path, scope = "packwiz") {
    await invoke('read_packwiz_file', { id, path, scope });
}
export async function writeFile(id, path, content, scope = "packwiz") {
    await invoke('write_packwiz_file', { id, path, content, scope });
}
export async function deleteFile(id, path, scope = "packwiz") {
    await invoke('delete_packwiz_file', { id, path, scope });
}
export async function createDirectory(id, path, scope = "packwiz") {
    await invoke('create_packwiz_directory', { id, path, scope });
}

export async function syncPackToServer(id) {
    updateStatus("Sincronizando mods con el servidor...", "#f9e2af");
    await invoke('sync_pack_to_server', { id });
}

export async function updateAllServer(id, loaderVersion, updateMods = true, updateEngine = true, force = false) {
    updateStatus("Actualizando servidor...", "#f9e2af");
    try {
        await invoke('update_server', { id, loaderVersion, updateMods, updateEngine, force });
    } catch (e) {
        alert("Error: " + e);
    }
}

export async function listVelocityPlugins(id) {
    await invoke('list_velocity_plugins', { id });
}
export async function addVelocityPlugin(id, source, value) {
    updateStatus("Añadiendo plugin...", "#f9e2af");
    await invoke('add_velocity_plugin', { id, source, value });
}
export async function removeVelocityPlugin(id, source, value) {
    updateStatus("Eliminando plugin...", "#f9e2af");
    await invoke('remove_velocity_plugin', { id, source, value });
}
export async function setVelocityMcVersionHint(id, mcVersion) {
    await invoke('set_velocity_mc_version_hint', { id, mcVersion: mcVersion || null });
}

export async function syncVelocityPluginsNow(id) {
    updateStatus("Actualizando plugins de Velocity...", "#f9e2af");
    await invoke('sync_velocity_plugins', { id });
}

export async function setMotd(id, motd) {
    await invoke('set_motd', { id, motd });
}

export async function setPort(id, port) {
    await invoke('set_port', { id, port });
}

export async function uploadServerIcon(id, dataBase64) {
    updateStatus("Subiendo ícono del servidor...", "#f9e2af");
    await invoke('upload_server_icon', { id, dataBase64 });
}

export async function setPublishConfigRemote(sshHost, remoteBase, domain) {
    updateStatus("Actualizando configuración de publicación...", "#f9e2af");
    await invoke('set_publish_config_remote', { sshHost, remoteBase, domain });
}

export async function getPublishConfigRemote() {
    await invoke('get_publish_config_remote');
}

window.sendAction = sendAction;
window.confirmDelete = confirmDelete;
window.openServerFolder = openServerFolder;