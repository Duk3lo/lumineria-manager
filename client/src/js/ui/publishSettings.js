import { invoke } from '../core/tauri.js';
import { STATE } from '../core/state.js';
import { setPublishConfigRemote, getPublishConfigRemote } from '../features/actions.js';

let pendingFill = false;

// Se llama desde main.js cuando llega el evento "publish_config" del agente remoto
export function applyRemotePublishConfig(cfg) {
    if (!pendingFill) return;
    pendingFill = false;
    document.getElementById('ps-ssh-host').value = cfg.ssh_host || '';
    document.getElementById('ps-remote-base').value = cfg.remote_base || '~/lumineria';
    document.getElementById('ps-domain').value = cfg.domain || '';
}

export function initPublishSettings() {
    const btnOpen = document.getElementById('btn-publish-settings');
    const modal = document.getElementById('publish-settings-modal');

    if (btnOpen) {
        btnOpen.onclick = async () => {
            if (STATE.mode === 'remote') {
                pendingFill = true;
                try {
                    await getPublishConfigRemote();
                } catch (e) {
                    pendingFill = false;
                    alert("No pude leer la configuración del agente remoto: " + e);
                }
            } else {
                try {
                    const cfg = await invoke('load_publish_config');
                    document.getElementById('ps-ssh-host').value = cfg.ssh_host || '';
                    document.getElementById('ps-remote-base').value = cfg.remote_base || '~/lumineria';
                    document.getElementById('ps-domain').value = cfg.domain || 'http://localhost';
                } catch (e) { /* si falla, quedan los placeholders vacíos */ }
            }
            modal.classList.remove('hidden');
        };
    }

    document.getElementById('btn-ps-cancel').onclick = () => modal.classList.add('hidden');

    document.getElementById('btn-ps-save').onclick = async () => {
        const sshHostRaw = document.getElementById('ps-ssh-host').value.trim();
        const remoteBase = document.getElementById('ps-remote-base').value.trim() || '~/lumineria';
        const domainRaw = document.getElementById('ps-domain').value.trim();
        const domain = domainRaw.length > 0 ? domainRaw : 'http://localhost';
        const sshHost = sshHostRaw.length > 0 ? sshHostRaw : null;

        try {
            if (STATE.mode === 'remote') {
                await setPublishConfigRemote(sshHost, remoteBase, domain);
            } else {
                await invoke('save_publish_config', { sshHost, remoteBase, domain });
            }
            modal.classList.add('hidden');
        } catch (e) {
            alert("Error guardando configuración: " + e);
        }
    };
}