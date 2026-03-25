//! An example to illustrate the use of services with the remote access gateway.
//!
//! This example exposes the following services:
//! - /echo: Echoes the request as the response
//! - /sleep/async: Sleeps for 1 second, and returns an empty response
//! - /sleep/block: Sleeps for 1 second, and returns an empty response
//! - /calc/{add,sub,mul,mod}: Performs simple integer arithmetic
//! - /flag_a: Sets/resets flag A
//! - /flag_b: Sets/resets flag B
//!
//! You can call these services from the Service Call panel in the Foxglove app.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{Context, Result};
use foxglove::Schema;
use foxglove::remote_access::{Capability, Gateway, Request, Service, ServiceSchema, SyncHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    // Start the remote access gateway with service support.
    let handle = Gateway::new()
        .capabilities([Capability::Services])
        .supported_encodings(["json"])
        .start()
        .context("Failed to start remote access gateway")?;

    // Simple services can be implemented with a closure.
    handle
        .add_services([Service::builder("/echo", echo_schema())
            .handler_fn(|req| anyhow::Ok(req.into_payload()))])
        .context("Failed to register services")?;

    // Services that need to do more heavy lifting should be handled asynchronously, either as an
    // async task, or a blocking task.
    handle
        .add_services([
            // Async handlers will be spawned using `tokio::spawn`.
            Service::builder("/sleep/async", empty_schema()).async_handler_fn(sleep_handler),
            // Blocking handlers will be spawned using `tokio::task::spawn_blocking`.
            Service::builder("/sleep/block", empty_schema()).blocking_handler_fn(blocking_handler),
        ])
        .context("Failed to register services")?;

    // A single handler function can be shared by multiple services.
    handle
        .add_services(
            ["/calc/add", "/calc/sub", "/calc/mul", "/calc/mod"]
                .into_iter()
                .map(|name| Service::builder(name, calc_schema()).handler_fn(calc_handler)),
        )
        .context("Failed to register services")?;

    // A stateful handler might be written as a type that implements `Handler` (or `SyncHandler`).
    let flag_a = Flag::default();
    let flag_b = Flag::default();
    handle
        .add_services([
            Service::builder("/flag_a", set_bool_schema()).handler(flag_a.clone()),
            Service::builder("/flag_b", set_bool_schema()).handler(flag_b.clone()),
        ])
        .context("Failed to register services")?;

    tokio::signal::ctrl_c().await?;

    _ = handle.stop().await;
    Ok(())
}

fn empty_schema() -> ServiceSchema {
    // A simple schema with a "well-known" request & response.
    ServiceSchema::new("/std_srvs/Empty")
}

fn echo_schema() -> ServiceSchema {
    // A simple schema with a specified request & response type.
    let any_object = Schema::new("any object", "jsonschema", br#"{"type":"object"}"#);
    ServiceSchema::new("/custom_srvs/Echo")
        .with_request("json", any_object.clone())
        .with_response("json", any_object)
}

async fn sleep_handler(_: Request) -> Result<&'static [u8], String> {
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(b"{}")
}

fn blocking_handler(_: Request) -> Result<&'static [u8], String> {
    std::thread::sleep(Duration::from_secs(1));
    Ok(b"{}")
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CalcRequest {
    pub a: u64,
    pub b: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CalcResponse {
    pub result: u64,
}

fn calc_schema() -> ServiceSchema {
    // Schemas can be derived from types that implement `JsonSchema` using the
    // `Schema::json_schema()` method.
    ServiceSchema::new("/custom_srvs/Calc")
        .with_request("json", Schema::json_schema::<CalcRequest>())
        .with_response("json", Schema::json_schema::<CalcResponse>())
}

/// A stateless handler function.
fn calc_handler(req: Request) -> Result<Vec<u8>> {
    let service_name = req.service_name();
    let req: CalcRequest = serde_json::from_slice(req.payload())?;
    info!("{service_name}: {req:?}");

    // Shared handlers can use `Request::service_name` to disambiguate the service endpoint.
    // Service names are guaranteed to be unique.
    let result = match service_name {
        "/calc/add" => req.a.saturating_add(req.b),
        "/calc/sub" => req.a.saturating_sub(req.b),
        "/calc/mul" => req.a.saturating_mul(req.b),
        "/calc/mod" => req.a.checked_rem(req.b).unwrap_or(0),
        m => return Err(anyhow::anyhow!("unexpected service: {m}")),
    };

    let payload = serde_json::to_vec(&CalcResponse { result })?;
    Ok(payload)
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SetBoolRequest {
    pub data: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct SetBoolResponse {
    pub success: bool,
    pub message: String,
}

fn set_bool_schema() -> ServiceSchema {
    ServiceSchema::new("/std_srvs/SetBool")
        .with_request("json", Schema::json_schema::<SetBoolRequest>())
        .with_response("json", Schema::json_schema::<SetBoolResponse>())
}

/// A stateful handler implements the `SyncHandler` trait.
#[derive(Debug, Default, Clone)]
struct Flag(Arc<AtomicBool>);

impl SyncHandler for Flag {
    type Error = anyhow::Error;
    type Response = Vec<u8>;

    fn call(&self, req: Request) -> Result<Self::Response, Self::Error> {
        // Decode the payload.
        let req: SetBoolRequest = serde_json::from_slice(req.payload())?;
        info!("{req:?}");

        // Update the flag.
        let prev = self.0.swap(req.data, std::sync::atomic::Ordering::Relaxed);

        // Encode the response.
        let message = if prev == req.data {
            "unchanged".to_string()
        } else {
            format!("updated {prev} -> {}", req.data)
        };
        let payload = serde_json::to_vec(&SetBoolResponse {
            success: true,
            message,
        })?;
        Ok(payload)
    }
}
