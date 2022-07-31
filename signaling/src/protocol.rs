use crate::rooms::{self, RoomId};
use crate::signals;
use async_tungstenite::tungstenite::Message;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(feature = "server")]
use crate::rooms::{Room, RoomHandle};
#[cfg(feature = "server")]
use crate::state::ServerState;
#[cfg(feature = "server")]
use anyhow::{bail, Context};
#[cfg(feature = "server")]
use async_std::net::TcpStream;
#[cfg(feature = "server")]
use async_tungstenite::WebSocketStream;
#[cfg(feature = "server")]
use futures::{select, FutureExt, SinkExt, StreamExt};
#[cfg(feature = "server")]
type WebSocket = WebSocketStream<TcpStream>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PeerMessage {
    JoinOrCreate(RoomId),
    Signal(signals::IceOrSdp),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerMessage {
    Hello(Uuid),
    Joined,

    Error(String),
    Room(rooms::Message),
}

impl From<ServerMessage> for Message {
    fn from(server_msg: ServerMessage) -> Message {
        Message::Binary(serde_json::to_vec(&server_msg).unwrap())
    }
}

#[cfg(feature = "server")]
pub async fn handler(mut websocket: WebSocket, state: ServerState) -> anyhow::Result<()> {
    log::info!("websocket connection established");

    let uuid = Uuid::new_v4();

    let mut lock = state.lock().await;
    lock.1.insert(uuid);
    drop(lock);

    websocket.send(ServerMessage::Hello(uuid).into()).await?;

    log::info!("registered peer {:?}", uuid);

    let mut room_handle = create_or_join_room(&mut websocket, uuid, &state).await?;

    log::info!("entered signaling loop for {}", uuid);

    loop {
        select! {
            peer_msg = read_peer_msg(&mut websocket).fuse() => {
                log::info!("received {:#?} from peer {}", peer_msg, uuid);

                if peer_msg.is_err() {
                    break Err(peer_msg.unwrap_err());
                }

                match peer_msg.unwrap() {
                    PeerMessage::Signal(signal) => room_handle.send(rooms::Message::Signal { peer: uuid, signal }).await?,
                    _ => {
                        websocket.send(ServerMessage::Error("Protocol error, only signaling allowed".to_string()).into()).await?;
                        bail!("(peer {}) ignored protocol", uuid);
                    }
                }
            },
            room_msg = room_handle.recv().fuse() => {
                log::info!("peer {} received room msg {:#?}", uuid, room_msg);
                if let Ok(room_msg) = room_msg {
                    let is_our_message = matches!(room_msg, rooms::Message::Join { peer }
                        | rooms::Message::Leave { peer }
                        | rooms::Message::Signal { peer, .. } if peer == uuid);

                    if !is_our_message {
                        websocket.send(ServerMessage::Room(room_msg).into()).await?;
                    } else {
                        log::warn!("received our own message");
                    }
                }
            }
        }
    }
}

#[cfg(feature = "server")]
async fn read_peer_msg(websocket: &mut WebSocket) -> anyhow::Result<PeerMessage> {
    let websocket_msg = websocket
        .next()
        .await
        .context("websocket stream ended unexpectedly")?;

    loop {
        match websocket_msg {
            Ok(Message::Text(_)) => log::error!("unexpected message type"),
            Ok(Message::Ping(_) | Message::Pong(_)) => continue,
            Ok(Message::Binary(blob)) => {
                break serde_json::from_slice(blob.as_slice()).map_err(|e| e.into())
            }
            Ok(Message::Close(_)) => bail!("websocket: received close"),
            Err(e) => bail!("websocket: failed to read message ({:?})", e),
        }
    }
}

#[cfg(feature = "server")]
async fn create_or_join_room(
    websocket: &mut WebSocket,
    id: Uuid,
    server_state: &ServerState,
) -> anyhow::Result<RoomHandle> {
    let message = read_peer_msg(websocket).await?;

    match message {
        PeerMessage::JoinOrCreate(room_id) => {
            let mut lock = server_state.lock().await;

            let handle = match lock.0.get(&room_id) {
                Some(room) => {
                    let cloned = room.clone();
                    drop(lock);
                    let handle = cloned.join(id).await;
                    log::info!("(peer {}) joined {}", id, room_id);
                    handle
                }
                None => {
                    let room = Room::new(id, room_id);
                    lock.0.insert(room_id, room.clone());
                    drop(lock);
                    let handle = room.join(id).await;
                    log::info!("(peer {}) created {}", id, room_id);
                    handle
                }
            };

            websocket.send(ServerMessage::Joined.into()).await?;

            Ok(handle)
        }
        _ => {
            websocket
                .send(
                    ServerMessage::Error("Protocol error, join or create a room first".to_string())
                        .into(),
                )
                .await?;
            bail!("(peer {}) ignored protocol", id);
        }
    }
}
