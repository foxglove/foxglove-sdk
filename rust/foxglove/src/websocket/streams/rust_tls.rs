//! Provides TLS support using rustls

use std::sync::Arc;

use tokio::net::TcpStream;
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    },
};
use tokio_util::either::Either;

use crate::{
    FoxgloveError,
    websocket::streams::{Acceptor, ServerStream, TlsIdentity},
};

pub(crate) type TlsStream<S> = tokio_rustls::server::TlsStream<S>;

pub struct StreamConfiguration {
    tls_acceptor: Option<TlsAcceptor>,
}

fn build_tls_acceptor(tls_identity: &TlsIdentity) -> Result<TlsAcceptor, FoxgloveError> {
    // Install aws-lc-rs as the default crypto provider, because we have both ring and aws-lc-rs in the dependency tree
    // TODO: can we choose one or the other via a crate feature (or flag on tls_identity?)
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let cert = CertificateDer::from_pem_slice(&tls_identity.cert)
        .map_err(|e| FoxgloveError::ConfigurationError(format!("TLS configuration: {e}")))?;

    let key = PrivateKeyDer::from_pem_slice(&tls_identity.key)
        .map_err(|e| FoxgloveError::ConfigurationError(format!("TLS configuration: {e}")))?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| FoxgloveError::ConfigurationError(format!("TLS configuration: {e}")))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

impl StreamConfiguration {
    pub fn new(identity: Option<&TlsIdentity>) -> Result<Self, FoxgloveError> {
        let tls_acceptor = if let Some(identity) = identity {
            let acceptor = build_tls_acceptor(identity)?;
            Some(acceptor)
        } else {
            None
        };
        Ok(Self { tls_acceptor })
    }
}

impl Acceptor for StreamConfiguration {
    async fn accept(
        &self,
        stream: TcpStream,
    ) -> Result<ServerStream<TcpStream>, crate::FoxgloveError> {
        let stream = if let Some(tls_acceptor) = &self.tls_acceptor {
            let stream = tls_acceptor.accept(stream).await?;
            Either::Right(stream)
        } else {
            Either::Left(stream)
        };
        Ok(stream)
    }

    fn accepts_tls(&self) -> bool {
        self.tls_acceptor.is_some()
    }
}
