use async_std::net::TcpStream;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use async_sub::Observable;
use async_tungstenite::WebSocketStream;
use serde::{Serialize, Deserialize};

pub type RoomMap = HashMap<u8, Room>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Room {
    creator: Uuid,
    peers: HashSet<Uuid>,
    negotiation: Observable<NegotiationState>
}

impl Room {
    pub fn new(creator: Uuid) -> Room {
        Room {
            creator,
            peers: {
                let mut set = HashSet::new();
                set.insert(creator);
                set
            },
            negotiation: Observable::new(NegotiationState::Initial)
        }
    }

    pub fn join(&mut self, peer: Uuid) {
        self.peers.insert(peer);
    }
}

#[derive(Clone, Debug)]
pub enum NegotiationState {
    Initial,
    Offered(WebSocketStream<async_rustls::server::TlsStream<TcpStream>>, ()),
    Answered,
    Connected
}