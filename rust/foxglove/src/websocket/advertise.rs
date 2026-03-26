//! Advertisement helpers

use super::service::Service;
use super::ws_protocol::server::{AdvertiseServices, advertise_services};

pub use super::ws_protocol::server::advertise::advertise_channels;

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
