mod webrtc;
mod signaling;

use url::Url;
use simple_logger::SimpleLogger;
use async_std::task;

#[derive(Clone, Copy, Debug)]
pub enum Environment {
    Development
}

const ENVIRONMENT: Environment = Environment::Development;

struct Options {
    environment: Environment,
    signaling_server: Url,
    room_id: signaling_types::rooms::RoomId
}

#[async_std::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();
    gstreamer::init().unwrap();

    async fn stream() -> anyhow::Result<()> {
        let websocket = signaling::entry(ENVIRONMENT).await?;
        webrtc::entry(websocket).await
    }

    loop {
        match stream().await {
            Ok(_) => log::info!("Stream ended expectedly, restarting"),
            Err(e) => log::error!("Stream ended unexpectedly ({}), trying again..", e),
        }
        task::sleep(std::time::Duration::from_millis(500)).await;
    }
}
