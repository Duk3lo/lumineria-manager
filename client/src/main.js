const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let selectedFolder = null;
let mojangVersionsCache = [];
let cachedNeoForge = [];
let cachedForge = {};
let cachedProjectVersions = {};

function mcVersionToNeoforgePrefix(mcVersion) {
  const parts = mcVersion.split('.');
  if (parts[0] === '1') {
    if (parts.length >= 3) return `${parts[1]}.${parts[2]}`;
    return `${parts[1]}.0`;
  }
  if (parts.length >= 3) return `${parts[0]}.${parts[1]}.${parts[2]}`;
  return `${parts[0]}.${parts[1]}.0`;
}

async function ensureMojangVersions() {
  if (mojangVersionsCache.length === 0) {
    try {
      const res = await fetch("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json");
      const data = await res.json();
      mojangVersionsCache = data.versions
        .filter(v => v.type === "release")
        .map(v => v.id);
    } catch (e) {
      console.error("Error cargando el manifiesto de Mojang, usando fallback:", e);
      mojangVersionsCache = ['1.21.1', '1.20.1', '1.19.2', '1.18.2', '1.16.5'];
    }
  }
}

async function ensureLoaderCacheForType(type) {
  if (type === 'forge' && Object.keys(cachedForge).length === 0) {
    try {
      cachedForge = await invoke('fetch_forge_versions');
    } catch (e) {
      console.error("Fallo al obtener Forge de Rust:", e);
    }
  }
  if (type === 'neoforge' && cachedNeoForge.length === 0) {
    try {
      cachedNeoForge = await invoke('fetch_neoforge_versions');
    } catch (e) {
      console.error("Fallo al obtener NeoForge de Rust:", e);
    }
  }
}

function isMcVersionSupported(type, mcVersion) {
  if (type === 'paper' || type === 'velocity' || type === 'fabric') return true;

  if (type === 'forge') {
    return !!(cachedForge[mcVersion]?.length);
  }
  if (type === 'neoforge') {
    if (cachedNeoForge.length === 0) return true;
    const prefix = mcVersionToNeoforgePrefix(mcVersion);
    return cachedNeoForge.some(v => v.startsWith(`${prefix}.`));
  }
  return true;
}
async function updateVersions() {
  const type = document.getElementById('new-type').value;
  const versionSelect = document.getElementById('new-version');
  const versionLbl = document.getElementById('version-lbl');

  if (!versionSelect || !versionLbl) return;

  const PAPER_PROJECTS = ['paper', 'velocity', 'folia'];

  if (PAPER_PROJECTS.includes(type)) {
    versionLbl.innerText = type === 'velocity' ? 'Versión de Velocity' : 'Versión de Minecraft';
    versionSelect.innerHTML = "<option>Cargando versiones...</option>";
    try {
      if (!cachedProjectVersions[type]) {
        cachedProjectVersions[type] = await invoke('fetch_paper_project_versions', { project: type });
      }
      const versions = cachedProjectVersions[type].slice().reverse();
      versionSelect.innerHTML = versions.map(v => `<option value="${v}">${v}</option>`).join('');
    } catch (e) {
      versionSelect.innerHTML = `<option value="">Error: ${e}</option>`;
    }
  } else {
    versionLbl.innerText = 'Versión de Minecraft';
    versionSelect.innerHTML = "<option>Cargando lista inteligente...</option>";

    await ensureMojangVersions();
    await ensureLoaderCacheForType(type);

    const filteredVersions = mojangVersionsCache.filter(v => isMcVersionSupported(type, v));

    versionSelect.innerHTML = filteredVersions.map(v => `<option value="${v}">${v}</option>`).join('');
  }
}

document.addEventListener("DOMContentLoaded", () => {
  document.getElementById('tab-local').onclick = () => switchTab('local');
  document.getElementById('tab-remote').onclick = () => switchTab('remote');

  document.getElementById('tab-new').onclick = openCreatorModal;
  document.getElementById('btn-cancel-create').onclick = () => document.getElementById('creator-modal').style.display = 'none';
  document.getElementById('new-type').onchange = async () => {
    await updateVersions();
    await updateLoaders();
  };
  document.getElementById('new-version').onchange = updateLoaders;
  document.getElementById('btn-submit-create').onclick = submitCreateServer;

  document.getElementById('btn-pick-folder').onclick = async () => {
    selectedFolder = await invoke('pick_folder');
    if (selectedFolder) {
      document.getElementById('folder-path').innerText = selectedFolder;
      document.getElementById('btn-start-local').disabled = false;
    }
  };

  document.getElementById('btn-start-local').onclick = async () => {
    updateStatus("Iniciando agente local...", "#f9e2af");
    try {
      const url = await invoke('start_local_agent', { rootPath: selectedFolder });
      await connectAgent(url);
    } catch (e) {
      updateStatus("Error: " + e, "#f38ba8");
    }
  };

  document.getElementById('btn-connect-remote').onclick = async () => {
    const url = document.getElementById('input-url').value;
    await connectAgent(url);
  };

  listen("server-event", (event) => {
    const data = event.payload;
    if (data.type === "servers") {
      renderServers(data.servers);
    } else if (data.type === "install_progress") {
      document.getElementById('install-progress-lbl').innerText = `[${data.percentage}%] ${data.step}`;
    } else if (data.type === "ack") {
      document.getElementById('install-progress-lbl').innerText = "";
      alert("Operación completada: " + (data.message || "OK"));
      invoke_ws_action({ type: "list_servers" });
    } else if (data.type === "error") {
      document.getElementById('install-progress-lbl').innerText = "";
      alert("Error: " + data.message);
    }
  });
});

async function openCreatorModal() {
  document.getElementById('creator-modal').style.display = 'flex';
  await updateVersions();
  await updateLoaders();
}

async function updateLoaders() {
  const type = document.getElementById('new-type').value;
  const version = document.getElementById('new-version').value;
  const loaderSelect = document.getElementById('new-loader');
  const loaderLbl = document.getElementById('loader-lbl');

  if (type === 'paper' || type === 'velocity') {
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
  }

  else if (type === 'neoforge') {
    if (cachedNeoForge.length === 0) {
      cachedNeoForge = await invoke('fetch_neoforge_versions');
    }
    const prefix = mcVersionToNeoforgePrefix(version);
    const matches = cachedNeoForge.filter(v => v.startsWith(`${prefix}.`)).reverse();
    loaderSelect.innerHTML = matches.map(v => `<option value="${v}">${v}</option>`).join('');
  } else if (type === 'forge') {
    if (Object.keys(cachedForge).length === 0) {
      cachedForge = await invoke('fetch_forge_versions');
    }
    const matches = (cachedForge[version] || []).slice().reverse();
    loaderSelect.innerHTML = matches.map(v => {
      const label = v.startsWith(`${version}-`) ? v.slice(version.length + 1) : v;
      return `<option value="${v}">${label}</option>`;
    }).join('');
  }
}

async function submitCreateServer() {
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
    loader_version: (type !== 'paper' && type !== 'velocity') ? loader : null,
    port: port,
    min_ram: minRam,
    max_ram: maxRam,
    online_mode: onlineMode,
    enforce_secure_profile: !onlineMode,
  };

  document.getElementById('creator-modal').style.display = 'none';
  updateStatus("Instalando servidor en segundo plano...", "#fab387");

  invoke_ws_action({
    type: "create_server",
    id: server_id,
    config: config
  });
}

async function connectAgent(url) {
  const maxAttempts = 5;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    updateStatus(`Conectando (Intento ${attempt}/${maxAttempts})...`, "#f9e2af");
    try {
      // Intentamos conectar
      await invoke('connect_agent', { url });

      // Si tiene éxito, actualizamos estado y pedimos servidores
      updateStatus("Conectado", "#a6e3a1");
      invoke_ws_action({ type: "list_servers" });
      return; // <-- Salimos de la función porque ya conectó con éxito
    } catch (e) {
      if (attempt === maxAttempts) {
        // Si fue el último intento y falló, reportamos el error definitivo
        updateStatus("Fallo de conexión definitivo: " + e, "#f38ba8");
      } else {
        // Si falló pero quedan intentos, esperamos 800 milisegundos antes de reintentar
        await new Promise(resolve => setTimeout(resolve, 800));
      }
    }
  }
}

async function invoke_ws_action(payload) {
  if (payload.type === "list_servers") await invoke('list_servers');
  if (payload.type === "create_server") {
    await invoke('create_server', { id: payload.id, config: payload.config });
  }
}

function updateStatus(text, color) {
  const status = document.getElementById('status-panel');
  status.innerText = text;
  status.style.backgroundColor = color;
}

function renderServers(servers) {
  const ul = document.getElementById('server-list');
  ul.innerHTML = "";
  servers.forEach(server => {
    const li = document.createElement('li');

    li.innerHTML = `
      <div class="server-header">
        <strong>${server.display_name}</strong>
        <span>Tipo: ${server.server_type.toUpperCase()} | MC: ${server.mc_version} | Status: ${server.status}</span>
      </div>
      <div class="actions">
        <button onclick="sendAction('start_server', '${server.id}')" style="background-color: #a6e3a1;">Iniciar</button>
        <button onclick="sendAction('stop_server', '${server.id}')" style="background-color: #f38ba8;">Detener</button>
        <button onclick="sendAction('restart_server', '${server.id}')" style="background-color: #f9e2af;">Reiniciar</button>
        ${(server.server_type === 'paper' || server.server_type === 'velocity') ?
        `<button onclick="sendAction('auto_update', '${server.id}')" style="background-color: #89b4fa;">Auto-Update Build</button>` : ''}
      </div>
    `;
    ul.appendChild(li);
  });
}

window.sendAction = async function (type, id) {
  updateStatus("Enviando comando " + type + "...", "#f9e2af");
  try {
    if (type === "start_server") await invoke('start_server', { id });
    if (type === "stop_server") await invoke('stop_server', { id });
    if (type === "restart_server") await invoke('restart_server', { id });
    if (type === "auto_update") {
      updateStatus("Actualizando compilación del motor...", "#fab387");
      await invoke('auto_update_server', { id });
    }
  } catch (e) {
    alert("Error: " + e);
  }
};

function switchTab(tab) {
  document.getElementById('view-local').style.display = tab === 'local' ? 'block' : 'none';
  document.getElementById('view-remote').style.display = tab === 'remote' ? 'block' : 'none';
  document.getElementById('tab-local').classList.toggle('active', tab === 'local');
  document.getElementById('tab-remote').classList.toggle('active', tab === 'remote');
}