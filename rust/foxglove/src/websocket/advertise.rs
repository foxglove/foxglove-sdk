//! Advertisement helpers

use std::sync::Arc;

use crate::{RawChannel, Schema};

use super::service::Service;
use super::ws_protocol::schema;
use super::ws_protocol::server::{advertise, advertise_services, Advertise, AdvertiseServices};

impl<'a> From<&'a Schema> for schema::Schema<'a> {
    fn from(schema: &'a Schema) -> Self {
        Self::new(&schema.name, &schema.encoding, schema.data.clone())
    }
}

impl<'a> TryFrom<&'a RawChannel> for advertise::Channel<'a> {
    type Error = schema::EncodeError;

    fn try_from(ch: &'a RawChannel) -> Result<Self, Self::Error> {
        let mut builder = Self::builder(ch.id().into(), ch.topic(), ch.message_encoding());
        if let Some(schema) = ch.schema() {
            builder = builder.with_schema(schema.into());
        }
        builder.build()
    }
}

/// Creates a channel advertisement, or logs an error message.
fn maybe_advertise_channel(channel: &Arc<RawChannel>) -> Option<advertise::Channel<'_>> {
    channel
        .as_ref()
        .try_into()
        .inspect_err(|err| match err {
            schema::EncodeError::MissingSchema => {
                tracing::error!(
                    "Ignoring advertise channel for {} because a schema is required",
                    channel.topic()
                );
            }
            err => {
                tracing::error!("Error advertising channel to client: {err}");
            }
        })
        .ok()
}

/// Creates an advertise message for the specified channels.
pub fn advertise_channels<'a>(
    channels: impl IntoIterator<Item = &'a Arc<RawChannel>>,
) -> Advertise<'a> {
    Advertise::new(channels.into_iter().filter_map(maybe_advertise_channel))
}

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
