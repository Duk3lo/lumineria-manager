use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    Running,
    Stopped,
    Restarting,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub id: String,
    pub display_name: String,
    pub server_type: String,
    pub port: u16,
    pub mc_version: String,
    pub mod_source: String,
    pub status: ServerStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfigParams {
    pub display_name: String,
    pub server_type: String,
    pub mc_version: String,
    pub loader_version: Option<String>,
    pub port: u16,
    pub min_ram: String,
    pub max_ram: String,
    pub online_mode: bool,
    pub enforce_secure_profile: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientRequest {
    ListServers,
    StartServer {
        id: String,
    },
    StopServer {
        id: String,
    },
    RestartServer {
        id: String,
    },
    SyncMods {
        id: String,
    },
    SubscribeLogs {
        id: String,
    },
    UnsubscribeLogs {
        id: String,
    },
    DeleteServer {
        id: String,
    },
    RecreateContainer {
        id: String,
    },
    StartStack,
    StopStack,
    RestartStack,

    CreateServer {
        id: String,
        config: ServerConfigParams,
    },
    AddModPackwiz {
        id: String,
        query: String,
    },
    RemoveModPackwiz {
        id: String,
        query: String,
    },
    UploadModPackwiz {
        id: String,
        filename: String,
        data_base64: String,
        folder: String,
    },
    PublishPackwiz {
        id: String,
        pack_key: String,
    },
    ListPackwizMods {
        id: String,
    },
    AutoUpdateServer {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    Servers {
        servers: Vec<ServerInfo>,
    },
    LogLine {
        id: String,
        line: String,
    },
    StatusChanged {
        id: String,
        status: ServerStatus,
    },
    Ack {
        ok: bool,
        message: Option<String>,
    },
    Error {
        message: String,
    },

    InstallProgress {
        id: String,
        step: String,
        percentage: u8,
    },
    PackwizLog {
        id: String,
        line: String,
    },
    PackwizModsList {
        id: String,
        mods: Vec<PackwizMod>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackwizMod {
    pub name: String,
    pub filename: String,
    pub side: String, 
}