//! rustls crypto provider selection.
//!
//! This module is only compiled when a TLS-using feature (`websocket-tls` or
//! `remote-access`) is enabled. The two compile_errors below enforce, at that
//! point, that the caller has selected exactly one crypto backend.

#[cfg(not(any(feature = "aws-lc-rs", feature = "ring")))]
compile_error!(
    "Enable one of the `aws-lc-rs` or `ring` crate features to provide a rustls \
     crypto backend for TLS."
);

#[cfg(all(feature = "aws-lc-rs", feature = "ring"))]
compile_error!("The `aws-lc-rs` and `ring` features are mutually exclusive.");

/// Installs the configured rustls crypto provider as the process-wide default.
///
/// The provider is selected at compile time by the `aws-lc-rs` or `ring` crate
/// feature; the compile_errors above guarantee exactly one is enabled. Called
/// internally before opening any TLS connections.
///
/// Applications that want to install a different provider should call
/// [`rustls::crypto::CryptoProvider::install_default`] themselves before Foxglove
/// initiates any TLS work; subsequent calls are no-ops.
pub(crate) fn install_default_crypto_provider() {
    // The mutex compile_error above guarantees these branches are mutually exclusive;
    // the explicit `not(...)` qualifier silences a dead-code warning during the
    // failed build when a user does enable both.
    #[cfg(feature = "aws-lc-rs")]
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    #[cfg(all(feature = "ring", not(feature = "aws-lc-rs")))]
    let provider = rustls::crypto::ring::default_provider();

    if provider.install_default().is_err() {
        tracing::debug!("rustls crypto provider already installed; using the existing provider");
    }
}
