import { sendAction, confirmDelete, openServerFolder } from '../features/actions.js';
import { openLogs, closeLogs } from '../features/logs.js';
import { syncPackwiz } from '../features/packwiz.js';

export let currentServerId = null;

const viewGrid = document.getElementById('view-grid');
const viewDetail = document.getElementById('view-server-detail');

// Elementos del DOM
const titleEl = document.getElementById('detail-title');
const badgeEl = document.getElementById('detail-badge');
const statusTextEl = document.getElementById('detail-status-text');

export function initServerDetail() {
    // Botón Volver
    document.getElementById('btn-back-grid').addEventListener('click', async () => {
        await closeLogs();
        currentServerId = null;
        viewDetail.classList.add('hidden');
        viewGrid.classList.remove('hidden');
    });

    // Pestañas
    const tabBtns = document.querySelectorAll('.tab-btn[data-tab]');
    const tabPanes = document.querySelectorAll('.tab-pane');

    tabBtns.forEach(btn => {
        btn.addEventListener('click', () => {
            tabBtns.forEach(b => b.classList.remove('active'));
            tabPanes.forEach(p => p.classList.add('hidden'));

            btn.classList.add('active');
            document.getElementById(btn.dataset.tab).classList.remove('hidden');
        });
    });

    // Botones de acción del servidor
    document.getElementById('btn-detail-start').onclick = () => sendAction('start_server', currentServerId);
    document.getElementById('btn-detail-stop').onclick = () => sendAction('stop_server', currentServerId);
    document.getElementById('btn-detail-restart').onclick = () => sendAction('restart_server', currentServerId);
    document.getElementById('btn-detail-delete').onclick = async () => {
        const deleted = await confirmDelete(currentServerId);
        if (deleted) {
            await closeLogs();
            currentServerId = null;
            viewDetail.classList.add('hidden');
            viewGrid.classList.remove('hidden');
        }
    };

    document.getElementById('btn-open-folder').onclick = () => openServerFolder(currentServerId);



    document.getElementById('btn-sync-packwiz').onclick = () => {
        const url = document.getElementById('packwiz-url-input').value.trim();
        if (url) syncPackwiz(currentServerId, url);
        else alert("Por favor ingresa una URL de packwiz.");
    };
}

export async function openServerDetail(server) {
    currentServerId = server.id;

    // Llenar datos visuales
    titleEl.innerText = server.display_name;
    badgeEl.innerText = `${server.server_type} ${server.mc_version}`;
    updateDetailStatus(server.status);

    // Cambiar vista
    viewGrid.classList.add('hidden');
    document.getElementById('view-connection').classList.add('hidden');
    viewDetail.classList.remove('hidden');

    // Volver a la pestaña de Terminal por defecto y cargar logs
    document.querySelector('.tab-btn[data-tab="tab-console"]').click();
    await openLogs(server.id); // Reutilizamos tu sistema de logs batcheado!
}

export function updateDetailStatus(status) {
    const labels = {
        running: '🟢 En ejecución',
        stopped: '🔴 Detenido',
        restarting: '🟡 Reiniciando',
        missing: '⚠ Falta contenedor'
    };
    statusTextEl.innerText = labels[status] || status;
}