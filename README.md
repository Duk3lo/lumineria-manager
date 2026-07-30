# Lumineria Manager

Workspace de Cargo con 3 crates:

- `crates/protocol` — tipos de mensajes compartidos (WebSocket, JSON vía serde).
  Sin lógica, solo structs/enums. Lo importan `agent` y `client/src-tauri`.
- `crates/agent` — binario que corre en el VPS (Oracle Cloud). Sin Tauri,
  sin GUI. Expone un WebSocket y por debajo llama a `podman` y a tus
  scripts ya existentes (`start-podman.sh`, `stop-podman.sh`,
  `restart-podman.sh`). **No reimplementa nada de eso.**
- `client/src-tauri` — tu app de escritorio (mismo patrón que el launcher
  que ya hiciste). Se conecta al agente por WebSocket a través de un
  túnel SSH.

Los tres compilaron y se probaron en este entorno excepto `client/src-tauri`,
que necesita el webview del sistema (webkit2gtk/WebView2/WKWebView) — eso
se compila en tu PC con `cargo tauri dev`, no en el VPS.

## 1. Compilar y probar el agente

```bash
cd crates/agent
cargo build --release
```

Comandos disponibles:

```bash
# Revisa que estén podman/jq/curl/python3/unzip.
./target/release/lumineria-agent check-deps

# Los instala (con confirmación) usando apt/dnf del sistema — nunca curl|bash.
./target/release/lumineria-agent check-deps --install

# Levanta el WebSocket. --root es la carpeta que contiene
# start-podman.sh, stop-podman.sh, restart-podman.sh y las carpetas
# de cada servidor (server.env adentro de cada una).
./target/release/lumineria-agent serve --root /home/tu-usuario/minecraft-network --bind 127.0.0.1:8080
```

`--bind` queda deliberadamente en loopback (`127.0.0.1`). El agente
**no debe** exponerse directo a internet — se llega a él solo vía túnel SSH.

### Corrarlo como servicio (systemd)

```ini
# /etc/systemd/system/lumineria-agent.service
[Unit]
Description=Lumineria Manager Agent
After=network.target

[Service]
ExecStart=/ruta/a/lumineria-agent serve --root /home/tu-usuario/minecraft-network
Restart=on-failure
User=tu-usuario

[Install]
WantedBy=multi-user.target
```

## 2. Conectarte desde tu PC

Túnel LOCAL (lo iniciás vos, desde tu PC — no confundir con el `-R`
que usás para el envío de mods, que es reverso y para otro propósito):

```bash
ssh -L 8080:127.0.0.1:8080 tu-usuario@tu-vps
```

Con el túnel abierto, `ws://127.0.0.1:8080/ws` en tu máquina llega al
agente del VPS.

## 3. El cliente Tauri

```bash
cd client
cargo tauri dev
```

(vas a necesitar `cargo install tauri-cli` si no lo tenés, y las
dependencias de sistema normales de Tauri para tu SO).

`client/dist/index.html` es un frontend de referencia en vanilla JS,
solo para probar el flujo de punta a punta (conectar → listar
servidores → ver logs). Reemplazalo por el framework que ya usás en tu
launcher — la parte que importa es `src-tauri/src/main.rs`, que expone
estos comandos invocables desde JS:

- `connect_agent(url)`
- `list_servers()`
- `start_server(id)` / `stop_server(id)` / `restart_server(id)`
- `sync_mods(id)` — fuerza una resincronización de packwiz (reinicia el contenedor)
- `subscribe_logs(id)` / `unsubscribe_logs(id)`

Y un evento de Tauri, `server-event`, con el payload tipado como
`ServerEvent` (`servers`, `log_line`, `status_changed`, `ack`, `error`).

## Convención de nombres

El `id` de cada servidor es el nombre de su carpeta, sanitizado con la
misma regla que ya usa `sanitize_name()` en `lib_podman.sh` (para que
coincida con el nombre real del contenedor que genera
`generate_podman_compose`). No hace falta ningún registro central: la
carpeta con `server.env` adentro ES la fuente de verdad, la agrega o
la quita el agente automáticamente en cada `ListServers`.

## Qué falta (a propósito, para después)

- Autenticación en el WebSocket (aunque vaya por túnel SSH, conviene un
  token compartido en el primer mensaje).
- El programa de drag-and-drop en Rust+Tauri que resuelve mods contra
  packwiz/Modrinth y, si no los encuentra, los manda por el túnel SSH.
  No toca nada de este workspace — es un tercer proyecto que puede
  reusar el crate `protocol` si en algún momento necesita hablar con
  el agente también.
