import { openServerDetail } from './serverDetail.js';

// 👇 FIX SEGURIDAD: esto faltaba en este archivo. Otros módulos (fileExplorer.js,
// serverDetail.js) ya tenían su propio escapeHtml, pero acá el display_name del
// servidor se metía directo en innerHTML sin escapar → XSS persistente, porque
// SERVER_NAME se puede editar libremente desde "Archivos → Carpeta del Servidor".
function escapeHtml(str) {
    return String(str ?? '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

export function updateStatus(text, color) {
    const statusPanel = document.getElementById('status-panel');
    const statusDot = document.getElementById('connection-status-dot');

    if (statusPanel) statusPanel.innerText = text;
    if (statusDot && color) statusDot.style.color = color;
}

export function renderServers(servers) {
    const grid = document.getElementById('server-grid');
    grid.innerHTML = "";

    if (servers.length === 0) {
        grid.innerHTML = `<p style="color: #a6adc8; grid-column: 1/-1; text-align:center;">No tienes servidores. ¡Crea uno nuevo!</p>`;
        return;
    }

    servers.forEach(server => {
        const card = document.createElement('div');
        card.className = 'profile-card';

        const isRunning = server.status === 'running';
        const dotColor = isRunning ? '#a6e3a1' : '#f38ba8';

        // 👇 escapamos TODO lo que viene del server.env (display_name, server_type,
        // mc_version), porque cualquiera de esos campos puede haber sido editado a
        // mano desde el explorador de archivos.
        const safeName = escapeHtml(server.display_name);
        const safeType = escapeHtml(String(server.server_type || '').toUpperCase());
        const safeVersion = escapeHtml(server.mc_version);

        card.innerHTML = `
            <div style="padding: 20px; background: #313244; border-radius: 10px; cursor: pointer; transition: 0.2s; border: 1px solid #45475a;">
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 15px;">
                    <h3 style="margin: 0; font-size: 1.2rem; color: #cdd6f4;">${safeName}</h3>
                    <span style="color: ${dotColor}; font-size: 1.2rem;">●</span>
                </div>
                <div style="display: flex; gap: 5px; flex-wrap: wrap;">
                    <span class="badge" style="background: #45475a; padding: 4px 8px; border-radius: 4px; font-size: 0.8rem;">${safeType}</span>
                    <span class="badge" style="background: #45475a; padding: 4px 8px; border-radius: 4px; font-size: 0.8rem;">MC: ${safeVersion}</span>
                </div>
            </div>
        `;

        card.addEventListener('click', () => {
            openServerDetail(server);
        });

        card.onmouseover = () => card.firstElementChild.style.borderColor = '#cba6f7';
        card.onmouseout = () => card.firstElementChild.style.borderColor = '#45475a';

        grid.appendChild(card);
    });
}