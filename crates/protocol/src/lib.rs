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
pub struct FileNode {
    pub name: String,
    pub is_dir: bool,
    pub path: String,
    pub children: Option<Vec<FileNode>>,
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
pub struct PackwizImage {
    pub filename: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileScope {
    Packwiz,
    ServerRoot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientRequest {
    ListServers,
    StartServer {
        id: String,
    },
    ListPackwizFiles {
        id: String,
        scope: FileScope,
    },
    UpdateServer {
        id: String,
        loader_version: Option<String>,
        update_mods: bool,
        update_engine: bool,
        #[serde(default)]
        force: bool,
    },
    CreateDirectory {
        id: String,
        path: String,
        scope: FileScope,
    },
    ReadFile {
        id: String,
        path: String,
        scope: FileScope,
    },
    WriteFile {
        id: String,
        path: String,
        content: String,
        scope: FileScope,
    },
    DeleteFile {
        id: String,
        path: String,
        scope: FileScope,
    },
    SendConsoleCommand {
        id: String,
        command: String,
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
        scope: FileScope,
    },
    PublishPackwiz {
        id: String,
        pack_key: String,
        image: Option<PackwizImage>,
    },
    UnpublishPackwiz {
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
    ConsoleResponse {
        id: String,
        response: String,
    },
    PackwizFilesList {
        id: String,
        scope: FileScope,
        files: Vec<FileNode>,
    },
    FileContent {
        id: String,
        path: String,
        scope: FileScope,
        content: Option<String>,
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
