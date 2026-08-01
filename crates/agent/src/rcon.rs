use anyhow::{bail, Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

const TYPE_AUTH: i32 = 3;
const TYPE_EXEC_COMMAND: i32 = 2;

pub struct RconClient {
    stream: TcpStream,
    next_id: i32,
}

impl RconClient {
    pub async fn connect(host: &str, port: u16, password: &str) -> Result<Self> {
        let stream = timeout(Duration::from_secs(3), TcpStream::connect((host, port)))
            .await
            .context("timeout conectando al RCON")?
            .context("no pude conectar al puerto RCON (¿está el servidor arrancado?)")?;

        let mut client = Self { stream, next_id: 1 };
        client.authenticate(password).await?;
        Ok(client)
    }

    async fn authenticate(&mut self, password: &str) -> Result<()> {
        self.send_packet(TYPE_AUTH, password).await?;
        let (resp_id, _ptype, _body) = self.read_packet().await?;
        if resp_id == -1 {
            bail!("autenticación RCON rechazada (¿contraseña incorrecta?)");
        }
        Ok(())
    }

    pub async fn command(&mut self, cmd: &str) -> Result<String> {
        self.send_packet(TYPE_EXEC_COMMAND, cmd).await?;
        let (_id, _ptype, body) = self.read_packet().await?;
        Ok(body)
    }

    async fn send_packet(&mut self, packet_type: i32, body: &str) -> Result<i32> {
        let id = self.next_id;
        self.next_id += 1;

        let mut payload = Vec::new();
        payload.extend_from_slice(&id.to_le_bytes());
        payload.extend_from_slice(&packet_type.to_le_bytes());
        payload.extend_from_slice(body.as_bytes());
        payload.push(0); // fin del body
        payload.push(0); // fin del paquete

        let len = payload.len() as i32;
        self.stream.write_all(&len.to_le_bytes()).await?;
        self.stream.write_all(&payload).await?;
        self.stream.flush().await?;
        Ok(id)
    }

    async fn read_packet(&mut self) -> Result<(i32, i32, String)> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = i32::from_le_bytes(len_buf) as usize;
        if !(10..=1_048_576).contains(&len) {
            bail!("paquete RCON con tamaño inválido: {len}");
        }

        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;

        let id = i32::from_le_bytes(buf[0..4].try_into().unwrap());
        let ptype = i32::from_le_bytes(buf[4..8].try_into().unwrap());
        let body = String::from_utf8_lossy(&buf[8..buf.len().saturating_sub(2)]).to_string();
        Ok((id, ptype, body))
    }
}