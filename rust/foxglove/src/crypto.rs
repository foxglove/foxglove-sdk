//! rustls crypto provider selection.

/// Installs aws-lc-rs as the process-wide default rustls crypto provider.
///
/// We have both ring and aws-lc-rs in the dependency tree, so rustls cannot pick
/// one automatically. See FLE-231 for introducing explicit crate features to
/// select the crypto provider.
pub(crate) fn install_default_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}
