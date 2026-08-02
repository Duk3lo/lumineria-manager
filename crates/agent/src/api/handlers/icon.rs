use super::super::state::AppState;
use protocol::ServerEvent;
use tokio::sync::mpsc;

pub(crate) async fn upload_server_icon(
    state: &AppState,
    tx: &mpsc::UnboundedSender<ServerEvent>,
    id: String,
    data_base64: String,
) {
    let tx_clone = tx.clone();
    let root_clone = state.root.clone();
    tokio::spawn(async move {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let bytes = match STANDARD.decode(&data_base64) {
            Ok(b) => b,
            Err(_) => {
                let _ = tx_clone.send(ServerEvent::Error { message: "Imagen inválida (base64 corrupto).".into() });
                return;
            }
        };

        let img = match image::load_from_memory(&bytes) {
            Ok(i) => i,
            Err(e) => {
                let _ = tx_clone.send(ServerEvent::Error { message: format!("No pude leer la imagen: {e}") });
                return;
            }
        };

        // Recorte central a cuadrado + resize a 64x64 (formato exacto que espera Minecraft)
        let (w, h) = (img.width(), img.height());
        let side = w.min(h);
        let x = (w - side) / 2;
        let y = (h - side) / 2;
        let square = img.crop_imm(x, y, side, side);
        let resized = square.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);

        let dest_dir = root_clone.join(&id);
        let icon_path = dest_dir.join("server-icon.png");

        if let Err(e) = resized.save_with_format(&icon_path, image::ImageFormat::Png) {
            let _ = tx_clone.send(ServerEvent::Error { message: format!("No pude guardar server-icon.png: {e}") });
            return;
        }

        let _ = tx_clone.send(ServerEvent::Ack {
            ok: true,
            message: Some("Ícono actualizado (recortado y reescalado a 64x64). Se ve tras reiniciar el servidor.".into()),
        });
    });
}