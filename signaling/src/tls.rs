use async_std::fs;
use async_std::sync::Arc;
use async_rustls::rustls::{NoClientAuth, ServerConfig};
use async_std::net::TcpStream;
use async_std::path::Path;
use async_std::task;
use rustls_acme::{acme, ResolvesServerCertUsingAcme, TlsAcceptor};

#[derive(Clone)]
pub enum Acceptor {
    ACME(rustls_acme::TlsAcceptor),
    SelfSigned(async_rustls::TlsAcceptor)
}

impl Acceptor {
    pub async fn accept(&self, tcp: TcpStream) -> Result<async_rustls::server::TlsStream<TcpStream>, std::io::Error> {
        match self {
            Acceptor::SelfSigned(self_signed_acceptor) => self_signed_acceptor.accept(tcp).await,
            Acceptor::ACME(acme_acceptor) => acme_acceptor.accept(tcp).await
        }
    }

}

pub async fn create_acme_acceptor() -> Acceptor {
    let resolver = ResolvesServerCertUsingAcme::new();
    let config = ServerConfig::new(NoClientAuth::new());
    let acceptor = TlsAcceptor::new(config, resolver.clone());
    let domains = vec!["signaling.schulke.xyz".into()];
    let cache_dir = Path::new("/tmp/signaling-cert-cache");

    task::spawn(async move {
        resolver
            .run(
                if cfg!(debug_assertions) {
                    acme::LETS_ENCRYPT_STAGING_DIRECTORY
                } else {
                    acme::LETS_ENCRYPT_PRODUCTION_DIRECTORY
                },
                domains,
                Some(cache_dir),
            )
            .await;
    });

    Acceptor::ACME(acceptor)
}

pub async fn create_self_signed_acceptor() -> Acceptor {
    const BASE_PATH: &str = "./keys";
    const KEY_FILE: &str = "domain_key,localhost,2020-12-01,p256.pem";
    const CERT_FILE: &str = "domain_cert,localhost,2020-12-01,p256.pem";

    let mut tls_config = ServerConfig::new(async_rustls::rustls::NoClientAuth::new());

    let key = {
        let key = fs::read(Path::new(BASE_PATH).join(KEY_FILE)).await.unwrap();
        rustls::internal::pemfile::pkcs8_private_keys(&mut key.as_slice()).unwrap().remove(0)
    };

    let cert = {
        let cert = fs::read(Path::new(BASE_PATH).join(CERT_FILE)).await.unwrap();
        rustls::internal::pemfile::certs(&mut cert.as_slice()).unwrap()
    };

    tls_config
        .set_single_cert(cert, key)
        .unwrap();

    Acceptor::SelfSigned(
        async_rustls::TlsAcceptor::from(Arc::new(tls_config))
    )
}