use anyhow::bail;
use async_std::net::TcpListener;
use async_std::task;
use async_tungstenite::accept_async;
use simple_logger::SimpleLogger;

use signaling::protocol;
use signaling::state;

#[async_std::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    let state = state::ServerState::new();
    let listener = TcpListener::bind("0.0.0.0:8192").await?;

    log::info!("started listening on wss://0.0.0.0:8192");

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
                let state = state.clone();

                task::spawn(async move {
                    let websocket = match accept_async(tcp).await {
                        Ok(websocket) => websocket,
                        Err(e) => bail!("failed to accept websocket connection! ({:?})", e),
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
