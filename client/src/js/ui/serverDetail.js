import {
    sendAction, confirmDelete, openServerFolder,
    addMod, removeMod, uploadMod, publishModpack,
    listPackwizMods, unpublishModpack, sendConsoleCommand
} from '../features/actions.js';
import { openLogs, closeLogs } from '../features/logs.js';
import { appendLine } from '../features/logs.js';

export let currentServerId = null;

const viewGrid = document.getElementById('view-grid');
const viewDetail = document.getElementById('view-server-detail');

// Elementos del DOM
const titleEl = document.getElementById('detail-title');
const badgeEl = document.getElementById('detail-badge');
const statusTextEl = document.getElementById('detail-status-text');

export function initServerDetail() {

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
    const btnUpload = document.getElementById('btn-pw-upload');
    if (btnUpload) {
        btnUpload.onclick = () => {
            const fileInput = document.getElementById('pw-upload-input');
            const folderSelect = document.getElementById('pw-upload-folder');

            if (!fileInput || !folderSelect) return;
            const targetFolder = folderSelect.value;

            if (fileInput.files.length === 0) return alert("Selecciona un archivo");

            const file = fileInput.files[0];
            const reader = new FileReader();

            reader.onload = async () => {
                const base64 = reader.result.split(',')[1];
                await uploadMod(currentServerId, file.name, base64, targetFolder);
                fileInput.value = "";
            };
            reader.readAsDataURL(file);
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
}

export async function openServerDetail(server) {
    currentServerId = server.id;

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