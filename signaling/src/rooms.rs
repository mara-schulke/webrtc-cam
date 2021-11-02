use async_std::sync::{Arc, Mutex, MutexGuard};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct RoomMap(Arc<Mutex<HashMap<Uuid, String>>>);

impl<'a> RoomMap {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    pub async fn lock(&'a self) -> MutexGuard<'a, HashMap<Uuid, String>> {
        self.0.lock().await
    }
}
