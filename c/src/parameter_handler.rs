use std::ffi::c_void;
use std::sync::Arc;

use foxglove::websocket::{
    AnyClient, GetParametersResponder, Parameter, ParameterHandler, SetParametersResponder,
};

use crate::FoxgloveString;
use crate::parameter::FoxgloveParameterArray;

/// Responder for a `getParameters` request from a client.
///
/// Obtained via the `get` callback of `foxglove_parameter_handler`. The implementation must
/// eventually call either `foxglove_get_parameters_responder_respond` or
/// `foxglove_get_parameters_responder_drop`, exactly once, in order to complete the request. It is
/// safe to invoke these functions synchronously from the context of the callback.
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
/// Obtained via the `set` callback of `foxglove_parameter_handler`. The implementation must
/// eventually call either `foxglove_set_parameters_responder_respond` or
/// `foxglove_set_parameters_responder_drop`, exactly once, in order to complete the request. It is
/// safe to invoke these functions synchronously from the context of the callback.
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
/// `foxglove_server_callbacks` / `foxglove_gateway_callbacks`. The handler also automatically
/// enables the `FOXGLOVE_SERVER_CAPABILITY_PARAMETERS` (or `FOXGLOVE_GATEWAY_CAPABILITY_PARAMETERS`)
/// capability when it is registered, but the caller is still responsible for setting that
/// capability bit if subscribe/unsubscribe notifications are also desired.
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
    /// The `request_id` argument may be NULL.
    ///
    /// The `param_names` argument may be NULL only when `param_names_len` is zero. The buffer is
    /// valid for the duration of this call; if the callback wishes to store these values, it must
    /// copy them out.
    ///
    /// The implementation takes ownership of `responder`, and must eventually complete it by
    /// calling either `foxglove_get_parameters_responder_respond` or
    /// `foxglove_get_parameters_responder_drop`, exactly once. Dropping the responder without
    /// responding sends a generic error status to the requesting client.
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
    /// The `request_id` argument may be NULL.
    ///
    /// The `params` argument is guaranteed to be non-NULL. The buffer is valid for the duration of
    /// this call; if the callback wishes to store these values, it must copy them out.
    ///
    /// The implementation takes ownership of `responder`, and must eventually complete it by
    /// calling either `foxglove_set_parameters_responder_respond` or
    /// `foxglove_set_parameters_responder_drop`, exactly once. The values passed to `respond` are
    /// echoed back to the requester (when `request_id` is non-NULL) and broadcast to subscribers.
    /// Dropping the responder without responding sends a generic error status to the requesting
    /// client and does not broadcast anything.
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
        let Some(get) = self.get else {
            // Dropping the responder sends an error status to the client.
            drop(responder);
            return;
        };
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
        let Some(set) = self.set else {
            drop(responder);
            return;
        };
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
/// This sends a generic error status to the requesting client.
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
/// Echoes to the requester when the request carried a `request_id`, and broadcasts to subscribers.
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
/// This sends a generic error status to the requesting client and does not broadcast anything.
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
