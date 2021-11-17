use async_std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;
use std::collections::HashSet;
use crate::rooms::RoomMap;

#[derive(Clone, Debug)]
pub struct ServerState(Arc<Mutex<(RoomMap, HashSet<Uuid>)>>);

impl<'a> ServerState {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new((RoomMap::new(), HashSet::new()))))
    }

    pub async fn lock(&'a self) -> MutexGuard<'a, (RoomMap, HashSet<Uuid>)> {
        self.0.lock().await
    }
}