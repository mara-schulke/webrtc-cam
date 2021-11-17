use anyhow::bail;
use async_std::net::TcpListener;
use async_std::sync::{Arc, Mutex, MutexGuard};
use async_std::task;
use async_tungstenite::accept_async;
use simple_logger::SimpleLogger;
use std::collections::HashSet;
use structopt::StructOpt;
use uuid::Uuid;
use std::str::FromStr;

mod protocol;
mod rooms;
mod tls;
mod signals;

use rooms::RoomMap;

#[derive(Clone, Debug)]
pub enum Environment {
    Production,
    Development
}

impl FromStr for Environment {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Environment> {
        match s {
            "dev" | "development" => Ok(Environment::Development),
            "prod" | "production" => Ok(Environment::Production),
            _ => bail!("Unknown environment")
        }
    }
}

#[derive(StructOpt)]
struct Options {
    #[structopt(short = "e", long = "env")]
    environment: Environment
}

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
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    let Options { environment } = Options::from_args();
    let state = ServerState::new();
    let listener = TcpListener::bind("0.0.0.0:8192").await?;

    log::info!("running in mode {:?}", environment);
    log::info!("started listening on wss://0.0.0.0:8192");

    let acceptor = match environment {
        Environment::Production => tls::create_acme_acceptor().await,
        Environment::Development => tls::create_self_signed_acceptor().await
    };

    let log_state = state.clone();
    task::spawn(async move {
        loop {
            let lock = log_state.0.lock().await;
            log::info!("Server State: {:#?}", lock);
            drop(lock);
            task::sleep(std::time::Duration::from_millis(3000)).await;
        }
    });

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
