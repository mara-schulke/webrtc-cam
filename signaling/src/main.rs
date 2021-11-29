use anyhow::bail;
use async_std::net::TcpListener;
use async_std::task;
use async_tungstenite::accept_async;
use simple_logger::SimpleLogger;
use structopt::StructOpt;
use std::str::FromStr;

use signaling::protocol;
use signaling::state;
use signaling::tls;

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

#[async_std::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    let Options { environment } = Options::from_args();
    let state = state::ServerState::new();
    let listener = TcpListener::bind("0.0.0.0:8192").await?;

    log::info!("running in mode {:?}", environment);
    log::info!("started listening on wss://0.0.0.0:8192");

    let acceptor = match environment {
        Environment::Production => tls::create_acme_acceptor().await,
        Environment::Development => tls::create_self_signed_acceptor().await
    };

    let log_state = state.clone();
    task::spawn(async move {
        let lock = log_state.lock().await;
        let mut old_state = format!("{:#?}", lock);
        drop(lock);

        log::info!("state initialized {}", old_state);

        loop {
            task::sleep(std::time::Duration::from_millis(1000)).await;

            let lock = log_state.lock().await;
            let current_state = format!("{:#?}", lock);
            drop(lock);

            if current_state != old_state {
                log::info!("state changed {}", current_state);
                old_state = current_state;
            }
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
