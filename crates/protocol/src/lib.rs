//! Protocolo compartido entre el agente (VPS) y el cliente (Tauri).
//!
//! Todo lo que viaja por el WebSocket pasa por estos tipos. Si agregas un
//! campo o una variante acá, tanto `agent` como `client/src-tauri` van a
//! dejar de compilar hasta que los actualices en los dos lados — esa es
//! la ventaja de compartir el crate en vez de tener el JSON "a mano" en
//! cada punta.

use serde::{Deserialize, Serialize};

/// Estado observado de un servidor (contenedor podman).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    Running,
    Stopped,
    Restarting,
    /// No se pudo consultar podman, o el contenedor no existe todavía.
    Unknown,
}

/// Info de un servidor, derivada de su carpeta + `server.env`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Nombre de la carpeta (y por convención, del contenedor podman
    /// sanitizado). Es el identificador que se usa en todos los comandos.
    pub id: String,
    /// SERVER_NAME dentro de server.env, para mostrar en la UI.
    pub display_name: String,
    pub server_type: String,
    pub port: u16,
    pub mc_version: String,
    /// "packwiz" o "requirements" (MOD_SOURCE)
    pub mod_source: String,
    pub status: ServerStatus,
}

/// Mensajes que el cliente (Tauri) le manda al agente (VPS).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientRequest {
    /// Pide la lista completa de servidores + su estado actual.
    ListServers,
    StartServer { id: String },
    StopServer { id: String },
    RestartServer { id: String },
    /// Dispara una resincronización de mods (packwiz) sin esperar
    /// al próximo ciclo natural del runner.
    SyncMods { id: String },
    /// Empieza a recibir `LogLine` para ese servidor (streaming).
    SubscribeLogs { id: String },
    UnsubscribeLogs { id: String },
    /// Acciones sobre el stack completo (llaman a tus .sh existentes).
    StartStack,
    StopStack,
    RestartStack,
}

/// Mensajes que el agente le manda al cliente.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    Servers { servers: Vec<ServerInfo> },
    LogLine { id: String, line: String },
    StatusChanged { id: String, status: ServerStatus },
    Ack { ok: bool, message: Option<String> },
    Error { message: String },
}
