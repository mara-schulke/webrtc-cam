mod signaling;
mod signals;
mod webrtc;

use async_std::task;
use simple_logger::SimpleLogger;

#[async_std::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    async fn stream() -> anyhow::Result<()> {
        let websocket = signaling::entry().await?;
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
