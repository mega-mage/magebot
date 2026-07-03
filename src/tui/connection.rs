use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, BufReader};
use super::event::TuiEvent;
use crate::ipc::IpcMessage;

pub async fn try_connect(tx_tui: tokio::sync::mpsc::UnboundedSender<TuiEvent>) -> Option<tokio::net::tcp::OwnedWriteHalf> {
    if let Ok(stream) = TcpStream::connect("127.0.0.1:42424").await {
        let (reader, writer) = stream.into_split();
        let tx = tx_tui.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(reader).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(msg) = serde_json::from_str::<IpcMessage>(&line) {
                    let _ = tx.send(TuiEvent::Ipc(msg));
                }
            }
            let _ = tx.send(TuiEvent::Disconnected);
        });
        Some(writer)
    } else {
        None
    }
}
