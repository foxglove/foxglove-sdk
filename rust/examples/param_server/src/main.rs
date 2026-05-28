//! Example of a parameter server using the Foxglove SDK.
//!
//! The handler implements [`ParameterHandler`] by enqueueing each get/set request on an mpsc
//! channel and returning immediately. A single worker task drains the channel, mutates a local
//! parameter store, and fulfils each responder. Because [`SetParametersResponder`] only echoes
//! the applied values to the requester, the worker is also responsible for publishing those
//! updates to other parameter subscribers; the same path is used to publish a periodic
//! "elapsed" tick. The parameter store has exactly one owner, so no synchronization is required.
//!
//! This is the recommended shape for handlers that need to perform non-trivial work to compute a
//! response: it keeps the SDK's internal threads unblocked.
//!
//! With the `remote-access` feature enabled, the example additionally spawns a remote-access
//! gateway and registers the same handler with it, so the parameter store is shared between
//! WebSocket clients and remote-access participants. The gateway reads `FOXGLOVE_DEVICE_TOKEN`
//! (and optionally `FOXGLOVE_API_URL` / `FOXGLOVE_API_TIMEOUT`) from the environment.
//!
//! Usage:
//! ```text
//! cargo run -p example_param_server
//! cargo run -p example_param_server --features remote-access
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
#[cfg(feature = "remote-access")]
use foxglove::remote_access::{Gateway, GatewayHandle};
use foxglove::remote_common::{
    AnyClient, GetParametersResponder, Parameter, ParameterHandler, ParameterType, ParameterValue,
    SetParametersResponder,
};
use foxglove::{WebSocketServer, WebSocketServerHandle};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(short, long, default_value_t = 8765)]
    port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
}

const QUEUE_CAPACITY: usize = 32;

/// Work item handed from the [`ParameterHandler`] callback to the worker task.
enum ParameterOp {
    Get {
        names: Vec<String>,
        responder: GetParametersResponder,
    },
    Set {
        parameters: Vec<Parameter>,
        responder: SetParametersResponder,
    },
}

/// Handler registered with the SDK to handle parameter get/set operations asynchronously.
struct ParamHandler {
    tx: mpsc::Sender<ParameterOp>,
}

impl ParameterHandler for ParamHandler {
    fn get(
        &self,
        _client: AnyClient,
        names: Vec<String>,
        _request_id: Option<String>,
        responder: GetParametersResponder,
    ) {
        // A real implementation might handle overflow by sending a specific error status to the
        // client. This implementation simply drops the responder, which sends a generic error
        // status to the client about how the server failed to send a response.
        let _ = self.tx.try_send(ParameterOp::Get { names, responder });
    }

    fn set(
        &self,
        _client: AnyClient,
        parameters: Vec<Parameter>,
        _request_id: Option<String>,
        responder: SetParametersResponder,
    ) {
        let _ = self.tx.try_send(ParameterOp::Set {
            parameters,
            responder,
        });
    }
}

/// Owns the parameter store. Drains parameter ops one at a time, broadcasting any applied
/// set-updates to subscribers, and on a separate tick updates the "elapsed" parameter and
/// broadcasts it as well. Shutdown is signalled via a `CancellationToken`; after the loop
/// exits, the worker stops the server (and the gateway, if configured).
struct ParamWorker {
    store: HashMap<String, Parameter>,
    rx: mpsc::Receiver<ParameterOp>,
    server: WebSocketServerHandle,
    #[cfg(feature = "remote-access")]
    gateway: Option<GatewayHandle>,
    shutdown: CancellationToken,
}

impl ParamWorker {
    async fn run(mut self) {
        let start = Instant::now();
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                () = self.shutdown.cancelled() => break,
                op = self.rx.recv() => match op {
                    Some(op) => self.handle_op(op),
                    None => break,
                },
                _ = tick.tick() => self.update_and_publish_elapsed(start),
            }
        }
        #[cfg(feature = "remote-access")]
        if let Some(gateway) = self.gateway.take() {
            let _ = gateway.stop().await;
        }
        self.server.stop().wait().await;
    }

    fn handle_op(&mut self, op: ParameterOp) {
        match op {
            ParameterOp::Get { names, responder } => self.handle_get(names, responder),
            ParameterOp::Set {
                parameters,
                responder,
            } => self.handle_set(parameters, responder),
        }
    }

    fn handle_get(&self, names: Vec<String>, responder: GetParametersResponder) {
        log::info!("get: {names:?}");
        let values = if names.is_empty() {
            self.store.values().cloned().collect()
        } else {
            names
                .iter()
                .filter_map(|name| self.store.get(name).cloned())
                .collect()
        };
        responder.respond(values);
    }

    fn handle_set(&mut self, mut parameters: Vec<Parameter>, responder: SetParametersResponder) {
        let names: Vec<&str> = parameters.iter().map(|p| p.name.as_str()).collect();
        log::info!("set: {names:?}");
        let mut applied = Vec::with_capacity(parameters.len());
        for param in &mut parameters {
            if let Some(existing) = self.store.get_mut(&param.name) {
                if param.name.starts_with("read_only_") {
                    // Send a warning, and echo back the existing value so the client sees no change.
                    responder
                        .client()
                        .send_warning(format!("parameter {} is read only", param.name));
                    param.value.clone_from(&existing.value);
                    param.r#type.clone_from(&existing.r#type);
                    continue;
                }
                existing.value.clone_from(&param.value);
                existing.r#type.clone_from(&param.r#type);
            } else {
                self.store.insert(param.name.clone(), param.clone());
            }
            applied.push(param.clone());
        }
        responder.respond(parameters);
        // SetParametersResponder echoes only to the requester, so the worker must publish the
        // applied updates to subscribers itself.
        if !applied.is_empty() {
            self.publish(applied);
        }
    }

    fn update_and_publish_elapsed(&mut self, start: Instant) {
        let elapsed = Parameter {
            name: "elapsed".to_string(),
            value: Some(ParameterValue::Float64(start.elapsed().as_secs_f64())),
            r#type: Some(ParameterType::Float64),
        };
        self.store.insert(elapsed.name.clone(), elapsed.clone());
        self.publish(vec![elapsed]);
    }

    fn publish(&self, parameters: Vec<Parameter>) {
        #[cfg(feature = "remote-access")]
        if let Some(gateway) = &self.gateway {
            gateway.publish_parameter_values(parameters.clone());
        }
        self.server.publish_parameter_values(parameters);
    }
}

#[tokio::main]
async fn main() {
    let env =
        env_logger::Env::default().default_filter_or("example_param_server=info,foxglove=info");
    env_logger::init_from_env(env);

    let args = Cli::parse();

    let initial_store: HashMap<String, Parameter> = [
        Parameter::string("read_only_str_param", "can't change me"),
        Parameter::float64("elapsed", 0.0),
        Parameter::float64_array("float_array_param", [1.0, 2.0, 3.0]),
    ]
    .into_iter()
    .map(|p| (p.name.clone(), p))
    .collect();

    let (tx, rx) = mpsc::channel(QUEUE_CAPACITY);
    let handler = Arc::new(ParamHandler { tx });

    // `parameter_handler` automatically enables Capability::Parameters.
    let server = WebSocketServer::new()
        .name(env!("CARGO_PKG_NAME"))
        .parameter_handler(handler.clone())
        .bind(args.host, args.port)
        .start()
        .await
        .expect("Failed to start server");

    #[cfg(feature = "remote-access")]
    let gateway = Some(
        Gateway::new()
            .name(env!("CARGO_PKG_NAME"))
            .parameter_handler(handler.clone())
            .start()
            .expect("Failed to start remote-access gateway"),
    );

    let shutdown = watch_ctrl_c();
    let worker = ParamWorker {
        store: initial_store,
        rx,
        server,
        #[cfg(feature = "remote-access")]
        gateway,
        shutdown,
    };
    worker.run().await;
}

fn watch_ctrl_c() -> CancellationToken {
    let token = CancellationToken::new();
    tokio::spawn({
        let token = token.clone();
        async move {
            tokio::signal::ctrl_c().await.ok();
            token.cancel();
        }
    });
    token
}
