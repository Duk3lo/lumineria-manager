import { invoke } from '../core/tauri.js';

export function initPublishSettings() {
    const btnOpen = document.getElementById('btn-publish-settings');
    const modal = document.getElementById('publish-settings-modal');

    if (btnOpen) {
        btnOpen.onclick = async () => {
            try {
                const cfg = await invoke('load_publish_config');
                document.getElementById('ps-ssh-host').value = cfg.ssh_host || '';
                document.getElementById('ps-remote-base').value = cfg.remote_base || '~/lumineria';
                document.getElementById('ps-domain').value = cfg.domain || 'localhost'; // 👈 NUEVO
            } catch (e) { /* si falla, quedan los placeholders vacíos */ }
            modal.classList.remove('hidden');
        };
    }

    document.getElementById('btn-ps-cancel').onclick = () => modal.classList.add('hidden');

    document.getElementById('btn-ps-save').onclick = async () => {
        const sshHostRaw = document.getElementById('ps-ssh-host').value.trim();
        const remoteBase = document.getElementById('ps-remote-base').value.trim() || '~/lumineria';
        const domainRaw = document.getElementById('ps-domain').value.trim(); // 👈 NUEVO
        
        const domain = domainRaw.length > 0 ? domainRaw : 'localhost'; // 👈 NUEVO
        const sshHost = sshHostRaw.length > 0 ? sshHostRaw : null;
        try {
            await invoke('save_publish_config', { sshHost, remoteBase, domain }); // 👈 NUEVO
            modal.classList.add('hidden');
        } catch (e) {
            alert("Error guardando configuración: " + e);
        }
    };
}