import { showConfirm } from '../ui/confirmModal.js';
import { withGuard } from '../ui/guard.js';
import {
    readFile as apiReadFile,
    writeFile as apiWriteFile,
    deleteFile as apiDeleteFile,
    createDirectory as apiCreateDirectory,
    uploadMod as apiUploadFile,
    listPackwizFiles as apiListFiles,
} from './actions.js';

function escapeHtml(str) {
    return String(str ?? '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

// ids: { tree, selectedPath, folderPanel, filePanel, uploadInput, btnUpload,
//        btnCreateDir, btnDeleteDir, fileEditor, btnSaveFile, btnDeleteFile, btnRefreshTree }
export function createFileExplorer({ scope, ids }) {
    let currentServerId = null;
    let currentSelectedPath = ".";
    let lastTree = [];
    // 👇 NUEVO: qué carpetas están "abiertas". La raíz arranca abierta para que
    // se vea algo apenas se carga; todo lo demás arranca cerrado (no se muestra
    // el árbol completo de una, como pediste).
    let expandedPaths = new Set(["."]);

    const el = (id) => document.getElementById(id);

    function setServerId(id) {
        currentServerId = id;
        currentSelectedPath = ".";
        expandedPaths = new Set(["."]); // 👈 reset del estado al cambiar de servidor
        const pathEl = el(ids.selectedPath);
        if (pathEl) pathEl.innerText = "/ (Raíz)";
        const folderPanel = el(ids.folderPanel);
        const filePanel = el(ids.filePanel);
        if (folderPanel) { folderPanel.style.display = 'flex'; folderPanel.classList.remove('hidden'); }
        if (filePanel) { filePanel.style.display = 'none'; filePanel.classList.add('hidden'); }
    }

    async function listFiles() {
        if (currentServerId) await apiListFiles(currentServerId, scope);
    }

    function toggleExpanded(path) {
        if (expandedPaths.has(path)) expandedPaths.delete(path);
        else expandedPaths.add(path);
    }

    function render(files) {
        lastTree = files;
        const container = el(ids.tree);
        if (!container) return;

        const rootExpanded = expandedPaths.has(".");
        const rootBullet = rootExpanded ? '▾' : '▸';

        let html = '<ul style="list-style: none; padding-left: 10px; margin: 0;">';
        html += `
            <li style="margin: 2px 0;">
                <div class="tree-node ${currentSelectedPath === '.' ? 'selected' : ''}" data-path="." data-isdir="true" style="cursor: pointer; padding: 6px; border-radius: 4px; display: flex; align-items: center; gap: 8px; color: #a6e3a1; font-weight: bold;">
                    <span style="display:inline-block; width:12px; text-align:center;">${rootBullet}</span> 🏠 / (Raíz)
                </div>
            </li>
        `;

        function drawNodes(nodes) {
            let out = '<ul style="list-style: none; padding-left: 20px; border-left: 1px solid #45475a; margin: 0;">';
            for (const file of nodes) {
                const icon = file.is_dir ? '📁' : '📄';
                const color = file.is_dir ? '#89b4fa' : '#cdd6f4';
                const isSelected = currentSelectedPath === file.path;
                const safePath = escapeHtml(file.path);
                const safeName = escapeHtml(file.name);
                const isExpanded = file.is_dir && expandedPaths.has(file.path);
                // 👇 la "viñeta": ▸ cerrado / ▾ abierto para carpetas, punto fijo para archivos
                const bullet = file.is_dir ? (isExpanded ? '▾' : '▸') : '·';

                out += `<li style="margin: 2px 0;">`;
                out += `<div class="tree-node ${isSelected ? 'selected' : ''}" data-path="${safePath}" data-isdir="${file.is_dir}" style="cursor: pointer; padding: 4px; border-radius: 4px; display:flex; align-items:center; gap:5px; color: ${color};">
                            <span style="display:inline-block; width:12px; text-align:center; color:#6c7086;">${bullet}</span> ${icon} ${safeName}
                        </div>`;
                // Solo dibujamos los hijos si la carpeta está expandida
                if (file.is_dir && isExpanded && file.children && file.children.length > 0) {
                    out += drawNodes(file.children);
                }
                out += `</li>`;
            }
            out += '</ul>';
            return out;
        }

        if (rootExpanded) {
            html += drawNodes(files);
        }
        html += '</ul>';
        container.innerHTML = html;

        container.querySelectorAll('.tree-node').forEach(node => {
            node.addEventListener('click', (e) => {
                e.stopPropagation();
                currentSelectedPath = node.dataset.path;
                const isDir = node.dataset.isdir === "true";

                const pathEl = el(ids.selectedPath);
                if (pathEl) pathEl.innerText = currentSelectedPath === "." ? "/ (Raíz)" : `/${currentSelectedPath}`;

                const folderPanel = el(ids.folderPanel);
                const filePanel = el(ids.filePanel);
                const btnDeleteDir = el(ids.btnDeleteDir);

                if (isDir) {
                    // 👇 NUEVO: clic en una carpeta = abrir/cerrar (toggle)
                    toggleExpanded(currentSelectedPath);

                    folderPanel.classList.remove('hidden'); folderPanel.style.display = 'flex';
                    filePanel.classList.add('hidden'); filePanel.style.display = 'none';
                    if (btnDeleteDir) {
                        if (currentSelectedPath === ".") btnDeleteDir.classList.add('hidden');
                        else btnDeleteDir.classList.remove('hidden');
                    }
                } else {
                    folderPanel.classList.add('hidden'); folderPanel.style.display = 'none';
                    filePanel.classList.remove('hidden'); filePanel.style.display = 'flex';
                    const editor = el(ids.fileEditor);
                    if (editor) editor.value = "Cargando archivo...";
                    apiReadFile(currentServerId, currentSelectedPath, scope);
                }
                render(lastTree);
            });
        });
    }

    function renderFileContent(data) {
        if (data.id !== currentServerId || data.path !== currentSelectedPath) return;
        const editor = el(ids.fileEditor);
        const saveBtn = el(ids.btnSaveFile);
        if (!editor || !saveBtn) return;
        if (data.content === null) {
            editor.value = "⚠️ Este es un archivo binario (.jar, .zip) y no se puede editar.";
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

    function init() {
        const btnRefreshTree = el(ids.btnRefreshTree);
        if (btnRefreshTree) btnRefreshTree.onclick = () => listFiles();

        const btnSaveFile = el(ids.btnSaveFile);
        if (btnSaveFile) {
            withGuard(btnSaveFile, async () => {
                const content = el(ids.fileEditor).value;
                await apiWriteFile(currentServerId, currentSelectedPath, content, scope);
            }, '⏳ Guardando...');
        }

        const btnDeleteFile = el(ids.btnDeleteFile);
        if (btnDeleteFile) {
            withGuard(btnDeleteFile, async () => {
                const ok = await showConfirm(`¿Estás seguro de eliminar '${currentSelectedPath}'?`, 'Eliminar Archivo');
                if (!ok) return;
                await apiDeleteFile(currentServerId, currentSelectedPath, scope);
                currentSelectedPath = ".";
                el(ids.selectedPath).innerText = "/ (Raíz)";
                el(ids.folderPanel).style.display = 'flex';
                el(ids.folderPanel).classList.remove('hidden');
                el(ids.filePanel).style.display = 'none';
                el(ids.filePanel).classList.add('hidden');
                setTimeout(() => listFiles(), 500);
            });
        }

        const btnUpload = el(ids.btnUpload);
        if (btnUpload) {
            withGuard(btnUpload, async () => {
                const fileInput = el(ids.uploadInput);
                if (!fileInput || fileInput.files.length === 0) return alert("Selecciona al menos un archivo");
                const targetFolder = currentSelectedPath;
                const totalFiles = fileInput.files.length;

                for (let i = 0; i < fileInput.files.length; i++) {
                    const file = fileInput.files[i];
                    const reader = new FileReader();
                    await new Promise((resolve) => {
                        reader.onload = async () => {
                            const base64 = reader.result.split(',')[1];
                            await apiUploadFile(currentServerId, file.name, base64, targetFolder, scope);
                            resolve();
                        };
                        reader.readAsDataURL(file);
                    });
                }
                fileInput.value = "";
                alert(`¡${totalFiles} archivo(s) subido(s) con éxito!`);
                listFiles();
            }, '⏳ Subiendo...');
        }

        const btnCreateDir = el(ids.btnCreateDir);
        if (btnCreateDir) {
            withGuard(btnCreateDir, async () => {
                const folderName = prompt(`Crear nueva carpeta dentro de '${currentSelectedPath === "." ? "/ (Raíz)" : currentSelectedPath}':\nEscribe el nombre:`);
                if (!folderName) return;
                if (folderName.includes("/") || folderName.includes("\\") || folderName.includes("..")) {
                    return alert("El nombre de la carpeta no puede contener barras o puntos suspensivos.");
                }
                const targetPath = currentSelectedPath === "." ? folderName : `${currentSelectedPath}/${folderName}`;
                await apiCreateDirectory(currentServerId, targetPath, scope);
                setTimeout(() => listFiles(), 500);
            });
        }

        const btnDeleteDir = el(ids.btnDeleteDir);
        if (btnDeleteDir) {
            withGuard(btnDeleteDir, async () => {
                if (currentSelectedPath === ".") return alert("No puedes eliminar la raíz.");
                const ok = await showConfirm(`¿Estás seguro de eliminar TODA la carpeta '${currentSelectedPath}'?`, 'Eliminar Carpeta');
                if (!ok) return;
                await apiDeleteFile(currentServerId, currentSelectedPath, scope);
                currentSelectedPath = ".";
                el(ids.selectedPath).innerText = "/ (Raíz)";
                setTimeout(() => listFiles(), 500);
            });
        }
    }

    return { init, setServerId, listFiles, render, renderFileContent };
}