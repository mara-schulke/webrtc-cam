use crate::signals::Signal;
use anyhow::{anyhow, bail, Context};
use async_std::net::TcpStream;
use async_tungstenite::WebSocketStream;
use futures::{SinkExt, StreamExt};
use url::Url;

pub type WebSocket = WebSocketStream<TcpStream>;

const SIGNALING_SERVER_ADDRESS_LOCAL: &str = "ws://localhost:80/0000000000000000000000000000000000000000000000000000000000000000/stream?peer=device";

pub async fn read_from_ws(websocket: &mut WebSocket) -> anyhow::Result<Signal> {
    use async_tungstenite::tungstenite::Message;

    let message = loop {
        match websocket.next().await {
            None => bail!("websocket stream ended unexpectedly"),
            Some(Ok(Message::Binary(json))) => break serde_json::from_slice(json.as_slice())?,
            Some(Ok(Message::Text(json))) => break serde_json::from_str(json.as_str())?,
            Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
            Some(Ok(Message::Close(_))) => bail!("websocket: received close"),
            Some(Err(err)) => bail!("websocket stream error: {:?}", err),
        }
    };

    Ok(message)
}

pub async fn write_to_ws(websocket: &mut WebSocket, msg: &Signal) -> anyhow::Result<()> {
    use async_tungstenite::tungstenite::Message;
    let message = Message::Text(serde_json::to_string(msg)?);
    websocket
        .send(message)
        .await
        .with_context(|| anyhow!("Failed to send message {:#?}", msg))
}

pub async fn entry() -> anyhow::Result<WebSocket> {
    log::info!("Starting signaling!");

    let websocket = {
        let url = Url::parse(SIGNALING_SERVER_ADDRESS_LOCAL)?;
        let addrs = url.socket_addrs(|| None).unwrap();
        let tcp = TcpStream::connect(&*addrs).await?;
        let (connection, _) = async_tungstenite::client_async(url.to_string(), tcp).await?;
        connection
    };

    Ok(websocket)
}
