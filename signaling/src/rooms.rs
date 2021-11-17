use uuid::Uuid;
use anyhow::ensure;
use std::convert::TryFrom;
use std::fmt;
use serde::{Serialize, Deserialize};

#[cfg(feature = "server")]
use std::collections::{HashMap, HashSet};
#[cfg(feature = "server")]
use async_broadcast::{Receiver, Sender, broadcast};
#[cfg(feature = "server")]
use async_std::sync::{Mutex, Arc};
#[cfg(feature = "server")]
use async_std::task::block_on;

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct RoomId(Uuid, Uuid);

impl TryFrom<String> for RoomId {
    type Error = anyhow::Error;

    fn try_from(s: String) -> anyhow::Result<Self> {
        ensure!(s.len() == 64, "Should be a 64 char hex string");
        Ok(Self(Uuid::from_slice(&s.as_bytes()[0..16])?, Uuid::from_slice(&s.as_bytes()[16..32])?))
    }
}

impl From<RoomId> for String {
    fn from(id: RoomId) -> String {
        format!("{}{}", &id.0.to_simple(), &id.1.to_simple())
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", String::from(*self))
    }
}

impl fmt::Debug for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("RoomId").field(&String::from(*self)).finish()
    }
}

#[cfg(feature = "server")]
pub type RoomMap = HashMap<RoomId, Room>;

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

#[cfg(feature = "server")]
#[derive(Clone)]
pub struct Room {
    id: RoomId,
    creator: Uuid,
    inner: Arc<Mutex<Inner>>
}

#[cfg(feature = "server")]
impl fmt::Debug for Room {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Room")
            .field("id", &self.id)
            .field("creator", &self.creator)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "server")]
#[derive(Clone, Debug)]
struct Inner {
    peers: HashSet<Uuid>,
    channel: (Sender<Message>, Receiver<Message>)
}

#[cfg(feature = "server")]
impl Room {
    pub fn new(creator: Uuid, id: RoomId) -> Room {
        Room {
            id,
            creator,
            inner: Arc::new(Mutex::new(Inner {
                peers: HashSet::new(),
                channel: broadcast(4096)
            }))
        }
    }

    pub async fn join(&self, peer: Uuid) -> RoomHandle {
        RoomHandle::new(peer, self.clone()).await
    }
}

#[cfg(feature = "server")]
pub struct RoomHandle(Uuid, Room, Sender<Message>, Receiver<Message>);

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
impl Drop for RoomHandle {
    fn drop(&mut self) {
        let mut inner = block_on(self.1.inner.lock());
        inner.peers.remove(&self.0);
        block_on(self.2.broadcast(Message::Leave{ peer: self.0 })).ok();
    }
}