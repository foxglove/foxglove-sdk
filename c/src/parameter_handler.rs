use std::ffi::c_void;
use std::sync::Arc;

use foxglove::websocket::{
    AnyClient, GetParametersResponder, Parameter, ParameterHandler, SetParametersResponder,
};

use crate::FoxgloveString;
use crate::parameter::FoxgloveParameterArray;

/// Responder for a `getParameters` request from a client.
///
/// Obtained via the `get` callback of `foxglove_parameter_handler`. The implementation **must**
/// complete the request by calling `foxglove_get_parameters_responder_respond` exactly once (pass
/// an empty array if no values are available). It is safe to invoke that function synchronously
/// from the context of the callback. See `foxglove_get_parameters_responder_drop` for the
/// drop-without-responding contract.
pub struct FoxgloveGetParametersResponder(GetParametersResponder);

impl FoxgloveGetParametersResponder {
    fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }

    /// # Safety
    /// - The raw pointer must have been obtained from [`Self::into_raw`].
    unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }
}

/// Responder for a `setParameters` request from a client.
///
/// Obtained via the `set` callback of `foxglove_parameter_handler`. The implementation **must**
/// complete the request by calling `foxglove_set_parameters_responder_respond` exactly once with
/// the values that were actually applied (pass an empty array if the request could not be
/// handled). It is safe to invoke that function synchronously from the context of the callback.
/// See `foxglove_set_parameters_responder_drop` for the drop-without-responding contract.
///
/// When the request carried a `request_id`, the values passed to `respond` are echoed back to the
/// requesting client; otherwise the responder does nothing on the wire. The responder does not
/// notify other parameter subscribers, so the implementer is responsible for broadcasting applied
/// updates to subscribers on each sink (for example, via `foxglove_server_publish_parameter_values`
/// and `foxglove_gateway_publish_parameter_values`).
pub struct FoxgloveSetParametersResponder(SetParametersResponder);

impl FoxgloveSetParametersResponder {
    fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }

    /// # Safety
    /// - The raw pointer must have been obtained from [`Self::into_raw`].
    unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }
}

/// Handler for client-initiated parameter operations.
///
/// When supplied to `foxglove_server_options` or `foxglove_gateway_options`, the handler takes
/// precedence over the deprecated `on_get_parameters` / `on_set_parameters` callbacks on
/// `foxglove_server_callbacks` / `foxglove_gateway_callbacks`. Registering a handler also
/// automatically advertises the `FOXGLOVE_SERVER_CAPABILITY_PARAMETERS` (or
/// `FOXGLOVE_GATEWAY_CAPABILITY_PARAMETERS`) capability. Subscribe/unsubscribe notifications
/// still go through the `on_parameters_subscribe` / `on_parameters_unsubscribe` callbacks on
/// `foxglove_server_callbacks` / `foxglove_gateway_callbacks`; wire those up separately if you
/// want to be notified.
///
/// Both `get` and `set` are required: if a handler is supplied with either set to NULL,
/// `foxglove_server_start` / `foxglove_gateway_start` returns `FOXGLOVE_ERROR_VALUE_ERROR`.
///
/// These methods are invoked from time-sensitive contexts and must not block. If long-running
/// behavior is required, the implementation should hand the responder off to another thread and
/// return immediately.
#[repr(C)]
#[derive(Clone)]
pub struct FoxgloveParameterHandler {
    /// A user-defined value that will be passed to callback functions.
    pub context: *const c_void,

    /// Callback invoked when a client requests parameters.
    ///
    /// Required: must not be NULL when this handler is registered.
    ///
    /// The `request_id` argument may be NULL.
    ///
    /// The `param_names` argument is guaranteed to be non-NULL. The buffer is valid for the
    /// duration of this call; if the callback wishes to store these values, it must copy them out.
    ///
    /// The implementation takes ownership of `responder`; see `FoxgloveGetParametersResponder`
    /// for the completion contract.
    pub get: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            request_id: *const FoxgloveString,
            param_names: *const FoxgloveString,
            param_names_len: usize,
            responder: *mut FoxgloveGetParametersResponder,
        ),
    >,

    /// Callback invoked when a client sets parameters.
    ///
    /// Required: must not be NULL when this handler is registered.
    ///
    /// The `request_id` argument may be NULL.
    ///
    /// The `params` argument is guaranteed to be non-NULL. The buffer is valid for the duration of
    /// this call; if the callback wishes to store these values, it must copy them out.
    ///
    /// The implementation takes ownership of `responder`; see `FoxgloveSetParametersResponder`
    /// for the completion contract, the `request_id` echo behavior, and the implementer's
    /// responsibility to broadcast applied updates to other parameter subscribers.
    pub set: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            client_id: u32,
            request_id: *const FoxgloveString,
            params: *const FoxgloveParameterArray,
            responder: *mut FoxgloveSetParametersResponder,
        ),
    >,
}

// SAFETY: The `context` pointer and callback function pointers are provided by the C caller, who
// is responsible for ensuring they are safe to invoke from any thread.
unsafe impl Send for FoxgloveParameterHandler {}
unsafe impl Sync for FoxgloveParameterHandler {}

impl FoxgloveParameterHandler {
    /// Validates that both `get` and `set` are non-null.
    ///
    /// Mirrors the Rust [`ParameterHandler`] trait, which requires both methods.
    pub(crate) fn validate(&self) -> Result<(), foxglove::FoxgloveError> {
        if self.get.is_none() || self.set.is_none() {
            return Err(foxglove::FoxgloveError::ValueError(
                "foxglove_parameter_handler requires both `get` and `set` to be non-NULL"
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// Constructs an Arc<dyn ParameterHandler> trait object for use with the SDK server / gateway
    /// builders.
    pub(crate) fn into_arc(self) -> Arc<dyn ParameterHandler> {
        Arc::new(self)
    }
}

impl ParameterHandler for FoxgloveParameterHandler {
    fn get(
        &self,
        client: AnyClient,
        names: Vec<String>,
        request_id: Option<String>,
        responder: GetParametersResponder,
    ) {
        // Validated to be Some at registration time (see `FoxgloveParameterHandler::validate`).
        let get = self
            .get
            .expect("foxglove_parameter_handler.get is required");
        let c_request_id = request_id.as_ref().map(FoxgloveString::from);
        let c_names: Vec<_> = names.iter().map(FoxgloveString::from).collect();
        let c_responder = FoxgloveGetParametersResponder(responder).into_raw();
        // SAFETY: The C caller's safety requirements are documented on
        // `FoxgloveParameterHandler::get`.
        unsafe {
            get(
                self.context,
                client.id().into(),
                c_request_id
                    .as_ref()
                    .map(|id| id as *const _)
                    .unwrap_or(std::ptr::null()),
                c_names.as_ptr(),
                c_names.len(),
                c_responder,
            );
        }
    }

    fn set(
        &self,
        client: AnyClient,
        parameters: Vec<Parameter>,
        request_id: Option<String>,
        responder: SetParametersResponder,
    ) {
        // Validated to be Some at registration time (see `FoxgloveParameterHandler::validate`).
        let set = self
            .set
            .expect("foxglove_parameter_handler.set is required");
        let c_request_id = request_id.as_ref().map(FoxgloveString::from);
        let params: FoxgloveParameterArray = parameters.into_iter().collect();
        let c_params = params.into_raw();
        let c_responder = FoxgloveSetParametersResponder(responder).into_raw();
        // SAFETY: The C caller's safety requirements are documented on
        // `FoxgloveParameterHandler::set`.
        unsafe {
            set(
                self.context,
                client.id().into(),
                c_request_id
                    .as_ref()
                    .map(|id| id as *const _)
                    .unwrap_or(std::ptr::null()),
                c_params,
                c_responder,
            );
        }
        // SAFETY: c_params was just produced by FoxgloveParameterArray::into_raw above.
        drop(unsafe { FoxgloveParameterArray::from_raw(c_params) });
    }
}

/// Completes a `getParameters` request by sending parameter values to the client.
///
/// # Safety
/// - `responder` must be a pointer to a `foxglove_get_parameters_responder` obtained via a `get`
///   callback. This value is moved into this function, and must not be accessed afterwards.
/// - `params` must be a valid pointer to a value allocated by `foxglove_parameter_array_create`.
///   This value is moved into this function, and must not be accessed afterwards. A NULL value is
///   treated as an empty array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_get_parameters_responder_respond(
    responder: *mut FoxgloveGetParametersResponder,
    params: *mut FoxgloveParameterArray,
) {
    if responder.is_null() {
        tracing::error!("foxglove_get_parameters_responder_respond called with null responder");
        if !params.is_null() {
            // SAFETY: caller's contract: params allocated by foxglove_parameter_array_create.
            drop(unsafe { FoxgloveParameterArray::from_raw(params) });
        }
        return;
    }
    // SAFETY: responder was produced by FoxgloveGetParametersResponder::into_raw.
    let responder = unsafe { FoxgloveGetParametersResponder::from_raw(responder) };
    let values = if params.is_null() {
        Vec::new()
    } else {
        // SAFETY: caller's contract: params allocated by foxglove_parameter_array_create.
        unsafe { FoxgloveParameterArray::from_raw(params) }.into_native()
    };
    responder.0.respond(values);
}

/// Drops a `getParameters` responder without responding.
///
/// Reserved for unrecoverable internal errors; sends a generic error status to the requesting
/// client. In all other cases, complete the request with
/// `foxglove_get_parameters_responder_respond` (passing an empty array if no values are available).
///
/// # Safety
/// - `responder` must be a pointer to a `foxglove_get_parameters_responder` obtained via a `get`
///   callback. This value is moved into this function, and must not be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_get_parameters_responder_drop(
    responder: *mut FoxgloveGetParametersResponder,
) {
    if responder.is_null() {
        tracing::error!("foxglove_get_parameters_responder_drop called with null responder");
        return;
    }
    // SAFETY: responder was produced by FoxgloveGetParametersResponder::into_raw.
    drop(unsafe { FoxgloveGetParametersResponder::from_raw(responder) });
}

/// Completes a `setParameters` request with the values that were actually applied.
///
/// Echoes those values back to the requesting client when the request carried a `request_id`;
/// otherwise does nothing on the wire. Does not notify other parameter subscribers; the caller is
/// responsible for broadcasting applied updates to subscribers on each sink (for example, via
/// `foxglove_server_publish_parameter_values` and `foxglove_gateway_publish_parameter_values`).
///
/// # Safety
/// - `responder` must be a pointer to a `foxglove_set_parameters_responder` obtained via a `set`
///   callback. This value is moved into this function, and must not be accessed afterwards.
/// - `params` must be a valid pointer to a value allocated by `foxglove_parameter_array_create`.
///   This value is moved into this function, and must not be accessed afterwards. A NULL value is
///   treated as an empty array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_set_parameters_responder_respond(
    responder: *mut FoxgloveSetParametersResponder,
    params: *mut FoxgloveParameterArray,
) {
    if responder.is_null() {
        tracing::error!("foxglove_set_parameters_responder_respond called with null responder");
        if !params.is_null() {
            // SAFETY: caller's contract: params allocated by foxglove_parameter_array_create.
            drop(unsafe { FoxgloveParameterArray::from_raw(params) });
        }
        return;
    }
    // SAFETY: responder was produced by FoxgloveSetParametersResponder::into_raw.
    let responder = unsafe { FoxgloveSetParametersResponder::from_raw(responder) };
    let values = if params.is_null() {
        Vec::new()
    } else {
        // SAFETY: caller's contract: params allocated by foxglove_parameter_array_create.
        unsafe { FoxgloveParameterArray::from_raw(params) }.into_native()
    };
    responder.0.respond(values);
}

/// Drops a `setParameters` responder without responding.
///
/// Reserved for unrecoverable internal errors; sends a generic error status to the requesting
/// client. In all other cases, complete the request with
/// `foxglove_set_parameters_responder_respond` (passing an empty array if the request could not
/// be handled).
///
/// # Safety
/// - `responder` must be a pointer to a `foxglove_set_parameters_responder` obtained via a `set`
///   callback. This value is moved into this function, and must not be accessed afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_set_parameters_responder_drop(
    responder: *mut FoxgloveSetParametersResponder,
) {
    if responder.is_null() {
        tracing::error!("foxglove_set_parameters_responder_drop called with null responder");
        return;
    }
    // SAFETY: responder was produced by FoxgloveSetParametersResponder::into_raw.
    drop(unsafe { FoxgloveSetParametersResponder::from_raw(responder) });
}
