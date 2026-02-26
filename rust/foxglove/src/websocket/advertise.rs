//! Advertisement helpers

use super::service::Service;
use super::ws_protocol::schema;
use super::ws_protocol::server::{AdvertiseServices, advertise_services};

pub use super::ws_protocol::server::advertise::advertise_channels;

impl<'a> TryFrom<&'a Service> for advertise_services::Service<'a> {
    type Error = schema::EncodeError;

    fn try_from(s: &'a Service) -> Result<Self, Self::Error> {
        let schema = s.schema();
        let mut service = Self::new(s.id().into(), s.name(), schema.name());
        if let Some(request) = schema.request() {
            service = service.with_request(&request.encoding, (&request.schema).into())?;
        }
        if let Some(response) = schema.response() {
            service = service.with_response(&response.encoding, (&response.schema).into())?;
        }
        Ok(service)
    }
}

/// Constructs a service advertisement, or logs an error message.
pub fn maybe_advertise_service(service: &Service) -> Option<advertise_services::Service<'_>> {
    service
        .try_into()
        .inspect_err(|err| {
            tracing::error!(
                "Failed to encode service advertisement for {}: {err}",
                service.name()
            )
        })
        .ok()
}

/// Creates an advertise services message for the specified services.
pub fn advertise_services<'a>(
    services: impl IntoIterator<Item = &'a Service>,
) -> AdvertiseServices<'a> {
    AdvertiseServices::new(services.into_iter().filter_map(maybe_advertise_service))
}
