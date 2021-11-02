use async_std::net::TcpListener;
use async_std::task;
use async_tungstenite::accept_async;
use simple_logger::SimpleLogger;

mod protocol;
mod rooms;

use rooms::RoomMap;

#[async_std::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    let rooms = RoomMap::new();
    let listener = TcpListener::bind("127.0.0.1:8192").await?;

    log::info!("started listening on 127.0.0.1:8192");

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let rooms = rooms.clone();

                task::spawn(async move {
                    if let Ok(websocket) = accept_async(stream).await {
                        match protocol::handler(websocket, rooms).await {
                            Ok(_) => log::info!("websocket handler exited!"),
                            Err(e) => log::error!("websocket handler crashed! ({:?})", e),
                        }
                    }

                    log::error!("failed to accept websocket connection!");
                });
            }
            Err(err) => log::error!("accept: {:?}", err),
        }
    }
}