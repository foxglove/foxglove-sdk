use crate::PySchema;
use base64::prelude::*;
use foxglove::websocket::{AssetHandler, StatusLevel};
use pyo3::IntoPyObjectExt;
use pyo3::exceptions::{PyIOError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::collections::HashMap;
use std::sync::Arc;

/// A handler for services which calls out to user-defined Python functions.
pub struct ServiceHandler {
    pub handler: Arc<Py<PyAny>>,
}

impl foxglove::websocket::service::Handler for ServiceHandler {
    fn call(
        &self,
        request: foxglove::websocket::service::Request,
        responder: foxglove::websocket::service::Responder,
    ) {
        let handler = self.handler.clone();
        let request = PyServiceRequest(request);
        // Punt the callback to a blocking thread.
        tokio::task::spawn_blocking(move || {
            let result = Python::with_gil(|py| {
                handler
                    .bind(py)
                    .call((request,), None)
                    .and_then(|data| data.extract::<Vec<u8>>())
            });
            responder.respond(result);
        });
    }
}

/// A service.
///
/// The handler must be a callback function which takes the :py:class:`foxglove.ServiceRequest` as its
/// argument, and returns `bytes` as a response. If the handler raises an exception, the stringified
/// exception message will be returned to the client as an error.
#[pyclass(name = "Service", module = "foxglove", get_all, set_all)]
#[derive(FromPyObject)]
pub struct PyService {
    name: String,
    schema: PyServiceSchema,
    handler: Py<PyAny>,
}

#[pymethods]
impl PyService {
    /// Create a new service.
    #[new]
    #[pyo3(signature = (name, *, schema, handler))]
    fn new(name: &str, schema: PyServiceSchema, handler: Py<PyAny>) -> Self {
        PyService {
            name: name.to_string(),
            schema,
            handler,
        }
    }
}

impl From<PyService> for foxglove::websocket::service::Service {
    fn from(value: PyService) -> Self {
        foxglove::websocket::service::Service::builder(value.name, value.schema.into()).handler(
            ServiceHandler {
                handler: Arc::new(value.handler),
            },
        )
    }
}

/// A service request.
#[pyclass(name = "ServiceRequest", module = "foxglove")]
pub struct PyServiceRequest(foxglove::websocket::service::Request);

#[pymethods]
impl PyServiceRequest {
    /// The service name.
    #[getter]
    fn service_name(&self) -> &str {
        self.0.service_name()
    }

    /// The client ID.
    #[getter]
    fn client_id(&self) -> u32 {
        self.0.client_id().into()
    }

    /// The call ID that uniquely identifies this request for this client.
    #[getter]
    fn call_id(&self) -> u32 {
        self.0.call_id().into()
    }

    /// The request encoding.
    #[getter]
    fn encoding(&self) -> &str {
        self.0.encoding()
    }

    /// The request payload.
    #[getter]
    fn payload(&self) -> &[u8] {
        self.0.payload()
    }
}

/// A service schema.
///
/// :param name: The name of the service.
/// :type name: str
/// :param request: The request schema.
/// :type request: :py:class:`foxglove.MessageSchema` | `None`
/// :param response: The response schema.
/// :type response: :py:class:`foxglove.MessageSchema` | `None`
#[pyclass(name = "ServiceSchema", module = "foxglove", get_all, set_all)]
#[derive(Clone)]
pub struct PyServiceSchema {
    /// The name of the service.
    name: String,
    /// The request schema.
    request: Option<PyMessageSchema>,
    /// The response schema.
    response: Option<PyMessageSchema>,
}

#[pymethods]
impl PyServiceSchema {
    #[new]
    #[pyo3(signature = (name, *, request=None, response=None))]
    fn new(
        name: &str,
        request: Option<&PyMessageSchema>,
        response: Option<&PyMessageSchema>,
    ) -> Self {
        PyServiceSchema {
            name: name.to_string(),
            request: request.cloned(),
            response: response.cloned(),
        }
    }
}

impl From<PyServiceSchema> for foxglove::websocket::service::ServiceSchema {
    fn from(value: PyServiceSchema) -> Self {
        let mut schema = foxglove::websocket::service::ServiceSchema::new(value.name);
        if let Some(request) = value.request {
            schema = schema.with_request(request.encoding, request.schema.into());
        }
        if let Some(response) = value.response {
            schema = schema.with_response(response.encoding, response.schema.into());
        }
        schema
    }
}

/// A service request or response schema.
///
/// :param encoding: The encoding of the message.
/// :type encoding: str
/// :param schema: The message schema.
/// :type schema: :py:class:`foxglove.Schema`
#[pyclass(name = "MessageSchema", module = "foxglove", get_all, set_all)]
#[derive(Clone)]
pub struct PyMessageSchema {
    /// The encoding of the message.
    encoding: String,
    /// The message schema.
    schema: PySchema,
}

#[pymethods]
impl PyMessageSchema {
    #[new]
    #[pyo3(signature = (*, encoding, schema))]
    fn new(encoding: &str, schema: PySchema) -> Self {
        PyMessageSchema {
            encoding: encoding.to_string(),
            schema,
        }
    }
}

/// A parameter type.
#[pyclass(name = "ParameterType", module = "foxglove", eq, eq_int)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PyParameterType {
    /// A byte array.
    ByteArray,
    /// A floating-point value that can be represented as a `float64`.
    Float64,
    /// An array of floating-point values that can be represented as `float64`s.
    Float64Array,
}

impl From<PyParameterType> for foxglove::websocket::ParameterType {
    fn from(value: PyParameterType) -> Self {
        match value {
            PyParameterType::ByteArray => foxglove::websocket::ParameterType::ByteArray,
            PyParameterType::Float64 => foxglove::websocket::ParameterType::Float64,
            PyParameterType::Float64Array => foxglove::websocket::ParameterType::Float64Array,
        }
    }
}

impl From<foxglove::websocket::ParameterType> for PyParameterType {
    fn from(value: foxglove::websocket::ParameterType) -> Self {
        match value {
            foxglove::websocket::ParameterType::ByteArray => PyParameterType::ByteArray,
            foxglove::websocket::ParameterType::Float64 => PyParameterType::Float64,
            foxglove::websocket::ParameterType::Float64Array => PyParameterType::Float64Array,
        }
    }
}

/// A parameter value.
#[pyclass(name = "ParameterValue", module = "foxglove", eq)]
#[derive(Clone, PartialEq)]
pub enum PyParameterValue {
    /// An integer value.
    Integer(i64),
    /// A floating-point value.
    Float64(f64),
    /// A boolean value.
    Bool(bool),
    /// A string value.
    ///
    /// For parameters of type ByteArray, this is a base64-encoding of the byte array.
    String(String),
    /// An array of parameter values.
    Array(Vec<PyParameterValue>),
    /// An associative map of parameter values.
    Dict(HashMap<String, PyParameterValue>),
}

impl From<PyParameterValue> for foxglove::websocket::ParameterValue {
    fn from(value: PyParameterValue) -> Self {
        match value {
            PyParameterValue::Integer(i) => foxglove::websocket::ParameterValue::Integer(i),
            PyParameterValue::Float64(n) => foxglove::websocket::ParameterValue::Float64(n),
            PyParameterValue::Bool(b) => foxglove::websocket::ParameterValue::Bool(b),
            PyParameterValue::String(s) => foxglove::websocket::ParameterValue::String(s),
            PyParameterValue::Array(py_parameter_values) => {
                foxglove::websocket::ParameterValue::Array(
                    py_parameter_values.into_iter().map(Into::into).collect(),
                )
            }
            PyParameterValue::Dict(hash_map) => foxglove::websocket::ParameterValue::Dict(
                hash_map.into_iter().map(|(k, v)| (k, v.into())).collect(),
            ),
        }
    }
}

impl From<foxglove::websocket::ParameterValue> for PyParameterValue {
    fn from(value: foxglove::websocket::ParameterValue) -> Self {
        match value {
            foxglove::websocket::ParameterValue::Integer(n) => PyParameterValue::Integer(n),
            foxglove::websocket::ParameterValue::Float64(n) => PyParameterValue::Float64(n),
            foxglove::websocket::ParameterValue::Bool(b) => PyParameterValue::Bool(b),
            foxglove::websocket::ParameterValue::String(s) => PyParameterValue::String(s),
            foxglove::websocket::ParameterValue::Array(parameter_values) => {
                PyParameterValue::Array(parameter_values.into_iter().map(Into::into).collect())
            }
            foxglove::websocket::ParameterValue::Dict(hash_map) => {
                PyParameterValue::Dict(hash_map.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
        }
    }
}

/// A parameter which can be sent to a client.
///
/// :param name: The parameter name.
/// :type name: str
/// :param value: Optional value, represented as a native python object, or a ParameterValue.
/// :type value: None|bool|float|str|bytes|list|dict|ParameterValue
/// :param type: Optional parameter type. This is automatically derived when passing a native
///              python object as the value.
/// :type type: ParameterType|None
#[pyclass(name = "Parameter", module = "foxglove")]
#[derive(Clone)]
pub struct PyParameter {
    /// The name of the parameter.
    #[pyo3(get)]
    pub name: String,
    /// The parameter type.
    #[pyo3(get)]
    pub r#type: Option<PyParameterType>,
    /// The parameter value.
    #[pyo3(get)]
    pub value: Option<PyParameterValue>,
}

#[pymethods]
impl PyParameter {
    #[new]
    #[pyo3(signature = (name, *, value=None, **kwargs))]
    pub fn new(
        name: String,
        value: Option<ParameterTypeValueConverter>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        // Use the derived type, unless there's a kwarg override.
        let mut r#type = value.as_ref().and_then(|tv| tv.0);
        if let Some(dict) = kwargs
            && let Some(kw_type) = dict.get_item("type")?
        {
            if kw_type.is_none() {
                r#type = None
            } else {
                r#type = kw_type.extract()?;
            }
        }
        Ok(Self {
            name,
            r#type,
            value: value.map(|tv| tv.1),
        })
    }

    /// Returns the parameter value as a native python object.
    ///
    /// :rtype: None|bool|float|str|bytes|list|dict
    pub fn get_value(&self) -> Option<ParameterTypeValueConverter> {
        self.value
            .clone()
            .map(|v| ParameterTypeValueConverter(self.r#type, v))
    }
}

impl From<PyParameter> for foxglove::websocket::Parameter {
    fn from(value: PyParameter) -> Self {
        Self {
            name: value.name,
            r#type: value.r#type.map(Into::into),
            value: value.value.map(Into::into),
        }
    }
}

impl From<foxglove::websocket::Parameter> for PyParameter {
    fn from(value: foxglove::websocket::Parameter) -> Self {
        Self {
            name: value.name,
            r#type: value.r#type.map(Into::into),
            value: value.value.map(Into::into),
        }
    }
}

/// A shim type for converting between PyParameterValue and native python types.
///
/// Note that we can't implement this on PyParameterValue directly, because it has its own
/// implementation by virtue of being exposed as a `#[pyclass]` enum.
pub struct ParameterValueConverter(PyParameterValue);

impl<'py> IntoPyObject<'py> for ParameterValueConverter {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self.0 {
            PyParameterValue::Integer(v) => v.into_bound_py_any(py),
            PyParameterValue::Float64(v) => v.into_bound_py_any(py),
            PyParameterValue::Bool(v) => v.into_bound_py_any(py),
            PyParameterValue::String(v) => v.into_bound_py_any(py),
            PyParameterValue::Array(values) => {
                let elems = values.into_iter().map(ParameterValueConverter);
                PyList::new(py, elems)?.into_bound_py_any(py)
            }
            PyParameterValue::Dict(values) => {
                let dict = PyDict::new(py);
                for (k, v) in values {
                    dict.set_item(k, ParameterValueConverter(v))?;
                }
                dict.into_bound_py_any(py)
            }
        }
    }
}

impl<'py> FromPyObject<'py> for ParameterValueConverter {
    fn extract_bound(obj: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(val) = obj.extract::<PyParameterValue>() {
            Ok(Self(val))
        } else if let Ok(val) = obj.extract::<bool>() {
            Ok(Self(PyParameterValue::Bool(val)))
        } else if let Ok(val) = obj.extract::<i64>() {
            Ok(Self(PyParameterValue::Integer(val)))
        } else if let Ok(val) = obj.extract::<f64>() {
            Ok(Self(PyParameterValue::Float64(val)))
        } else if let Ok(val) = obj.extract::<String>() {
            Ok(Self(PyParameterValue::String(val)))
        } else if let Ok(list) = obj.downcast::<PyList>() {
            let mut values = Vec::with_capacity(list.len());
            for item in list.iter() {
                let value: ParameterValueConverter = item.extract()?;
                values.push(value.0);
            }
            Ok(Self(PyParameterValue::Array(values)))
        } else if let Ok(dict) = obj.downcast::<PyDict>() {
            let mut values = HashMap::new();
            for (key, value) in dict {
                let key: String = key.extract()?;
                let value: ParameterValueConverter = value.extract()?;
                values.insert(key, value.0);
            }
            Ok(Self(PyParameterValue::Dict(values)))
        } else {
            Err(PyErr::new::<PyTypeError, _>(format!(
                "Unsupported type for ParameterValue: {}",
                obj.get_type().name()?
            )))
        }
    }
}

/// A shim type for converting between (PyParameterType, PyParameterValue) and native python types.
pub struct ParameterTypeValueConverter(Option<PyParameterType>, PyParameterValue);

impl<'py> IntoPyObject<'py> for ParameterTypeValueConverter {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match (self.0, self.1) {
            (Some(PyParameterType::ByteArray), PyParameterValue::String(v)) => {
                let data = BASE64_STANDARD
                    .decode(v)
                    .map_err(|e| PyValueError::new_err(format!("Failed to decode base64: {e}")))?;
                PyBytes::new(py, &data).into_bound_py_any(py)
            }
            (_, v) => ParameterValueConverter(v).into_bound_py_any(py),
        }
    }
}

impl<'py> FromPyObject<'py> for ParameterTypeValueConverter {
    fn extract_bound(obj: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(val) = obj.extract::<ParameterValueConverter>() {
            let val = val.0;
            let (typ, val) = match val {
                // If the value is a float, the type is float64.
                PyParameterValue::Float64(_) => (Some(PyParameterType::Float64), val),
                // If the value is an array of numbers, then the type is float64 array.
                PyParameterValue::Array(ref vec)
                    if vec
                        .iter()
                        .all(|v| matches!(v, PyParameterValue::Float64(_))) =>
                {
                    (Some(PyParameterType::Float64Array), val)
                }
                _ => (None, val),
            };
            Ok(Self(typ, val))
        } else if let Ok(val) = obj.extract::<Vec<u8>>() {
            let b64 = BASE64_STANDARD.encode(val);
            Ok(Self(
                Some(PyParameterType::ByteArray),
                PyParameterValue::String(b64),
            ))
        } else {
            Err(PyErr::new::<PyTypeError, _>(format!(
                "Unsupported type for ParameterValue: {}",
                obj.get_type().name()?
            )))
        }
    }
}

/// A connection graph.
#[pyclass(name = "ConnectionGraph", module = "foxglove")]
#[derive(Clone)]
pub struct PyConnectionGraph(foxglove::websocket::ConnectionGraph);

#[pymethods]
impl PyConnectionGraph {
    /// Create a new connection graph.
    #[new]
    fn default() -> Self {
        Self(foxglove::websocket::ConnectionGraph::new())
    }

    /// Set a published topic and its associated publisher ids.
    /// Overwrites any existing topic with the same name.
    ///
    /// :param topic: The topic name.
    /// :param publisher_ids: The set of publisher ids.
    pub fn set_published_topic(&mut self, topic: &str, publisher_ids: Vec<String>) {
        self.0.set_published_topic(topic, publisher_ids);
    }

    /// Set a subscribed topic and its associated subscriber ids.
    /// Overwrites any existing topic with the same name.
    ///
    /// :param topic: The topic name.
    /// :param subscriber_ids: The set of subscriber ids.
    pub fn set_subscribed_topic(&mut self, topic: &str, subscriber_ids: Vec<String>) {
        self.0.set_subscribed_topic(topic, subscriber_ids);
    }

    /// Set an advertised service and its associated provider ids.
    /// Overwrites any existing service with the same name.
    ///
    /// :param service: The service name.
    /// :param provider_ids: The set of provider ids.
    pub fn set_advertised_service(&mut self, service: &str, provider_ids: Vec<String>) {
        self.0.set_advertised_service(service, provider_ids);
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

impl From<PyConnectionGraph> for foxglove::websocket::ConnectionGraph {
    fn from(value: PyConnectionGraph) -> Self {
        value.0
    }
}

/// An asset handler that calls out to a user-defined Python function.
pub struct CallbackAssetHandler {
    pub handler: Arc<Py<PyAny>>,
}

impl AssetHandler for CallbackAssetHandler {
    fn fetch(&self, uri: String, responder: foxglove::websocket::AssetResponder) {
        let handler = self.handler.clone();

        tokio::task::spawn_blocking(move || {
            let result = Python::with_gil(|py| {
                handler.bind(py).call((uri,), None).and_then(|data| {
                    if data.is_none() {
                        Err(PyIOError::new_err("not found"))
                    } else {
                        data.extract::<Vec<u8>>()
                    }
                })
            });
            responder.respond(result);
        });
    }
}

/// A status message severity level.
#[pyclass(name = "StatusLevel", module = "foxglove", eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyStatusLevel {
    Info,
    Warning,
    Error,
}

impl From<PyStatusLevel> for StatusLevel {
    fn from(value: PyStatusLevel) -> Self {
        match value {
            PyStatusLevel::Info => StatusLevel::Info,
            PyStatusLevel::Warning => StatusLevel::Warning,
            PyStatusLevel::Error => StatusLevel::Error,
        }
    }
}
