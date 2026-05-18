//! Example of a parameter server using the Foxglove SDK.
//!
//! The handler implements [`ParameterHandler`] by enqueueing each get/set request on an mpsc
//! channel and returning immediately. A single worker task drains the channel, mutates a local
//! parameter store, and fulfils each responder. The same worker also publishes a periodic
//! "elapsed" update to subscribers, so the parameter store has exactly one owner and no
//! synchronization is required.
//!
//! This is the recommended shape for handlers that need to perform non-trivial work to compute a
//! response: it keeps the SDK's internal threads unblocked, and serializes ordering per-client
//! (the SDK guarantees per-client message order, which is preserved by the single worker).
//!
//! Usage:
//! ```text
//! cargo run -p example_param_server
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use foxglove::WebSocketServer;
use foxglove::websocket::{
    Client, GetParametersResponder, Parameter, ParameterHandler, ParameterType, ParameterValue,
    SetParametersResponder, Status,
};
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

impl ParameterHandler<Client> for ParamHandler {
    fn get(
        &self,
        _client: Client,
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
        _client: Client,
        parameters: Vec<Parameter>,
        _request_id: Option<String>,
        responder: SetParametersResponder,
    ) {
        let _ = self.tx.try_send(ParameterOp::Set {
            parameters,
            responder,
        });
    }

    fn subscribe(&self, names: Vec<String>) {
        println!("subscribe: {names:?}");
    }

    fn unsubscribe(&self, names: Vec<String>) {
        println!("unsubscribe: {names:?}");
    }
}

/// Owns the parameter store. Drains parameter ops one at a time, and on a separate tick updates
/// the "elapsed" parameter and broadcasts it to subscribers. Shutdown is signalled via a
/// `CancellationToken`; after the loop exits, the worker stops the server.
struct ParamWorker {
    store: HashMap<String, Parameter>,
    rx: mpsc::Receiver<ParameterOp>,
    server: foxglove::WebSocketServerHandle,
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
        println!("get: {names:?}");
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
        println!("set: {names:?}");
        for param in &mut parameters {
            if let Some(existing) = self.store.get_mut(&param.name) {
                if param.name.starts_with("read_only_") {
                    // Send a warning, and echo back the existing value so the client sees no change.
                    responder.client().send_status(Status::warning(format!(
                        "parameter {} is read only",
                        param.name
                    )));
                    param.value.clone_from(&existing.value);
                    param.r#type.clone_from(&existing.r#type);
                } else {
                    existing.value.clone_from(&param.value);
                    existing.r#type.clone_from(&param.r#type);
                }
            } else {
                self.store.insert(param.name.clone(), param.clone());
            }
        }
        responder.respond(parameters);
    }

    fn update_and_publish_elapsed(&mut self, start: Instant) {
        let elapsed = Parameter {
            name: "elapsed".to_string(),
            value: Some(ParameterValue::Float64(start.elapsed().as_secs_f64())),
            r#type: Some(ParameterType::Float64),
        };
        self.store.insert(elapsed.name.clone(), elapsed.clone());
        self.server.publish_parameter_values(vec![elapsed]);
    }
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
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
        .parameter_handler(handler)
        .bind(args.host, args.port)
        .start()
        .await
        .expect("Failed to start server");

    let shutdown = watch_ctrl_c();
    let worker = ParamWorker {
        store: initial_store,
        rx,
        server,
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
