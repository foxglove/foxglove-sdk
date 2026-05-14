//! rustls crypto provider selection.

/// Installs aws-lc-rs as the process-wide default rustls crypto provider.
///
/// We have both ring and aws-lc-rs in the dependency tree, so rustls cannot pick
/// one automatically. See FLE-231 for introducing explicit crate features to
/// select the crypto provider.
pub(crate) fn install_default_crypto_provider() {
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    if provider.install_default().is_err() {
        tracing::debug!("rustls crypto provider already installed; using the existing provider");
    }
}
