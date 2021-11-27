use uuid::Uuid;
use anyhow::{anyhow, bail, Context};
use async_tungstenite::WebSocketStream;
use futures::{SinkExt, StreamExt};
use url::Url;
use crate::Environment;
use signaling_types::rooms::RoomId;
use signaling_types::protocol::{PeerMessage, ServerMessage};

pub type WebSocket = WebSocketStream<async_rustls::TlsStream<async_std::net::TcpStream>>;

const SIGNALING_SERVER_ADDRESS: &str = "wss://signaling.schulke.xyz:8192";
const SIGNALING_SERVER_ADDRESS_LOCAL: &str = "wss://localhost:8192";

pub async fn read_from_ws(websocket: &mut WebSocket) -> anyhow::Result<ServerMessage> {
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

pub async fn write_to_ws(websocket: &mut WebSocket, msg: &PeerMessage) -> anyhow::Result<()> {
    use async_tungstenite::tungstenite::Message;
    let message = Message::Binary(serde_json::to_vec(msg)?);
    websocket.send(message).await.with_context(|| anyhow!("Failed to send message {:#?}", msg))
}

pub async fn entry(environment: Environment) -> anyhow::Result<WebSocket> {
    log::info!("Starting signaling!");

    let mut websocket = {
        let url = Url::parse(SIGNALING_SERVER_ADDRESS_LOCAL)?;
        let tls_stream = tls::get_dangerous_tls_stream(&url).await?;
        let (connection, _) = async_tungstenite::client_async(url.to_string(), tls_stream).await?;
        connection
    };

    let uuid = match read_from_ws(&mut websocket).await? {
        ServerMessage::Hello(uuid) => uuid,
        _ => bail!("Unexpected message from server")
    };

    log::info!("We have this uuid: {}", uuid.to_simple());

    let room_id = RoomId::new(Uuid::from_slice(&[255; 16])?, Uuid::from_slice(&[255; 16])?);
    write_to_ws(&mut websocket, &PeerMessage::JoinOrCreate(room_id)).await?;

    match read_from_ws(&mut websocket).await? {
        ServerMessage::Joined => log::info!("Joined room {}", room_id),
        msg => bail!("Unexpected message from server: {:#?}", msg)
    }

    Ok(websocket)
}

mod tls {
    use async_rustls::{rustls::ClientConfig, webpki::DNSNameRef, TlsConnector};
    use async_std::net::TcpStream;
    use async_std::sync::Arc;
    use url::Url;

    pub type TlsStream = async_rustls::TlsStream<TcpStream>;

    pub async fn get_dangerous_tls_stream(uri: &Url) -> anyhow::Result<TlsStream> {
        mod dangerous {
            use async_rustls::rustls::{Certificate, RootCertStore, TLSError};
            use async_rustls::rustls::{ServerCertVerified, ServerCertVerifier};

            pub struct NoCertificateVerification {}

            impl ServerCertVerifier for NoCertificateVerification {
                fn verify_server_cert(
                    &self,
                    _: &RootCertStore,
                    _: &[Certificate],
                    _: async_rustls::webpki::DNSNameRef<'_>,
                    _: &[u8],
                ) -> Result<ServerCertVerified, TLSError> {
                    Ok(ServerCertVerified::assertion())
                }
            }
        }

        let addrs = uri.socket_addrs(|| None).unwrap();
        let tcp = TcpStream::connect(&*addrs).await?;

        let mut config = ClientConfig::new();
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(dangerous::NoCertificateVerification {}));

        let connector = TlsConnector::from(Arc::new(config));
        let domain = DNSNameRef::try_from_ascii_str("x")?;
        let tls = connector.connect(domain, tcp).await?;

        Ok(tls.into())
    }
}