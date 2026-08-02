import { invoke } from '../core/tauri.js';
import { STATE } from '../core/state.js';
import { invoke_ws_action } from './actions.js';
import { updateStatus } from '../ui/serverList.js';

export function mcVersionToNeoforgePrefix(mcVersion) {
    const parts = mcVersion.split('.');
    if (parts[0] === '1') {
        if (parts.length >= 3) return `${parts[1]}.${parts[2]}`;
        return `${parts[1]}.0`;
    }
    if (parts.length >= 3) return `${parts[0]}.${parts[1]}.${parts[2]}`;
    return `${parts[0]}.${parts[1]}.0`;
}

export async function getLatestLoaderVersion(type, mcVersion) {
    if (type === 'fabric') {
        const res = await fetch(`https://meta.fabricmc.net/v2/versions/loader/${mcVersion}`);
        const loaders = await res.json();
        const stable = loaders.find(l => l.loader.stable) || loaders[0];
        return stable ? stable.loader.version : null;
    }
    if (type === 'neoforge') {
        if (STATE.cachedNeoForge.length === 0) STATE.cachedNeoForge = await invoke('fetch_neoforge_versions');
        const prefix = mcVersionToNeoforgePrefix(mcVersion);
        const matches = STATE.cachedNeoForge.filter(v => v.startsWith(`${prefix}.`));
        return matches.length ? matches[matches.length - 1] : null;
    }
    if (type === 'forge') {
        if (Object.keys(STATE.cachedForge).length === 0) STATE.cachedForge = await invoke('fetch_forge_versions');
        const matches = STATE.cachedForge[mcVersion] || [];
        return matches.length ? matches[matches.length - 1] : null;
    }
    return null;
}

async function ensureMojangVersions() {
    if (STATE.mojangVersionsCache.length === 0) {
        try {
            const res = await fetch("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json");
            const data = await res.json();
            STATE.mojangVersionsCache = data.versions
                .filter(v => v.type === "release")
                .map(v => v.id);
        } catch (e) {
            STATE.mojangVersionsCache = ['1.21.1', '1.20.1', '1.19.2', '1.18.2', '1.16.5'];
        }
    }
}

async function ensureLoaderCacheForType(type) {
    if (type === 'forge' && Object.keys(STATE.cachedForge).length === 0) {
        try { STATE.cachedForge = await invoke('fetch_forge_versions'); } catch (e) { }
    }
    if (type === 'neoforge' && STATE.cachedNeoForge.length === 0) {
        try { STATE.cachedNeoForge = await invoke('fetch_neoforge_versions'); } catch (e) { }
    }
}

function isMcVersionSupported(type, mcVersion) {
    if (type === 'paper' || type === 'velocity' || type === 'fabric') return true;
    if (type === 'forge') return !!(STATE.cachedForge[mcVersion]?.length);
    if (type === 'neoforge') {
        if (STATE.cachedNeoForge.length === 0) return true;
        const prefix = mcVersionToNeoforgePrefix(mcVersion);
        return STATE.cachedNeoForge.some(v => v.startsWith(`${prefix}.`));
    }
    return true;
}

export async function updateVersions() {
    const type = document.getElementById('new-type').value;
    const versionSelect = document.getElementById('new-version');
    const versionLbl = document.getElementById('version-lbl');
    if (!versionSelect || !versionLbl) return;

    const PAPER_PROJECTS = ['paper', 'velocity', 'folia'];

    if (PAPER_PROJECTS.includes(type)) {
        versionLbl.innerText = type === 'velocity' ? 'Versión de Velocity' : 'Versión de Minecraft';
        versionSelect.innerHTML = "<option>Cargando versiones...</option>";
        try {
            if (!STATE.cachedProjectVersions[type]) {
                STATE.cachedProjectVersions[type] = await invoke('fetch_paper_project_versions', { project: type });
            }
            const versions = STATE.cachedProjectVersions[type].slice().reverse();
            versionSelect.innerHTML = versions.map(v => `<option value="${v}">${v}</option>`).join('');
        } catch (e) {
            versionSelect.innerHTML = `<option value="">Error: ${e}</option>`;
        }
    } else {
        versionLbl.innerText = 'Versión de Minecraft';
        versionSelect.innerHTML = "<option>Cargando lista inteligente...</option>";
        await ensureMojangVersions();
        await ensureLoaderCacheForType(type);
        const filteredVersions = STATE.mojangVersionsCache.filter(v => isMcVersionSupported(type, v));
        versionSelect.innerHTML = filteredVersions.map(v => `<option value="${v}">${v}</option>`).join('');
    }
}

export async function updateLoaders() {
    const type = document.getElementById('new-type').value;
    const version = document.getElementById('new-version').value;
    const loaderSelect = document.getElementById('new-loader');
    const loaderLbl = document.getElementById('loader-lbl');

    if (type === 'paper' || type === 'velocity' || type === 'folia') {
        loaderSelect.style.display = 'none';
        loaderLbl.style.display = 'none';
        return;
    }
    loaderSelect.style.display = 'block';
    loaderLbl.style.display = 'block';
    loaderSelect.innerHTML = "<option>Cargando loaders...</option>";

    if (type === 'fabric') {
        try {
            const res = await fetch(`https://meta.fabricmc.net/v2/versions/loader/${version}`);
            const loaders = await res.json();
            loaderSelect.innerHTML = loaders.map(l => `<option value="${l.loader.version}">${l.loader.version}</option>`).join('');
        } catch (e) {
            loaderSelect.innerHTML = "<option value='0.15.11'>0.15.11 (Default)</option>";
        }
    } else if (type === 'neoforge') {
        if (STATE.cachedNeoForge.length === 0) STATE.cachedNeoForge = await invoke('fetch_neoforge_versions');
        const prefix = mcVersionToNeoforgePrefix(version);
        const matches = STATE.cachedNeoForge.filter(v => v.startsWith(`${prefix}.`)).reverse();
        loaderSelect.innerHTML = matches.map(v => `<option value="${v}">${v}</option>`).join('');
    } else if (type === 'forge') {
        if (Object.keys(STATE.cachedForge).length === 0) STATE.cachedForge = await invoke('fetch_forge_versions');
        const matches = (STATE.cachedForge[version] || []).slice().reverse();
        loaderSelect.innerHTML = matches.map(v => {
            const label = v.startsWith(`${version}-`) ? v.slice(version.length + 1) : v;
            return `<option value="${v}">${label}</option>`;
        }).join('');
    }
}

export async function submitCreateServer() {
    const name = document.getElementById('new-name').value.trim();
    const type = document.getElementById('new-type').value;
    const version = document.getElementById('new-version').value;
    const loader = document.getElementById('new-loader').value;
    const port = parseInt(document.getElementById('new-port').value);
    const minRam = document.getElementById('new-min-ram').value;
    const maxRam = document.getElementById('new-max-ram').value;
    const onlineMode = document.getElementById('new-online-mode').checked;

    if (!name) return alert("Especifica un nombre");

    const server_id = name.toLowerCase().replace(/[^a-z0-9]/g, '-');
    const config = {
        display_name: name,
        server_type: type,
        mc_version: version,
        loader_version: (type !== 'paper' && type !== 'velocity' && type !== 'folia') ? loader : null,
        port: port,
        min_ram: minRam,
        max_ram: maxRam,
        online_mode: onlineMode,
        enforce_secure_profile: onlineMode,
    };

    document.getElementById('creator-modal').classList.add('hidden');
    updateStatus("Instalando servidor en segundo plano...", "#fab387");
    invoke_ws_action({ type: "create_server", id: server_id, config: config });
}

export function initCreator() {
    document.getElementById('btn-new-server').onclick = async () => {
        document.getElementById('creator-modal').classList.remove('hidden');
        await updateVersions();
        await updateLoaders();
    };


    document.getElementById('btn-cancel-create').onclick = () => {
        document.getElementById('creator-modal').classList.add('hidden');
    };

    document.getElementById('new-type').onchange = async () => {
        await updateVersions();
        await updateLoaders();
    };

    document.getElementById('new-version').onchange = updateLoaders;
    document.getElementById('btn-submit-create').onclick = submitCreateServer;
}