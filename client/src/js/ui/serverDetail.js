import {
    sendAction, confirmDelete, openServerFolder,
    addMod, removeMod, uploadMod, publishModpack, syncPackToServer,
    listPackwizMods, unpublishModpack, sendConsoleCommand, listPackwizFiles,
    readFile, writeFile, deleteFile, createDirectory, updateAllServer,
    listVelocityPlugins, addVelocityPlugin, removeVelocityPlugin, setVelocityMcVersionHint, syncVelocityPluginsNow,
    setMotd, setPort, uploadServerIcon
} from '../features/actions.js';
import { openLogs, closeLogs } from '../features/logs.js';
import { appendLine } from '../features/logs.js';
import { showConfirm } from '../ui/confirmModal.js';
import { getLatestLoaderVersion } from '../features/creator.js';
import { withGuard } from '../ui/guard.js';
import { updateStatus } from '../ui/serverList.js';
import { createLogBuffer } from '../utils/logBuffer.js';
import { createFileExplorer } from '../features/fileExplorer.js';

const packwizExplorer = createFileExplorer({
    scope: "packwiz",
    ids: {
        tree: 'pw-file-tree', selectedPath: 'pw-selected-path',
        folderPanel: 'pw-folder-panel', filePanel: 'pw-file-panel',
        uploadInput: 'pw-upload-input', btnUpload: 'btn-pw-upload',
        btnCreateDir: 'btn-pw-create-dir', btnDeleteDir: 'btn-pw-delete-dir',
        fileEditor: 'pw-file-editor', btnSaveFile: 'btn-pw-save-file',
        btnDeleteFile: 'btn-pw-delete-file', btnRefreshTree: 'btn-pw-refresh-tree',
    },
});

const serverExplorer = createFileExplorer({
    scope: "server_root",
    ids: {
        tree: 'sv-file-tree', selectedPath: 'sv-selected-path',
        folderPanel: 'sv-folder-panel', filePanel: 'sv-file-panel',
        uploadInput: 'sv-upload-input', btnUpload: 'btn-sv-upload',
        btnCreateDir: 'btn-sv-create-dir', btnDeleteDir: 'btn-sv-delete-dir',
        fileEditor: 'sv-file-editor', btnSaveFile: 'btn-sv-save-file',
        btnDeleteFile: 'btn-sv-delete-file', btnRefreshTree: 'btn-sv-refresh-tree',
    },
});

export function handleFilesListEvent(data) {
    if (data.id !== currentServerId) return;
    if (data.scope === "packwiz") packwizExplorer.render(data.files);
    else if (data.scope === "server_root") serverExplorer.render(data.files);
}

export function handleVelocityPluginsListEvent(data) {
    if (data.id !== currentServerId) return;
    renderVelocityPlugins(data.plugins);
}

function renderVelocityPlugins(plugins) {
    const container = document.getElementById('vp-plugins-list');
    if (!container) return;
    if (!plugins || plugins.length === 0) {
        container.innerHTML = `<p style="color:#6c7086; text-align:center; margin:20px 0;">No hay plugins configurados todavía.</p>`;
        return;
    }
    const sourceLabel = { modrinth: '🟢 Modrinth', github: '🐙 GitHub', direct: '🔗 Directo' };
    const sourceColor = { modrinth: '#a6e3a1', github: '#89b4fa', direct: '#fab387' };
    container.innerHTML = plugins.map(p => `
        <div style="display:flex; justify-content:space-between; align-items:center; padding:8px; border-bottom:1px solid #313244;">
            <div>
                <span style="color:${sourceColor[p.source] || '#cdd6f4'}; font-size:0.75em; font-weight:bold;">${sourceLabel[p.source] || p.source}</span>
                <div style="font-family:monospace; color:#cdd6f4;">${escapeHtml(p.value)}</div>
            </div>
            <button class="secondary-btn vp-remove-btn" data-source="${escapeHtml(p.source)}" data-value="${escapeHtml(p.value)}" style="color:#f38ba8; border-color:#f38ba8; font-size:0.8em; padding:4px 10px;">🗑</button>
        </div>
    `).join('');

    container.querySelectorAll('.vp-remove-btn').forEach(btn => {
        btn.onclick = async () => {
            const ok = await showConfirm(`¿Quitar '${btn.dataset.value}' de la lista?`, 'Quitar plugin');
            if (!ok) return;
            await removeVelocityPlugin(currentServerId, btn.dataset.source, btn.dataset.value);
            listVelocityPlugins(currentServerId);
        };
    });
}

export function handleFileContentEvent(data) {
    if (data.scope === "packwiz") packwizExplorer.renderFileContent(data);
    else if (data.scope === "server_root") serverExplorer.renderFileContent(data);
}

export let currentServerId = null;
export let currentServerType = null;
export let currentMcVersion = null;

const viewGrid = document.getElementById('view-grid');
const viewDetail = document.getElementById('view-server-detail');

// Elementos del DOM
const titleEl = document.getElementById('detail-title');
const badgeEl = document.getElementById('detail-badge');
const statusTextEl = document.getElementById('detail-status-text');

const bufferedPackwizLog = createLogBuffer(() => document.getElementById('packwiz-console'));
const bufferedUpdateLog = createLogBuffer(() => document.getElementById('update-console'));

// Evita inyectar HTML/JS desde nombres de mods o archivos que no controlamos
// (vienen de Modrinth/CurseForge o de lo que suba cualquiera al modpack).
function escapeHtml(str) {
    return String(str ?? '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

export function initServerDetail() {
    packwizExplorer.init();
    serverExplorer.init();

    const btnExplorerPackwiz = document.getElementById('btn-explorer-packwiz');
    const btnExplorerServer = document.getElementById('btn-explorer-server');
    const panelPackwiz = document.getElementById('explorer-panel-packwiz');
    const panelServer = document.getElementById('explorer-panel-server');

    if (btnExplorerPackwiz && btnExplorerServer) {
        btnExplorerPackwiz.addEventListener('click', () => {
            btnExplorerPackwiz.classList.add('active');
            btnExplorerServer.classList.remove('active');
            panelPackwiz.classList.remove('hidden');
            panelServer.classList.add('hidden');
        });

        btnExplorerServer.addEventListener('click', () => {
            btnExplorerServer.classList.add('active');
            btnExplorerPackwiz.classList.remove('active');
            panelServer.classList.remove('hidden');
            panelPackwiz.classList.add('hidden');
            // Carga (o recarga) recién al entrar a esta sub-pestaña
            serverExplorer.setServerId(currentServerId);
            serverExplorer.listFiles();
        });
    }

    const btnSvOpenOs = document.getElementById('btn-sv-open-os');
    if (btnSvOpenOs) btnSvOpenOs.onclick = () => openServerFolder(currentServerId);



    // ==========================================
    // PESTAÑA "PLUGINS (VELOCITY)"
    // ==========================================

    const btnUpdateVelocityPlugins = document.getElementById('btn-update-velocity-plugins');
    if (btnUpdateVelocityPlugins) {
        withGuard(btnUpdateVelocityPlugins, async () => {
            const ok = await showConfirm(
                `Esto va a:\n• Volver a chequear Modrinth/GitHub/enlaces directos\n• Detener y reiniciar el servidor si estaba corriendo\n\n¿Continuar?`,
                'Actualizar Plugins de Velocity'
            );
            if (!ok) return;
            await syncVelocityPluginsNow(currentServerId);
        }, '⏳ Actualizando...');
    }

    const btnMotd = document.getElementById('btn-set-motd');
    if (btnMotd) {
        withGuard(btnMotd, async () => {
            const input = document.getElementById('motd-input');
            const motd = input.value.trim();
            if (!motd) return alert("Escribe un MOTD.");
            await setMotd(currentServerId, motd);
        });
    }

    const btnPort = document.getElementById('btn-set-port');
    if (btnPort) {
        withGuard(btnPort, async () => {
            const input = document.getElementById('port-input');
            const port = parseInt(input.value, 10);
            if (!port || port < 1 || port > 65535) return alert("Puerto inválido (1-65535).");
            const ok = await showConfirm(
                `Vas a cambiar el puerto a ${port}.\nSe aplica recién la próxima vez que el servidor arranque o se reinicie.\n\n¿Continuar?`,
                'Cambiar Puerto'
            );
            if (!ok) return;
            await setPort(currentServerId, port);
        });
    }

    const btnIcon = document.getElementById('btn-upload-icon');
    if (btnIcon) {
        withGuard(btnIcon, async () => {
            const fileInput = document.getElementById('server-icon-input');
            if (!fileInput || fileInput.files.length === 0) return alert("Selecciona una imagen");
            const file = fileInput.files[0];
            await new Promise((resolve) => {
                const reader = new FileReader();
                reader.onload = async () => {
                    const base64 = reader.result.split(',')[1];
                    await uploadServerIcon(currentServerId, base64);
                    resolve();
                };
                reader.readAsDataURL(file);
            });
            fileInput.value = "";
        }, '⏳ Subiendo...');
    }

    const btnAddModrinth = document.getElementById('btn-vp-add-modrinth');
    if (btnAddModrinth) {
        withGuard(btnAddModrinth, async () => {
            const input = document.getElementById('vp-add-modrinth');
            const value = input.value.trim();
            if (!value) return;
            await addVelocityPlugin(currentServerId, 'modrinth', value);
            input.value = '';
            listVelocityPlugins(currentServerId);
        }, '⏳...');
    }

    const btnAddGithub = document.getElementById('btn-vp-add-github');
    if (btnAddGithub) {
        withGuard(btnAddGithub, async () => {
            const input = document.getElementById('vp-add-github');
            const value = input.value.trim();
            if (!value) return;
            if (!value.includes('/')) return alert("Formato esperado: autor/repositorio");
            await addVelocityPlugin(currentServerId, 'github', value);
            input.value = '';
            listVelocityPlugins(currentServerId);
        }, '⏳...');
    }

    const btnAddDirect = document.getElementById('btn-vp-add-direct');
    if (btnAddDirect) {
        withGuard(btnAddDirect, async () => {
            const input = document.getElementById('vp-add-direct');
            const value = input.value.trim();
            if (!value) return;
            if (!/^https?:\/\//i.test(value)) return alert("Tiene que ser una URL http(s) completa.");
            await addVelocityPlugin(currentServerId, 'direct', value);
            input.value = '';
            listVelocityPlugins(currentServerId);
        }, '⏳...');
    }

    const btnVpRefresh = document.getElementById('btn-vp-refresh');
    if (btnVpRefresh) btnVpRefresh.onclick = () => listVelocityPlugins(currentServerId);

    const btnVpSaveMcHint = document.getElementById('btn-vp-save-mc-hint');
    if (btnVpSaveMcHint) {
        withGuard(btnVpSaveMcHint, async () => {
            const value = document.getElementById('vp-mc-version-hint').value.trim();
            await setVelocityMcVersionHint(currentServerId, value || null);
        }, '⏳...');
    }

    // ==========================================
    // ACCIONES BÁSICAS DEL SERVIDOR
    // ==========================================

    const btnConsoleSend = document.getElementById('btn-console-send');
    const consoleInput = document.getElementById('console-cmd-input');
    if (btnConsoleSend && consoleInput) {
        const send = () => {
            const cmd = consoleInput.value.trim();
            if (!cmd) return;
            appendLine(`> ${cmd}`);
            sendConsoleCommand(currentServerId, cmd);
            consoleInput.value = '';
        };
        btnConsoleSend.onclick = send;
        consoleInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') send();
        });
    }

    // Botón Volver
    const btnBack = document.getElementById('btn-back-grid');
    if (btnBack) {
        btnBack.addEventListener('click', async () => {
            await closeLogs();
            currentServerId = null;
            viewDetail.classList.add('hidden');
            viewGrid.classList.remove('hidden');
        });
    }

    // Pestañas
    const tabBtns = document.querySelectorAll('.tab-btn[data-tab]');
    const tabPanes = document.querySelectorAll('.tab-pane');

    tabBtns.forEach(btn => {
        btn.addEventListener('click', () => {
            tabBtns.forEach(b => b.classList.remove('active'));
            tabPanes.forEach(p => p.classList.add('hidden'));

            btn.classList.add('active');
            const targetPane = document.getElementById(btn.dataset.tab);
            if (targetPane) targetPane.classList.remove('hidden');
        });
    });

    const btnStart = document.getElementById('btn-detail-start');
    if (btnStart) {
        withGuard(btnStart, async () => {
            await sendAction('start_server', currentServerId);
            setTimeout(() => openLogs(currentServerId), 1500);
        });
    }

    const btnStop = document.getElementById('btn-detail-stop');
    if (btnStop) withGuard(btnStop, () => sendAction('stop_server', currentServerId));

    const btnRestart = document.getElementById('btn-detail-restart');
    if (btnRestart) {
        withGuard(btnRestart, async () => {
            await sendAction('restart_server', currentServerId);
            setTimeout(() => openLogs(currentServerId), 1500);
        });
    }

    const btnDelete = document.getElementById('btn-detail-delete');
    if (btnDelete) {
        withGuard(btnDelete, async () => {
            const deleted = await confirmDelete(currentServerId);
            if (deleted) {
                await closeLogs();
                currentServerId = null;
                viewDetail.classList.add('hidden');
                viewGrid.classList.remove('hidden');
            }
        });
    }

    const btnOpenFolder = document.getElementById('btn-open-folder');
    if (btnOpenFolder) {
        btnOpenFolder.onclick = () => openServerFolder(currentServerId);
    }

    // ==========================================
    // PESTAÑA "ACTUALIZACIONES"
    // ==========================================

    const btnUpdateEngine = document.getElementById('btn-update-engine');
    if (btnUpdateEngine) {
        withGuard(btnUpdateEngine, async () => {
            const ok = await showConfirm(
                `Esto va a:\n• Detener el servidor si está corriendo\n• Descargar la última compilación estable del motor (${currentServerType}) para MC ${currentMcVersion}\n• Borrar binarios viejos y redescargar el jar\n• Reiniciar el servidor si estaba corriendo\n\nNo toca mods ni plugins.\n\n¿Continuar?`,
                'Actualizar Compilado del Motor'
            );
            if (!ok) return;

            appendUpdateLog(`> Buscando la última compilación de ${currentServerType} para MC ${currentMcVersion}...`);
            updateStatus("Buscando última versión del motor...", "#f9e2af");
            let loaderVersion = null;
            try {
                loaderVersion = await getLatestLoaderVersion(currentServerType, currentMcVersion);
                if (['fabric', 'forge', 'neoforge'].includes(currentServerType) && !loaderVersion) {
                    appendUpdateLog(`❌ No encontré una versión de ${currentServerType} disponible para MC ${currentMcVersion}.`);
                    return alert(`No encontré una versión de ${currentServerType} disponible para MC ${currentMcVersion}.`);
                }
                if (loaderVersion) {
                    appendUpdateLog(`✔ Última versión encontrada: ${loaderVersion}`);
                }
            } catch (e) {
                appendUpdateLog(`❌ Error buscando la última versión del motor: ${e}`);
                return alert("Error buscando la última versión del motor: " + e);
            }

            await updateAllServer(currentServerId, loaderVersion, false, true);
        }, '⏳ Actualizando...');
    }

    const btnRepairEngine = document.getElementById('btn-repair-engine');
    if (btnRepairEngine) {
        withGuard(btnRepairEngine, async () => {
            const ok = await showConfirm(
                `Esto va a:\n• Detener el servidor si está corriendo\n• Borrar el jar y las librerías actuales, aunque ya estén "al día"\n• Volver a descargar/instalar el motor desde cero (${currentServerType}, MC ${currentMcVersion})\n• Reiniciar el servidor si estaba corriendo\n\nUsalo si sospechás que el jar o las librerías se corrompieron.\n\n¿Continuar?`,
                'Reparar Motor'
            );
            if (!ok) return;

            appendUpdateLog(`> Reparando motor (${currentServerType} MC ${currentMcVersion}) — reinstalación forzada...`);
            let loaderVersion = null;
            if (['fabric', 'forge', 'neoforge'].includes(currentServerType)) {
                try {
                    loaderVersion = await getLatestLoaderVersion(currentServerType, currentMcVersion);
                } catch (e) {
                    appendUpdateLog(`❌ Error buscando la versión del cargador: ${e}`);
                    return alert("Error buscando la versión del cargador: " + e);
                }
            }

            await updateAllServer(currentServerId, loaderVersion, false, true, true); // 👈 force=true
        }, '⏳ Reparando...');
    }

    const btnUpdateMods = document.getElementById('btn-update-mods');
    if (btnUpdateMods) {
        withGuard(btnUpdateMods, async () => {
            const ok = await showConfirm(
                `Esto va a:\n• Detener el servidor si está corriendo\n• Buscar versiones más nuevas de todos los mods/plugins del pack\n• Sincronizar los archivos actualizados al servidor\n• Reiniciar el servidor si estaba corriendo\n\nNo toca el jar del motor.\n\n¿Continuar?`,
                'Actualizar Mods/Plugins'
            );
            if (!ok) return;
            await updateAllServer(currentServerId, null, true, false);
        }, '⏳ Actualizando...');
    }

    const btnUpdateAll = document.getElementById('btn-update-all');
    if (btnUpdateAll) {
        withGuard(btnUpdateAll, async () => {
            const ok = await showConfirm(
                `Esto va a:\n• Actualizar mods/plugins a sus últimas versiones\n• Actualizar el motor (${currentServerType}) a la última compilación para MC ${currentMcVersion}\n• Borrar binarios viejos y redescargar todo\n\n¿Continuar?`,
                'Actualizar Todo'
            );
            if (!ok) return;

            appendUpdateLog(`> Buscando la última compilación de ${currentServerType} para MC ${currentMcVersion}...`);
            updateStatus("Buscando última versión del motor...", "#f9e2af");
            let loaderVersion = null;
            try {
                loaderVersion = await getLatestLoaderVersion(currentServerType, currentMcVersion);
                if (['fabric', 'forge', 'neoforge'].includes(currentServerType) && !loaderVersion) {
                    appendUpdateLog(`❌ No encontré una versión de ${currentServerType} disponible para MC ${currentMcVersion}.`);
                    return alert(`No encontré una versión de ${currentServerType} disponible para MC ${currentMcVersion}.`);
                }
                if (loaderVersion) {
                    appendUpdateLog(`✔ Última versión encontrada: ${loaderVersion}`);
                }
            } catch (e) {
                appendUpdateLog(`❌ Error buscando la última versión del motor: ${e}`);
                return alert("Error buscando la última versión del motor: " + e);
            }

            await updateAllServer(currentServerId, loaderVersion, true, true);
        }, '⏳ Actualizando...');
    }

    const btnClearUpdateConsole = document.getElementById('btn-clear-update-console');
    if (btnClearUpdateConsole) {
        btnClearUpdateConsole.onclick = () => {
            const consoleEl = document.getElementById('update-console');
            if (consoleEl) consoleEl.textContent = 'Esperando actualizaciones...';
        };
    }

    // ==========================================
    // BOTONES DE PACKWIZ
    // ==========================================

    const btnClearConsole = document.getElementById('btn-clear-pw-console');
    if (btnClearConsole) {
        btnClearConsole.onclick = () => {
            const consoleEl = document.getElementById('packwiz-console');
            if (consoleEl) consoleEl.textContent = 'Esperando comandos...';
        };
    }

    const btnAdd = document.getElementById('btn-pw-add');
    if (btnAdd) {
        withGuard(btnAdd, async () => {
            const query = document.getElementById('pw-add-input').value.trim();
            if (query) await addMod(currentServerId, query);
        }, '⏳ Añadiendo...');
    }

    const btnRemove = document.getElementById('btn-pw-remove');
    if (btnRemove) {
        withGuard(btnRemove, async () => {
            const query = document.getElementById('pw-remove-input').value.trim();
            if (query) await removeMod(currentServerId, query);
        }, '⏳ Eliminando...');
    }

    const btnSyncServer = document.getElementById('btn-pw-sync-server');
    if (btnSyncServer) {
        withGuard(btnSyncServer, async () => {
            const ok = await showConfirm(
                `Esto va a:\n• Sincronizar los mods/plugins actuales del pack directamente en este servidor\n• Reiniciar el servidor si estaba corriendo\n\nNO publica nada para los clientes ni toca el modpack público.\n\n¿Continuar?`,
                'Sincronizar solo con el Servidor'
            );
            if (!ok) return;
            await syncPackToServer(currentServerId);
        }, '⏳ Sincronizando...');
    }



    const btnRefreshTree = document.getElementById('btn-pw-refresh-tree');
    if (btnRefreshTree) btnRefreshTree.onclick = () => listPackwizFiles(currentServerId);

    const btnQuickPublish = document.getElementById('btn-pw-publish-quick');
    if (btnQuickPublish) {
        withGuard(btnQuickPublish, async () => {
            const packName = prompt("Confirma el ID para publicar el Modpack:", currentServerId);
            if (packName) await publishModpack(currentServerId, packName, null);
        });
    }

    const btnPublish = document.getElementById('btn-pw-publish');
    if (btnPublish) {
        withGuard(btnPublish, async () => {
            const packKey = document.getElementById('pw-publish-input').value.trim();
            if (!packKey) return alert("Escribe el nombre de la carpeta destino (Ej: lumineria_1_21)");

            const imageInput = document.getElementById('pw-publish-image');
            const file = imageInput && imageInput.files.length > 0 ? imageInput.files[0] : null;

            if (!file) {
                await publishModpack(currentServerId, packKey, null);
                return;
            }

            // 👇 envuelto en Promise para que el guard espere a que termine de verdad
            await new Promise((resolve) => {
                const reader = new FileReader();
                reader.onload = async () => {
                    const base64 = reader.result.split(',')[1];
                    await publishModpack(currentServerId, packKey, { filename: file.name, data_base64: base64 });
                    imageInput.value = "";
                    resolve();
                };
                reader.readAsDataURL(file);
            });
        }, '⏳ Publicando...');
    }

    const btnUnpublish = document.getElementById('btn-pw-unpublish');
    if (btnUnpublish) {
        withGuard(btnUnpublish, async () => {
            const packKey = document.getElementById('pw-publish-input').value.trim();
            if (packKey) await unpublishModpack(currentServerId, packKey);
            else alert("Escribe el nombre de la carpeta a quitar");
        });
    }

    const btnRefreshMods = document.getElementById('btn-pw-refresh-list');
    if (btnRefreshMods) {
        btnRefreshMods.onclick = () => listPackwizMods(currentServerId);
    }

}

export async function openServerDetail(server) {
    currentServerId = server.id;
    currentServerType = server.server_type;
    currentMcVersion = server.mc_version;

    if (titleEl) titleEl.innerText = server.display_name;
    if (badgeEl) badgeEl.innerText = `${server.server_type} ${server.mc_version}`;
    updateDetailStatus(server.status);

    const publishInput = document.getElementById('pw-publish-input');
    if (publishInput) publishInput.value = server.id;

    const portInput = document.getElementById('port-input');
    if (portInput) portInput.value = server.port;

    if (viewGrid) viewGrid.classList.add('hidden');
    const connView = document.getElementById('view-connection');
    if (connView) connView.classList.add('hidden');
    if (viewDetail) viewDetail.classList.remove('hidden');

    const defaultTab = document.querySelector('.tab-btn[data-tab="tab-console"]');
    if (defaultTab) defaultTab.click();

    await openLogs(server.id);

    const isVelocity = server.server_type === 'velocity';
    const isModLoader = ['fabric', 'forge', 'neoforge', 'arclight'].includes(server.server_type);
    const usesDirectPlugins = !isModLoader;

    document.getElementById('tab-btn-packwiz')?.classList.toggle('hidden', isVelocity);
    document.getElementById('tab-btn-velocity-plugins')?.classList.toggle('hidden', !usesDirectPlugins);
    document.getElementById('card-update-mods-packwiz')?.classList.toggle('hidden', isVelocity);
    document.getElementById('card-update-plugins-velocity')?.classList.toggle('hidden', !usesDirectPlugins);

    if (usesDirectPlugins) listVelocityPlugins(server.id);
    if (!isVelocity) listPackwizMods(server.id);

    packwizExplorer.setServerId(server.id);
    serverExplorer.setServerId(server.id);

    const btnExplorerPackwiz = document.getElementById('btn-explorer-packwiz');
    const btnExplorerServer = document.getElementById('btn-explorer-server');
    const panelPackwiz = document.getElementById('explorer-panel-packwiz');
    const panelServer = document.getElementById('explorer-panel-server');

    if (isVelocity) {
        btnExplorerPackwiz?.classList.add('hidden');
        btnExplorerServer?.classList.add('active');
        btnExplorerPackwiz?.classList.remove('active');
        panelPackwiz?.classList.add('hidden');
        panelServer?.classList.remove('hidden');
        serverExplorer.listFiles();
    } else {
        btnExplorerPackwiz?.classList.remove('hidden');
        btnExplorerPackwiz?.classList.add('active');
        btnExplorerServer?.classList.remove('active');
        panelPackwiz?.classList.remove('hidden');
        panelServer?.classList.add('hidden');
        packwizExplorer.listFiles();
    }
}

export function updateDetailStatus(status) {
    if (!statusTextEl) return;
    const labels = {
        running: '🟢 En ejecución',
        stopped: '🔴 Detenido',
        restarting: '🟡 Reiniciando',
        missing: '⚠ Falta contenedor'
    };
    statusTextEl.innerText = labels[status] || status;
}

// RENDERIZAR TODO LO QUE TIENE PACKWIZ (Mods, Resourcepacks, Shaders, Configs)
export function renderPackwizMods(mods) {
    const container = document.getElementById('pw-mods-list-container');
    if (!container) return;

    if (mods.length === 0) {
        container.innerHTML = `<p style="color: #6c7086; text-align: center; margin: 20px 0;">No hay archivos en este pack todavía. ¡Prueba agregando uno!</p>`;
        return;
    }

    container.innerHTML = `
        <table style="width: 100%; border-collapse: collapse; text-align: left; font-size: 0.9em; color: #cdd6f4;">
            <thead>
                <tr style="border-bottom: 2px solid #313244; color: #a6adc8;">
                    <th style="padding: 8px;">Tipo</th>
                    <th style="padding: 8px;">Nombre de Archivo / Recurso</th>
                    <th style="padding: 8px; text-align: right;">Lado (Side)</th>
                </tr>
            </thead>
            <tbody>
                ${mods.map(mod => {
        let sideBadge = "";
        if (mod.side === "client") {
            sideBadge = `<span class="badge" style="background: #89b4fa; color: #11111b; font-weight: bold; font-size:0.75rem; padding: 2px 6px; border-radius:4px;">Solo Cliente</span>`;
        } else if (mod.side === "server") {
            sideBadge = `<span class="badge" style="background: #f9e2af; color: #11111b; font-weight: bold; font-size:0.75rem; padding: 2px 6px; border-radius:4px;">Solo Servidor</span>`;
        } else {
            sideBadge = `<span class="badge" style="background: #a6e3a1; color: #11111b; font-weight: bold; font-size:0.75rem; padding: 2px 6px; border-radius:4px;">Ambos</span>`;
        }

        let typeBadge = `<span style="color: #cba6f7;">📦 Mod</span>`;
        if (mod.filename.includes("resourcepacks/")) {
            typeBadge = `<span style="color: #fab387;">🎨 Texturas</span>`;
        } else if (mod.filename.includes("config/")) {
            typeBadge = `<span style="color: #94e2d5;">⚙️ Config</span>`;
        } else if (mod.filename.includes("shaderpacks/")) {
            typeBadge = `<span style="color: #f9e2af;">🔮 Shader</span>`;
        } else if (mod.filename.includes("plugins/")) {
            typeBadge = `<span style="color: #89b4fa;">🔌 Plugin</span>`;
        }

        return `
                        <tr style="border-bottom: 1px solid #313244;">
                            <td style="padding: 8px; font-weight: bold;">${typeBadge}</td>
                            <td style="padding: 8px; color: #cdd6f4;">
                                <div style="font-weight: bold; color: #f5c2e7;">${escapeHtml(mod.name)}</div>
                                <div style="font-family: monospace; font-size: 0.8rem; color: #6c7086;">${escapeHtml(mod.filename)}</div>
                            </td>
                            <td style="padding: 8px; text-align: right;">${sideBadge}</td>
                        </tr>
                    `;
    }).join('')}
            </tbody>
        </table>
    `;
}


export function appendPackwizLog(text) {
    if (bufferedPackwizLog._first !== false) {
        const el = document.getElementById('packwiz-console');
        if (el && el.textContent === 'Esperando comandos...') el.textContent = '';
    }
    bufferedPackwizLog(text);
}

// Consola dedicada de la pestaña "Actualizaciones"
export function appendUpdateLog(text) {
    const el = document.getElementById('update-console');
    if (el && el.textContent === 'Esperando actualizaciones...') el.textContent = '';
    bufferedUpdateLog(text);
}

