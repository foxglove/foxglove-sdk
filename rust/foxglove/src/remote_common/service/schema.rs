use crate::Schema;

/// A service request or response schema.
#[derive(Debug, Clone)]
pub(crate) struct MessageSchema {
    pub encoding: String,
    pub schema: Schema,
}

/// A service schema.
#[derive(Debug, Clone)]
pub struct ServiceSchema {
    name: String,
    request: Option<MessageSchema>,
    response: Option<MessageSchema>,
}
impl ServiceSchema {
    /// Creates a new named service schema with an empty request and response.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            request: None,
            response: None,
        }
    }

    /// Adds request schema information.
    #[must_use]
    pub fn with_request(mut self, encoding: impl Into<String>, schema: Schema) -> Self {
        self.request = Some(MessageSchema {
            encoding: encoding.into(),
            schema,
        });
        self
    }

    /// Adds response schema information.
    #[must_use]
    pub fn with_response(mut self, encoding: impl Into<String>, schema: Schema) -> Self {
        self.response = Some(MessageSchema {
            encoding: encoding.into(),
            schema,
        });
        self
    }

    /// Returns the name of the schema.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the request schema.
    pub(crate) fn request(&self) -> Option<&MessageSchema> {
        self.request.as_ref()
    }

    /// Returns the response schema.
    pub(crate) fn response(&self) -> Option<&MessageSchema> {
        self.response.as_ref()
    }
}
