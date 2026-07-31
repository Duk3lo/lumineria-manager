import { sendAction, confirmDelete } from '../features/actions.js';
import { openLogs } from '../features/logs.js';

const STATUS_LABELS = {
    running: 'En ejecución',
    stopped: 'Detenido',
    restarting: 'Reiniciando',
    missing: 'Contenedor eliminado',
    unknown: 'Desconocido',
};

export function updateStatus(text, color) {
    const status = document.getElementById('status-panel');
    status.innerText = text;
    status.style.backgroundColor = color;
}

export function renderServers(servers) {
    const ul = document.getElementById('server-list');
    ul.innerHTML = "";
    servers.forEach(server => {
        const li = document.createElement('li');
        const isMissing = server.status === 'missing';
        const statusLabel = STATUS_LABELS[server.status] || server.status;

        li.innerHTML = `
        <div class="server-header">
            <strong>${server.display_name}</strong>
            <span>Tipo: ${server.server_type.toUpperCase()} | MC: ${server.mc_version} | Status: ${statusLabel}</span>
        </div>
        <div class="actions">
            ${isMissing
                ? `<button onclick="window.sendAction('recreate_container', '${server.id}')" style="background-color: #cba6f7;">Recrear Contenedor</button>
                 <button onclick="window.confirmDelete('${server.id}')" style="background-color: #ed8796;">Eliminar</button>`
                : `
                <button onclick="window.sendAction('start_server', '${server.id}')" style="background-color: #a6e3a1;">Iniciar</button>
                <button onclick="window.sendAction('stop_server', '${server.id}')" style="background-color: #f38ba8;">Detener</button>
                <button onclick="window.sendAction('restart_server', '${server.id}')" style="background-color: #f9e2af;">Reiniciar</button>
                <button onclick="window.openLogs('${server.id}')" style="background-color: #89dceb;">Terminal (Logs)</button>
                ${(server.server_type === 'paper' || server.server_type === 'velocity') ?
                    `<button onclick="window.sendAction('auto_update', '${server.id}')" style="background-color: #89b4fa;">Actualizar</button>` : ''}
                <button onclick="window.confirmDelete('${server.id}')" style="background-color: #ed8796;">Eliminar</button>
            `}
        </div>
        `;
        ul.appendChild(li);
    });
}