use super::agent_client::AgentConnection;
use tokio::sync::Mutex;

pub struct AppState {
    pub connection: Mutex<Option<AgentConnection>>,
}