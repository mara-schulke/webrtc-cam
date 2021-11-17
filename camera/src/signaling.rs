use async_tungstenite::WebSocketStream;
use async_std::channel::{Sender, Receiver};
use url::Url;
use crate::Environment;
use signaling_types::protocol::{PeerMessage, ServerMessage};

pub type WebSocket = WebSocketStream<async_rustls::TlsStream<async_std::net::TcpStream>>;

const SIGNALING_SERVER_ADDRESS: &str = "wss://signaling.schulke.xyz:8192";
const SIGNALING_SERVER_ADDRESS_LOCAL: &str = "wss://localhost:8192";

pub async fn entry(environment: Environment) -> anyhow::Result<WebSocket> {
    log::info!("Starting signaling!");

    let websocket = {
        let url = Url::parse(SIGNALING_SERVER_ADDRESS_LOCAL)?;
        let tls_stream = tls::get_dangerous_tls_stream(&url).await?;
        let (connection, _) = async_tungstenite::client_async(url.to_string(), tls_stream).await?;
        connection
    };

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