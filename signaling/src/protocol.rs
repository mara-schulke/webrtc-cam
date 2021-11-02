use anyhow::{bail, Context};
use async_std::net::TcpStream;
use async_tungstenite::WebSocketStream;
use serde::{Deserialize, Serialize};
use futures::StreamExt;
use uuid::Uuid;
use crate::rooms::RoomMap;

#[derive(Serialize, Deserialize, Debug)]
pub enum PeerMessage {
    Signup,
    Connect(Uuid),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerMessage {
    Ice(String),
    Sdp(String),
    Registered(Uuid)
}

pub async fn handler(
    mut websocket: WebSocketStream<TcpStream>,
    rooms: RoomMap,
) -> anyhow::Result<()> {
    use async_tungstenite::tungstenite::Message;

    log::info!("websocket connection established");

    loop {
        let websocket_msg = websocket
            .next()
            .await
            .context("websocket stream ended unexpectedly")?;

        let message: PeerMessage = loop {
            match websocket_msg {
                Ok(Message::Text(_)) => log::error!("unexpected message type"),
                Ok(Message::Ping(_) | Message::Pong(_)) => continue,
                Ok(Message::Binary(blob)) => break serde_json::from_slice(blob.as_slice())?,
                Ok(Message::Close(_)) => bail!("websocket: received close"),
                Err(e) => bail!("websocket: failed to read message ({:?})", e)
            }
        };

        match message {
            PeerMessage::Signup => {
                let mut map = rooms.lock().await;
                map.insert(Uuid::new_v4(), "this is a test".to_string());
            },
            PeerMessage::Connect(uuid) => {
                log::info!("Connect this peer with the peer {:?}", uuid);
            }
        }

        log::info!("session state: {:#?}", rooms);
    }
}
