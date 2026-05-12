//! rustls crypto provider selection.

#[cfg(not(any(feature = "aws-lc-rs", feature = "ring")))]
compile_error!(
    "Enable one of the `aws-lc-rs` or `ring` crate features to provide a rustls \
     crypto backend for TLS."
);

/// Installs the configured rustls crypto provider as the process-wide default.
///
/// The provider is selected at compile time by the `aws-lc-rs` or `ring` crate
/// feature. When both are enabled (e.g. `cargo --all-features`), `aws-lc-rs` is
/// preferred. Called internally before opening any TLS connections.
///
/// Applications that want to install a different provider should call
/// [`rustls::crypto::CryptoProvider::install_default`] themselves before Foxglove
/// initiates any TLS work; subsequent calls are no-ops.
pub(crate) fn install_default_crypto_provider() {
    #[cfg(feature = "aws-lc-rs")]
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    #[cfg(all(feature = "ring", not(feature = "aws-lc-rs")))]
    let provider = rustls::crypto::ring::default_provider();

    if provider.install_default().is_err() {
        tracing::debug!("rustls crypto provider already installed; using the existing provider");
    }
}
