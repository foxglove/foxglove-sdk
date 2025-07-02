use std::ffi::c_void;

use foxglove::websocket::service::{Handler, Request, Responder, Service, ServiceSchema};

use crate::bytes::FoxgloveBytes;
use crate::{FoxgloveError, FoxgloveSchema, FoxgloveString};

pub struct FoxgloveServiceResponder(Responder);
impl FoxgloveServiceResponder {
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
    /// - The raw pointer must have been obtained from [`Self::into_raw`].
    unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }
}

/// A websocket service request message.
#[repr(C)]
pub struct FoxgloveServiceRequest {
    /// The service name.
    pub service_name: FoxgloveString,
    /// The client ID.
    pub client_id: u32,
    /// The call ID that uniquely identifies this request for this client.
    pub call_id: u32,
    /// The request encoding.
    pub encoding: FoxgloveString,
    /// The request payload.
    pub payload: FoxgloveBytes,
}
impl From<&Request> for FoxgloveServiceRequest {
    fn from(req: &Request) -> Self {
        Self {
            service_name: req.service_name().into(),
            client_id: req.client_id().into(),
            call_id: req.call_id().into(),
            encoding: req.encoding().into(),
            payload: req.payload().into(),
        }
    }
}

pub struct FoxgloveService(Service);
impl FoxgloveService {
    /// Moves the service handle to the heap and returns a pointer.
    ///
    /// After calling this function, the caller is responsible for eventually calling
    /// [`Self::from_raw`] to recover the service handle.
    fn into_raw(self) -> *mut Self {
        Box::into_raw(Box::new(self))
    }

    /// Recovers the boxed service handle from the heap.
    ///
    /// # Safety
    /// - The raw pointer must have been obtained from [`Self::into_raw`].
    pub unsafe fn from_raw(ptr: *mut Self) -> Box<Self> {
        unsafe { Box::from_raw(ptr) }
    }

    /// Returns the inner service handle.
    pub fn into_inner(self) -> Service {
        self.0
    }
}

/// A schema describing either a websocket service request or response.
#[repr(C)]
pub struct FoxgloveServiceMessageSchema {
    /// The message encoding.
    pub encoding: FoxgloveString,
    /// The message schema.
    pub schema: FoxgloveSchema,
}
impl FoxgloveServiceMessageSchema {
    /// Converts a service message schema to native types.
    ///
    /// # Safety
    /// - `encoding` must be a valid pointer to a UTF-8 string.
    /// - `schema` must meet the safety requirements of [`FoxgloveSchema::to_native`].
    unsafe fn to_native(&self) -> Result<(String, foxglove::Schema), foxglove::FoxgloveError> {
        let encoding = unsafe { self.encoding.as_utf8_str() }?;
        let schema = unsafe { self.schema.to_native() }?;
        Ok((encoding.to_string(), schema))
    }
}

/// A websocket service schema.
#[repr(C)]
pub struct FoxgloveServiceSchema<'a> {
    /// Service schema name.
    pub name: FoxgloveString,
    /// Optional request message schema.
    pub request: Option<&'a FoxgloveServiceMessageSchema>,
    /// Optional response message schema.
    pub response: Option<&'a FoxgloveServiceMessageSchema>,
}
impl FoxgloveServiceSchema<'_> {
    /// Converts a service schema to the native type.
    ///
    /// # Safety
    /// - `name` must be a valid pointer to a UTF-8 string.
    /// - `request` and `response` must each be either NULL , or a pointer to a struct that meets
    ///   the safety requirements of [`FoxgloveServiceMessageSchema::to_native`].
    unsafe fn to_native(&self) -> Result<ServiceSchema, foxglove::FoxgloveError> {
        let name = unsafe { self.name.as_utf8_str() }?;
        let mut schema = ServiceSchema::new(name);
        if let Some(request) = self.request {
            let (encoding, request_schema) = unsafe { request.to_native() }?;
            schema = schema.with_request(encoding, request_schema);
        }
        if let Some(response) = self.response {
            let (encoding, response_schema) = unsafe { response.to_native() }?;
            schema = schema.with_response(encoding, response_schema);
        }
        Ok(schema)
    }
}

/// Internal type for implementing the service handler trait.
#[derive(Clone)]
struct ServiceHandler {
    callback_context: *const c_void,
    callback: unsafe extern "C" fn(
        *const c_void,
        *const FoxgloveServiceRequest,
        *mut FoxgloveServiceResponder,
    ),
}
unsafe impl Send for ServiceHandler {}
unsafe impl Sync for ServiceHandler {}
impl Handler for ServiceHandler {
    fn call(&self, request: Request, responder: Responder) {
        let c_request = FoxgloveServiceRequest::from(&request);
        let c_responder = FoxgloveServiceResponder(responder).into_raw();
        // SAFETY: It's the callback implementation's responsibility to ensure that this callback
        // function pointer remains valid for the lifetime of the service, as described in the
        // safety requirements of `foxglove_service_create`.
        unsafe { (self.callback)(self.callback_context, &raw const c_request, c_responder) };
    }
}

/// Creates a new websocket service.
///
/// The service must be registered with a websocket server using `foxglove_server_add_service`, or
/// freed with `foxglove_service_free`.
///
/// The callback is invoked from the client's main poll loop and must not block. If blocking or
/// long-running behavior is required, the implementation should return immediately and handle the
/// request asynchronously.
///
/// The `request` structure provided to the callback is only valid for the duration of the
/// callback. If the implementation wishes to retain its data for a longer lifetime, it must copy
/// data out of it.
///
/// The `responder` provided to the callback represents an unfulfilled response. The implementation
/// must eventually call either `foxglove_service_respond_ok` or `foxglove_service_respond_error`,
/// exactly once, in order to complete the request. It is safe to invoke these completion functions
/// synchronously from the context of the callback.
///
/// # Safety
/// - `service` must be a valid pointer.
/// - `name` must be a valid pointer to a UTF-8 string.
/// - `schema` must be NULL, or a valid pointer to a service schema.
/// - `callback` must be a valid pointer to a service callback function, which must remain valid
///   until the service is either unregistered or freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_service_create(
    service: *mut *mut FoxgloveService,
    name: FoxgloveString,
    schema: Option<&FoxgloveServiceSchema>,
    context: *const c_void,
    callback: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            request: *const FoxgloveServiceRequest,
            responder: *mut FoxgloveServiceResponder,
        ),
    >,
) -> FoxgloveError {
    if service.is_null() {
        return FoxgloveError::ValueError;
    }
    let name = unsafe { name.as_utf8_str() };
    let Ok(name) = name else {
        return FoxgloveError::Utf8Error;
    };
    let Some(schema) = schema else {
        return FoxgloveError::ValueError;
    };
    let schema = match unsafe { schema.to_native() } {
        Ok(schema) => schema,
        Err(e) => return FoxgloveError::from(e),
    };
    let Some(callback) = callback else {
        return FoxgloveError::ValueError;
    };
    let handler = ServiceHandler {
        callback_context: context,
        callback,
    };
    let inner = Service::builder(name, schema).handler(handler);
    let ptr = FoxgloveService(inner).into_raw();
    unsafe { *service = ptr };
    FoxgloveError::Ok
}

/// Frees a service that was never registered to a websocket server.
///
/// # Safety
/// - `service` must be a valid pointer to a service allocated by `foxglove_service_create`. The
///   service MUST NOT have been previously registered with a websocket server.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_service_free(service: *mut FoxgloveService) {
    if !service.is_null() {
        drop(unsafe { FoxgloveService::from_raw(service) });
    }
}

/// Overrides the default response encoding.
///
/// # Safety
/// - `responder` must be a pointer to a `foxglove_service_responder` obtained via the
///   `foxglove_service.handler` callback.
/// - `encoding` must be a pointer to a valid UTF-8 string. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_service_set_response_encoding(
    responder: Option<&mut FoxgloveServiceResponder>,
    encoding: FoxgloveString,
) -> FoxgloveError {
    let Some(responder) = responder else {
        return FoxgloveError::ValueError;
    };
    let encoding = unsafe { encoding.as_utf8_str() };
    let Ok(encoding) = encoding else {
        return FoxgloveError::Utf8Error;
    };
    responder.0.set_encoding(encoding);
    FoxgloveError::Ok
}

/// Completes a request by sending response data to the client.
///
/// # Safety
/// - `responder` must be a pointer to a `foxglove_service_responder` obtained via the
///   `foxglove_service.handler` callback. This value is moved into this function, and must not
///   accessed afterwards.
/// - `data` must be a pointer to the response data. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_service_respond_ok(
    responder: *mut FoxgloveServiceResponder,
    data: FoxgloveBytes,
) {
    let responder = unsafe { FoxgloveServiceResponder::from_raw(responder) };
    let data = unsafe { data.as_slice() };
    responder.0.respond_ok(data);
}

/// Completes a request by sending an error message to the client.
///
/// # Safety
/// - `responder` must be a pointer to a `foxglove_service_responder` obtained via the
///   `foxglove_service.handler` callback. This value is moved into this function, and must not
///   accessed afterwards.
/// - `message` must be a pointer to a valid UTF-8 string. This value is copied by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_service_respond_error(
    responder: *mut FoxgloveServiceResponder,
    message: FoxgloveString,
) {
    let responder = unsafe { FoxgloveServiceResponder::from_raw(responder) };
    let message = unsafe { message.as_utf8_str() };
    let message = match message {
        Ok(s) => s.to_string(),
        Err(e) => format!("Server produced an invalid error message: {e}"),
    };
    responder.0.respond_err(message);
}
