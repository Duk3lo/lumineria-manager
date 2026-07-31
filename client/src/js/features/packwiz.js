import { invoke } from '../core/tauri.js';

export async function syncPackwiz(serverId, packUrl) {
    const btn = document.getElementById('btn-sync-packwiz');
    const resultsDiv = document.getElementById('packwiz-results');

    btn.disabled = true;
    btn.innerText = "Sincronizando...";
    resultsDiv.innerHTML = `<p style="color: #f9e2af;">Descargando mods y verificando hashes. Esto puede tardar...</p>`;

    try {
        // Asumiendo que has creado un comando 'sync_packwiz' en Tauri
        // que envía la orden al Agente remoto/local.
        const results = await invoke('sync_packwiz_command', { id: serverId, url: packUrl });

        resultsDiv.innerHTML = `<ul style="list-style: none; padding: 0; margin: 0;">` +
            results.map(mod => `
                <li style="border-bottom: 1px solid #45475a; padding: 5px 0; display: flex; justify-content: space-between;">
                    <span style="color: #cdd6f4;">${mod.name}</span>
                    <span style="color: ${mod.status.includes('error') ? '#f38ba8' : '#a6e3a1'};">${mod.status}</span>
                </li>
            `).join('') +
            `</ul>`;

    } catch (error) {
        resultsDiv.innerHTML = `<p style="color: #f38ba8;">Error en la sincronización: ${error}</p>`;
    } finally {
        btn.disabled = false;
        btn.innerText = "Sincronizar Mods";
    }
}