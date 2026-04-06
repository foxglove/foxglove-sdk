use std::ffi::c_void;

use foxglove::websocket;

use crate::{FoxgloveString, bytes::FoxgloveBytes};

enum AssetResponderVariant {
    WebSocket(websocket::AssetResponder),
    #[cfg(feature = "remote-access")]
    Gateway(foxglove::remote_access::AssetResponder),
}

impl AssetResponderVariant {
    fn respond_ok(self, data: &[u8]) {
        match self {
            Self::WebSocket(r) => r.respond_ok(data),
            #[cfg(feature = "remote-access")]
            Self::Gateway(r) => r.respond_ok(data),
        }
    }

    fn respond_err(self, message: impl AsRef<str>) {
        match self {
            Self::WebSocket(r) => r.respond_err(message),
            #[cfg(feature = "remote-access")]
            Self::Gateway(r) => r.respond_err(message),
        }
    }
}

pub struct FoxgloveFetchAssetResponder(AssetResponderVariant);
impl FoxgloveFetchAssetResponder {
    /// Moves the responder to the heap and returns a raw pointer.
    ///
    /// After calling this function, the caller is responsible for eventually calling
    /// [`Self::from_raw`] to recover the responder.
    fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }

    /// Recovers the boxed responder from a raw pointer.
    ///
    /// # Safety
    /// - The raw pointer must have been obtained from [`Self::into_raw`]
    unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }
}

#[derive(Clone)]
pub(crate) struct FetchAssetHandler {
    callback_context: *const c_void,
    callback: unsafe extern "C" fn(
        *const c_void,
        *const FoxgloveString,
        *mut FoxgloveFetchAssetResponder,
    ),
}
impl FetchAssetHandler {
    pub fn new(
        callback_context: *const c_void,
        callback: unsafe extern "C" fn(
            *const c_void,
            *const FoxgloveString,
            *mut FoxgloveFetchAssetResponder,
        ),
    ) -> Self {
        Self {
            callback_context,
            callback,
        }
    }
}
unsafe impl Send for FetchAssetHandler {}
unsafe impl Sync for FetchAssetHandler {}

impl websocket::AssetHandler<websocket::Client> for FetchAssetHandler {
    fn fetch(&self, uri: String, responder: websocket::AssetResponder) {
        let c_uri = FoxgloveString::from(&uri);
        let c_responder =
            FoxgloveFetchAssetResponder(AssetResponderVariant::WebSocket(responder)).into_raw();
        // SAFETY: It's the callback implementation's responsibility to ensure that this callback
        // function pointer remains valid for the lifetime of the websocket server, as described in
        // the safety requirements of `foxglove_server_options.fetch_asset`.
        unsafe { (self.callback)(self.callback_context, &raw const c_uri, c_responder) };
    }
}

#[cfg(feature = "remote-access")]
impl foxglove::remote_access::AssetHandler<foxglove::remote_access::Client> for FetchAssetHandler {
    fn fetch(&self, uri: String, responder: foxglove::remote_access::AssetResponder) {
        let c_uri = FoxgloveString::from(&uri);
        let c_responder =
            FoxgloveFetchAssetResponder(AssetResponderVariant::Gateway(responder)).into_raw();
        // SAFETY: It's the callback implementation's responsibility to ensure that this callback
        // function pointer remains valid for the lifetime of the gateway, as described in
        // the safety requirements of `foxglove_gateway_options.fetch_asset`.
        unsafe { (self.callback)(self.callback_context, &raw const c_uri, c_responder) };
    }
}

/// Completes a fetch asset request by sending asset data to the client.
///
/// # Safety
/// - `responder` must be a pointer to a `foxglove_fetch_asset_responder` obtained via a
///   `fetch_asset` callback. This value is moved into this function, and must not be accessed
///   afterwards.
/// - `data` must be a pointer to the response data. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_fetch_asset_respond_ok(
    responder: *mut FoxgloveFetchAssetResponder,
    data: FoxgloveBytes,
) {
    let responder = unsafe { FoxgloveFetchAssetResponder::from_raw(responder) };
    let data = unsafe { data.as_slice() };
    responder.0.respond_ok(data);
}

/// Completes a request by sending an error message to the client.
///
/// # Safety
/// - `responder` must be a pointer to a `foxglove_fetch_asset_responder` obtained via a
///   `fetch_asset` callback. This value is moved into this function, and must not be accessed
///   afterwards.
/// - `message` must be a pointer to a valid UTF-8 string. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_fetch_asset_respond_error(
    responder: *mut FoxgloveFetchAssetResponder,
    message: FoxgloveString,
) {
    let responder = unsafe { FoxgloveFetchAssetResponder::from_raw(responder) };
    let message = unsafe { message.as_utf8_str() };
    let message = match message {
        Ok(s) => s.to_string(),
        Err(e) => format!("Server produced an invalid error message: {e}"),
    };
    responder.0.respond_err(message);
}
