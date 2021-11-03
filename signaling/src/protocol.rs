use anyhow::{bail, Context};
use async_tungstenite::tungstenite::Message;
use async_std::net::TcpStream;
use async_rustls::server::TlsStream;
use async_tungstenite::WebSocketStream;
use serde::{Deserialize, Serialize};
use futures::{SinkExt, StreamExt};
use uuid::Uuid;
use crate::rooms::Room;
use crate::ServerState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PeerMessage {
    Create(()), // Create a room with an offer
    Join(u8), // Join a room
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerMessage {
    Hello(Uuid),
    Created(u8),

    Joined(()), // joined and got an offer
    Failed
}

impl From<ServerMessage> for Message {
    fn from(server_msg: ServerMessage) -> Message {
        Message::Binary(serde_json::to_vec(&server_msg).unwrap())
    }
}

pub async fn handler(
    mut websocket: WebSocketStream<TlsStream<TcpStream>>,
    state: ServerState
) -> anyhow::Result<()> {
    log::info!("websocket connection established");

    let uuid = Uuid::new_v4();

    let mut lock = state.lock().await;
    lock.1.insert(uuid);
    drop(lock);

    websocket.send(ServerMessage::Hello(uuid).into()).await?;

    log::info!("registered peer {:?}", uuid);

    'msg: loop {
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
            PeerMessage::Join(room_id) => {
                let mut lock = state.lock().await;
                let room = lock.0.get_mut(&room_id);
                match room {
                    Some(room) => {
                        room.join(uuid);
                        log::info!("(peer {}) joined {}", uuid, room_id);
                        websocket.send(ServerMessage::Joined.into()).await?;
                    }
                    None => {
                        log::error!("(peer {}) tried to join {} but it doesnt exist", uuid, room_id);
                        websocket.send(ServerMessage::Failed.into()).await?;
                    }
                }
            }
            PeerMessage::Create => {
                let mut lock = state.lock().await;

                for room_id in 0..255 {
                    if lock.0.contains_key(&room_id) {
                        continue;
                    }

                    lock.0.insert(room_id, Room::new(uuid));
                    websocket.send(ServerMessage::Created(room_id).into()).await?;

                    log::info!("(peer {}) created {}", uuid, room_id);
                    continue 'msg;
                }

                websocket.send(ServerMessage::Failed.into()).await?;
                log::error!("(peer {}) tried to create room but all are used", uuid);
            }
        }

        log::info!("session state: {:#?}", state);
    }
}
