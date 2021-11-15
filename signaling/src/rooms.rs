use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use async_broadcast::{Receiver, Sender, broadcast};
use async_std::sync::{Mutex, Arc};
use async_std::task::block_on;
use serde::{Serialize, Deserialize};

pub type RoomMap = HashMap<u8, Room>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Message {
    Join {
        peer: Uuid
    },
    Leave {
        peer: Uuid
    },
    Signal {
        peer: Uuid,
        signal: crate::signals::IceOrSdp
    }
}

#[derive(Clone, Debug)]
pub struct Room {
    creator: Uuid,
    inner: Arc<Mutex<Inner>>
}

#[derive(Clone, Debug)]
struct Inner {
    peers: HashSet<Uuid>,
    channel: (Sender<Message>, Receiver<Message>)
}

impl Room {
    pub fn new(creator: Uuid) -> Room {
        Room {
            creator,
            inner: Arc::new(Mutex::new(Inner {
                peers: HashSet::new(),
                channel: broadcast(512)
            }))
        }
    }

    pub async fn join(&self, peer: Uuid) -> RoomHandle {
        RoomHandle::new(peer, self.clone()).await
    }
}

#[derive(Debug)]
pub struct RoomHandle(Uuid, Room, Sender<Message>, Receiver<Message>);

impl RoomHandle {
    pub async fn new(peer: Uuid, room: Room) -> Self {
        let mut inner = room.inner.lock().await;
        inner.peers.insert(peer);
        let (s, r) = inner.channel.clone();
        s.broadcast(Message::Join { peer }).await.ok();
        drop(inner);
        Self(peer, room, s, r)
    }

    pub async fn send(&mut self, msg: Message) -> anyhow::Result<()> {
        self.2.broadcast(msg).await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> anyhow::Result<Message> {
        self.3.recv().await.map_err(|e| e.into())
    }
}

impl Drop for RoomHandle {
    fn drop(&mut self) {
        let mut inner = block_on(self.1.inner.lock());
        inner.peers.remove(&self.0);
        block_on(self.2.broadcast(Message::Leave{ peer: self.0 })).ok();
    }
}