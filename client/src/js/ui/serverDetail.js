import {
    sendAction, confirmDelete, openServerFolder,
    addMod, removeMod, uploadMod, publishModpack,
    listPackwizMods, unpublishModpack, sendConsoleCommand, listPackwizFiles,
    readFile, writeFile, deleteFile, createDirectory, updateAllServer
} from '../features/actions.js';
import { openLogs, closeLogs } from '../features/logs.js';
import { appendLine } from '../features/logs.js';
import { showConfirm } from '../ui/confirmModal.js';
import { getLatestLoaderVersion } from '../features/creator.js';

let currentSelectedPath = ".";
let lastPackwizTree = [];

export let currentServerId = null;
export let currentServerType = null;
export let currentMcVersion = null;

const viewGrid = document.getElementById('view-grid');
const viewDetail = document.getElementById('view-server-detail');

// Elementos del DOM
const titleEl = document.getElementById('detail-title');
const badgeEl = document.getElementById('detail-badge');
const statusTextEl = document.getElementById('detail-status-text');

export function initServerDetail() {

    const btnUpdateAll = document.getElementById('btn-detail-update-all');
    if (btnUpdateAll) {
        btnUpdateAll.onclick = async () => {
            const ok = await showConfirm(
                `Esto va a:\n• Actualizar mods/plugins a sus últimas versiones\n• Actualizar el motor (${currentServerType}) a la última compilación para MC ${currentMcVersion}\n• Borrar binarios viejos y redescargar todo\n\n¿Continuar?`,
                'Actualizar Todo'
            );
            if (!ok) return;

            updateStatus("Buscando última versión del motor...", "#f9e2af");
            let loaderVersion = null;
            try {
                loaderVersion = await getLatestLoaderVersion(currentServerType, currentMcVersion);
                if (['fabric', 'forge', 'neoforge'].includes(currentServerType) && !loaderVersion) {
                    return alert(`No encontré una versión de ${currentServerType} disponible para MC ${currentMcVersion}.`);
                }
            } catch (e) {
                return alert("Error buscando la última versión del motor: " + e);
            }

            updateAllServer(currentServerId, loaderVersion);
        };
    }

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

    // Botones de acción del servidor
    const btnStart = document.getElementById('btn-detail-start');
    if (btnStart) {
        btnStart.onclick = () => {
            sendAction('start_server', currentServerId);
            // Volvemos a pedir los logs después de 1.5s para asegurarnos de que el contenedor ya existe
            setTimeout(() => openLogs(currentServerId), 1500);
        };
    }

    const btnStop = document.getElementById('btn-detail-stop');
    if (btnStop) btnStop.onclick = () => sendAction('stop_server', currentServerId);

    const btnRestart = document.getElementById('btn-detail-restart');
    if (btnRestart) {
        btnRestart.onclick = () => {
            sendAction('restart_server', currentServerId);
            setTimeout(() => openLogs(currentServerId), 1500);
        };
    }

    const btnDelete = document.getElementById('btn-detail-delete');
    if (btnDelete) {
        btnDelete.onclick = async () => {
            const deleted = await confirmDelete(currentServerId);
            if (deleted) {
                await closeLogs();
                currentServerId = null;
                viewDetail.classList.add('hidden');
                viewGrid.classList.remove('hidden');
            }
        };
    }

    const btnClearConsole = document.getElementById('btn-clear-pw-console');
    if (btnClearConsole) {
        btnClearConsole.onclick = () => {
            const consoleEl = document.getElementById('packwiz-console');
            if (consoleEl) consoleEl.textContent = 'Esperando comandos...';
        };
    }

    // Guardas de seguridad: Si no se encuentra el botón de abrir carpeta local, la app ya no crasheará
    const btnOpenFolder = document.getElementById('btn-open-folder');
    if (btnOpenFolder) {
        btnOpenFolder.onclick = () => openServerFolder(currentServerId);
    }

    // ==========================================
    // BOTONES DE PACKWIZ
    // ==========================================

    // AÑADIR MOD
    const btnAdd = document.getElementById('btn-pw-add');
    if (btnAdd) {
        btnAdd.onclick = () => {
            const query = document.getElementById('pw-add-input').value.trim();
            if (query) addMod(currentServerId, query);
        };
    }

    // ELIMINAR MOD / ARCHIVO
    const btnRemove = document.getElementById('btn-pw-remove');
    if (btnRemove) {
        btnRemove.onclick = () => {
            const query = document.getElementById('pw-remove-input').value.trim();
            if (query) removeMod(currentServerId, query);
        };
    }

    // SUBIR ARCHIVO GENÉRICO (JAR, ZIP, CONF)
    // SUBIR ARCHIVO(S) EN EL ÁRBOL
    const btnUpload = document.getElementById('btn-pw-upload');
    if (btnUpload) {
        btnUpload.onclick = async () => {
            const fileInput = document.getElementById('pw-upload-input');
            if (!fileInput || fileInput.files.length === 0) return alert("Selecciona al menos un archivo");

            // Usamos la variable global del árbol en lugar de un <select>
            const targetFolder = currentSelectedPath;

            // Iteramos sobre todos los archivos subidos para permitir selección múltiple
            for (let i = 0; i < fileInput.files.length; i++) {
                const file = fileInput.files[i];
                const reader = new FileReader();

                await new Promise((resolve) => {
                    reader.onload = async () => {
                        const base64 = reader.result.split(',')[1];
                        await uploadMod(currentServerId, file.name, base64, targetFolder);
                        resolve();
                    };
                    reader.readAsDataURL(file);
                });
            }

            fileInput.value = "";
            alert(`¡${fileInput.files.length} archivo(s) subido(s) con éxito!`);
            listPackwizFiles(currentServerId); // Recargamos el árbol
            listPackwizMods(currentServerId); // Recargamos la lista visual
        };
    }

    // BOTONES EXTRA DEL GESTOR (Refrescar y Publicar Rápido)
    const btnRefreshTree = document.getElementById('btn-pw-refresh-tree');
    if (btnRefreshTree) btnRefreshTree.onclick = () => listPackwizFiles(currentServerId);

    const btnQuickPublish = document.getElementById('btn-pw-publish-quick');
    if (btnQuickPublish) {
        btnQuickPublish.onclick = () => {
            const packName = prompt("Confirma el ID para publicar el Modpack:", currentServerId);
            if (packName) {
                publishModpack(currentServerId, packName, null);
            }
        };
    }

    // PUBLICAR AL VPS
    const btnPublish = document.getElementById('btn-pw-publish');
    if (btnPublish) {
        btnPublish.onclick = () => {
            const packKey = document.getElementById('pw-publish-input').value.trim();
            if (!packKey) return alert("Escribe el nombre de la carpeta destino (Ej: lumineria_1_21)");

            const imageInput = document.getElementById('pw-publish-image');
            const file = imageInput && imageInput.files.length > 0 ? imageInput.files[0] : null;

            if (!file) {
                publishModpack(currentServerId, packKey, null);
                return;
            }

            const reader = new FileReader();
            reader.onload = () => {
                const base64 = reader.result.split(',')[1];
                publishModpack(currentServerId, packKey, { filename: file.name, data_base64: base64 });
                imageInput.value = "";
            };
            reader.readAsDataURL(file);
        };
    }

    const btnUnpublish = document.getElementById('btn-pw-unpublish');
    if (btnUnpublish) {
        btnUnpublish.onclick = () => {
            const packKey = document.getElementById('pw-publish-input').value.trim();
            if (packKey) unpublishModpack(currentServerId, packKey);
            else alert("Escribe el nombre de la carpeta a quitar");
        };
    }

    const btnRefreshMods = document.getElementById('btn-pw-refresh-list');
    if (btnRefreshMods) {
        btnRefreshMods.onclick = () => listPackwizMods(currentServerId);
    }

    // GUARDAR ARCHIVO EDITADO
    const btnSaveFile = document.getElementById('btn-pw-save-file');
    if (btnSaveFile) {
        btnSaveFile.onclick = () => {
            const content = document.getElementById('pw-file-editor').value;
            writeFile(currentServerId, currentSelectedPath, content);
        };
    }

    // ELIMINAR ARCHIVO O CARPETA
    const btnDeleteFile = document.getElementById('btn-pw-delete-file');
    if (btnDeleteFile) {
        btnDeleteFile.onclick = async () => {
            const ok = await showConfirm(`¿Estás seguro de eliminar '${currentSelectedPath}'?\nEsto lo quitará del modpack.`, 'Eliminar Archivo');
            if (ok) {
                deleteFile(currentServerId, currentSelectedPath);

                // Resetear UI a la raíz tras borrar
                currentSelectedPath = ".";
                document.getElementById('pw-selected-path').innerText = "/ (Raíz)";
                document.getElementById('pw-folder-panel').style.display = 'flex';
                document.getElementById('pw-folder-panel').classList.remove('hidden');
                document.getElementById('pw-file-panel').style.display = 'none';
                document.getElementById('pw-file-panel').classList.add('hidden');

                // Recargar árbol tras un segundo para dar tiempo al OS
                setTimeout(() => {
                    listPackwizFiles(currentServerId);
                    listPackwizMods(currentServerId);
                }, 500);
            }
        };
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

    if (viewGrid) viewGrid.classList.add('hidden');
    const connView = document.getElementById('view-connection');
    if (connView) connView.classList.add('hidden');
    if (viewDetail) viewDetail.classList.remove('hidden');

    const defaultTab = document.querySelector('.tab-btn[data-tab="tab-console"]');
    if (defaultTab) defaultTab.click();

    await openLogs(server.id);
    listPackwizMods(server.id);

    currentSelectedPath = ".";
    document.getElementById('pw-selected-path').innerText = "/ (Raíz)";
    listPackwizFiles(server.id);
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

        // Badge dinámico para identificar si es un Mod, un Config, o un ResourcePack
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
                                <div style="font-weight: bold; color: #f5c2e7;">${mod.name}</div>
                                <div style="font-family: monospace; font-size: 0.8rem; color: #6c7086;">${mod.filename}</div>
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
    const consoleEl = document.getElementById('packwiz-console');
    if (consoleEl) {
        if (consoleEl.textContent === 'Esperando comandos...') consoleEl.textContent = '';
        consoleEl.textContent += text + '\n';
        consoleEl.scrollTop = consoleEl.scrollHeight;
    }
}

export function renderPackwizTree(files) {
    lastPackwizTree = files; // Guardamos el estado
    const container = document.getElementById('pw-file-tree');
    if (!container) return;

    let html = '<ul style="list-style: none; padding-left: 10px; margin: 0;">';

    // Raíz siempre visible
    html += `
        <li style="margin: 2px 0;">
            <div class="tree-node ${currentSelectedPath === '.' ? 'selected' : ''}" data-path="." data-isdir="true" style="cursor: pointer; padding: 6px; border-radius: 4px; display: flex; align-items: center; gap: 8px; color: #a6e3a1; font-weight: bold;">
                🏠 / (Raíz del Modpack)
            </div>
        </li>
    `;

    // Función recursiva para dibujar hijos
    function drawNodes(nodes) {
        let out = '<ul style="list-style: none; padding-left: 20px; border-left: 1px solid #45475a; margin: 0;">';
        for (const file of nodes) {
            const icon = file.is_dir ? '📁' : '📄';
            const color = file.is_dir ? '#89b4fa' : '#cdd6f4';
            const isSelected = currentSelectedPath === file.path;

            out += `<li style="margin: 2px 0;">`;
            out += `<div class="tree-node ${isSelected ? 'selected' : ''}" data-path="${file.path}" data-isdir="${file.is_dir}" style="cursor: pointer; padding: 4px; border-radius: 4px; display:flex; align-items:center; gap:5px; color: ${color};">
                        ${icon} ${file.name}
                    </div>`;
            if (file.is_dir && file.children && file.children.length > 0) {
                out += drawNodes(file.children);
            }
            out += `</li>`;
        }
        out += '</ul>';
        return out;
    }

    html += drawNodes(files);
    html += '</ul>';
    container.innerHTML = html;

    // Eventos click para seleccionar
    // Eventos click para seleccionar
    container.querySelectorAll('.tree-node').forEach(node => {
        node.addEventListener('click', (e) => {
            e.stopPropagation();
            currentSelectedPath = node.dataset.path;
            const isDir = node.dataset.isdir === "true";

            document.getElementById('pw-selected-path').innerText = currentSelectedPath === "." ? "/ (Raíz)" : `/${currentSelectedPath}`;

            const folderPanel = document.getElementById('pw-folder-panel');
            const filePanel = document.getElementById('pw-file-panel');
            const btnDeleteDir = document.getElementById('btn-pw-delete-dir');

            if (isDir) {
                // Mostrar panel de carpeta
                folderPanel.classList.remove('hidden');
                folderPanel.style.display = 'flex';
                filePanel.classList.add('hidden');
                filePanel.style.display = 'none';

                // Mostrar botón de borrar carpeta SOLO si no es la raíz (".")
                if (currentSelectedPath === ".") {
                    btnDeleteDir.classList.add('hidden');
                } else {
                    btnDeleteDir.classList.remove('hidden');
                }

            } else {
                // Mostrar panel de archivo
                folderPanel.classList.add('hidden');
                folderPanel.style.display = 'none';
                filePanel.classList.remove('hidden');
                filePanel.style.display = 'flex';

                document.getElementById('pw-file-editor').value = "Cargando archivo...";
                readFile(currentServerId, currentSelectedPath);
            }

            renderPackwizTree(lastPackwizTree);
        });
    });

    // CREAR NUEVA CARPETA
    const btnCreateDir = document.getElementById('btn-pw-create-dir');
    if (btnCreateDir) {
        btnCreateDir.onclick = async () => {
            const folderName = prompt(`Crear nueva carpeta dentro de '${currentSelectedPath === "." ? "/ (Raíz)" : currentSelectedPath}':\nEscribe el nombre:`);
            if (folderName) {
                // Validación básica para evitar inyecciones de ruta
                if (folderName.includes("/") || folderName.includes("\\") || folderName.includes("..")) {
                    return alert("El nombre de la carpeta no puede contener barras o puntos suspensivos.");
                }

                const targetPath = currentSelectedPath === "." ? folderName : `${currentSelectedPath}/${folderName}`;
                await createDirectory(currentServerId, targetPath);

                // Damos medio segundo para que el OS cree la carpeta antes de recargar
                setTimeout(() => listPackwizFiles(currentServerId), 500);
            }
        };
    }

    // ELIMINAR CARPETA
    const btnDeleteDir = document.getElementById('btn-pw-delete-dir');
    if (btnDeleteDir) {
        btnDeleteDir.onclick = async () => {
            if (currentSelectedPath === ".") return alert("No puedes eliminar la raíz del modpack.");

            const ok = await showConfirm(`¿Estás seguro de eliminar TODA la carpeta '${currentSelectedPath}'?\nEsto borrará todos los archivos en su interior y los quitará del modpack.`, 'Eliminar Carpeta');
            if (ok) {
                deleteFile(currentServerId, currentSelectedPath);

                currentSelectedPath = ".";
                document.getElementById('pw-selected-path').innerText = "/ (Raíz)";

                setTimeout(() => {
                    listPackwizFiles(currentServerId);
                    listPackwizMods(currentServerId);
                }, 500);
            }
        };
    }
}

export function renderFileContent(data) {
    if (data.id !== currentServerId || data.path !== currentSelectedPath) return;
    const editor = document.getElementById('pw-file-editor');
    const saveBtn = document.getElementById('btn-pw-save-file');
    if (data.content === null) {
        editor.value = "⚠️ Este es un archivo binario (.jar, .zip) y no se puede editar.\nSi crees que debería existir, actualiza el árbol de archivos.";
        editor.disabled = true;
        saveBtn.disabled = true;
        saveBtn.style.opacity = "0.5";
    } else {
        editor.value = data.content;
        editor.disabled = false;
        saveBtn.disabled = false;
        saveBtn.style.opacity = "1";
    }
}

window.renderPackwizTree = renderPackwizTree