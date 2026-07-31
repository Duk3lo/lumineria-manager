import { openServerDetail } from './serverDetail.js';

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

        card.innerHTML = `
            <div style="padding: 20px; background: #313244; border-radius: 10px; cursor: pointer; transition: 0.2s; border: 1px solid #45475a;">
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 15px;">
                    <h3 style="margin: 0; font-size: 1.2rem; color: #cdd6f4;">${server.display_name}</h3>
                    <span style="color: ${dotColor}; font-size: 1.2rem;">●</span>
                </div>
                <div style="display: flex; gap: 5px; flex-wrap: wrap;">
                    <span class="badge" style="background: #45475a; padding: 4px 8px; border-radius: 4px; font-size: 0.8rem;">${server.server_type.toUpperCase()}</span>
                    <span class="badge" style="background: #45475a; padding: 4px 8px; border-radius: 4px; font-size: 0.8rem;">MC: ${server.mc_version}</span>
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