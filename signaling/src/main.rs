use anyhow::bail;
use async_std::net::TcpListener;
use async_std::task;
use async_std::sync::{Arc, Mutex, MutexGuard};
use async_tungstenite::accept_async;
use simple_logger::SimpleLogger;
use std::collections::HashSet;
use uuid::Uuid;

mod protocol;
mod rooms;
mod tls;
mod signals;

use rooms::RoomMap;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum Environment {
    Production,
    Development
}

const ENVIRONMENT: Environment = Environment::Development;

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

#[async_std::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    let state = ServerState::new();
    let listener = TcpListener::bind("127.0.0.1:8192").await?;

    log::info!("running in mode {:?}", ENVIRONMENT);
    log::info!("started listening on wss://127.0.0.1:8192");

    let acceptor = match ENVIRONMENT {
        Environment::Production => tls::create_acme_acceptor().await,
        Environment::Development => tls::create_self_signed_acceptor().await
    };

    loop {
        match listener.accept().await {
            Ok((tcp, _)) => {
                let acceptor = acceptor.clone();
                let state = state.clone();

                task::spawn(async move {
                    let tls = match acceptor.accept(tcp).await {
                        Ok(tls) => tls,
                        _ => bail!("tls: failed to accept tls stream")
                    };

                    let websocket = match accept_async(tls).await {
                        Ok(websocket) => websocket,
                        Err(e) => bail!("failed to accept websocket connection! ({:?})", e)
                    };

                    match protocol::handler(websocket, state).await {
                        Ok(_) => log::info!("websocket handler exited!"),
                        Err(e) => log::error!("websocket handler crashed! ({:?})", e),
                    }

                    Ok(())
                });
            }
            Err(err) => log::error!("accept: {:?}", err),
        }
    }
}
