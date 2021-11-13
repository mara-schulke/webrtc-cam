use anyhow::{bail, Context};
use async_tungstenite::tungstenite::Message;
use async_std::net::TcpStream;
use async_rustls::server::TlsStream;
use async_tungstenite::WebSocketStream;
use serde::{Deserialize, Serialize};
use futures::{FutureExt, SinkExt, StreamExt, select};
use uuid::Uuid;
use crate::rooms::{self, Room, RoomHandle};
use crate::ServerState;

type WebSocket = WebSocketStream<TlsStream<TcpStream>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PeerMessage {
    Create,   // Create a room
    Join(u8), // Join a room

    Signal(crate::signals::IceOrSdp), // Signaling
    Data(String) // Plain Data Msg
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerMessage {
    Hello(Uuid),
    Created(u8),

    Joined,
    Error(String),

    Room(rooms::Message),
}

impl From<ServerMessage> for Message {
    fn from(server_msg: ServerMessage) -> Message {
        Message::Binary(serde_json::to_vec(&server_msg).unwrap())
    }
}

pub async fn handler(
    mut websocket: WebSocket,
    state: ServerState
) -> anyhow::Result<()> {
    log::info!("websocket connection established");

    let uuid = Uuid::new_v4();

    let mut lock = state.lock().await;
    lock.1.insert(uuid);
    drop(lock);

    websocket.send(ServerMessage::Hello(uuid).into()).await?;

    log::info!("registered peer {:?}", uuid);

    let mut room_handle = create_or_join_room(&mut websocket, uuid.clone(), &state).await?;

    log::info!("room state: {:#?}", room_handle);
    log::info!("entered signaling loop for {}", uuid);

    loop {
        select! {
            peer_msg = read_peer_msg(&mut websocket).fuse() => {
                log::info!("received {:#?} from peer {}", peer_msg, uuid);

                if peer_msg.is_err() {
                    break Err(peer_msg.unwrap_err());
                }

                match peer_msg.unwrap() {
                    PeerMessage::Signal(signal) => room_handle.send(rooms::Message::Signal { peer: uuid.clone(), signal }).await?,
                    PeerMessage::Data(data) => room_handle.send(rooms::Message::Data { peer: uuid.clone(), data }).await?,
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
                        | rooms::Message::Data { peer, .. }
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

async fn read_peer_msg(websocket: &mut WebSocket) -> anyhow::Result<PeerMessage> {
    let websocket_msg = websocket.next().await
        .context("websocket stream ended unexpectedly")?;

    loop {
        match websocket_msg {
            Ok(Message::Text(_)) => log::error!("unexpected message type"),
            Ok(Message::Ping(_) | Message::Pong(_)) => continue,
            Ok(Message::Binary(blob)) => break serde_json::from_slice(blob.as_slice()).map_err(|e| e.into()),
            Ok(Message::Close(_)) => bail!("websocket: received close"),
            Err(e) => bail!("websocket: failed to read message ({:?})", e)
        }
    }
}

async fn create_or_join_room(websocket: &mut WebSocket, id: Uuid, server_state: &ServerState) -> anyhow::Result<RoomHandle> {
    let message = read_peer_msg(websocket).await?;

    let handle = match message {
        PeerMessage::Join(room_id) => {
            let mut lock = server_state.lock().await;
            let room = lock.0.get_mut(&room_id);

            if room.is_none() {
                websocket.send(ServerMessage::Error(format!("Room {} does not exist", room_id)).into()).await?;
                bail!("(peer {}) tried to join {} but it doesnt exist", id, room_id);
            }

            let handle = room.unwrap().join(id.clone()).await;

            log::info!("(peer {}) joined {}", id, room_id);
            websocket.send(ServerMessage::Joined.into()).await?;

            handle
        }
        PeerMessage::Create => {
            let mut lock = server_state.lock().await;
            let room_id = match (0..255).skip_while(|id| lock.0.contains_key(&id)).take(1).next() {
                Some(room_id) => room_id,
                None => {
                    websocket.send(ServerMessage::Error("All rooms are in use".to_string()).into()).await?;
                    bail!("(peer {}) tried to create room but all are used", id);
                }
            };
            let room = Room::new(id.clone());
            let handle = room.join(id.clone()).await;

            lock.0.insert(room_id, room);
            drop(lock);
            websocket.send(ServerMessage::Created(room_id).into()).await?;
            log::info!("(peer {}) created {}", id, room_id);

            handle
        }
        _ => {
            websocket.send(ServerMessage::Error("Protocol error, join or create a room first".to_string()).into()).await?;
            bail!("(peer {}) ignored protocol", id);
        }
    };

    Ok(handle)
}

