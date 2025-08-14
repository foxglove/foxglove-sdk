//! Example demonstrating the use of the ConnectedAgent sink.
//!
//! This example shows how to create a connected agent for interprocess communication.

use foxglove::schemas::Log;
use foxglove::{log, ConnectedAgent, Context};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    // Get the default context (same one used by log! macro)
    let ctx = Context::get_default();

    // Create a connected agent for interprocess communication
    let connected_agent = ConnectedAgent::new();

    // Attempt to connect to the agent
    if let Err(e) = connected_agent.connect().await {
        eprintln!("Failed to connect to agent: {}", e);
        // Continue anyway to see the debug messages
    }

    // Add the connected agent to the default context
    ctx.add_sink(connected_agent.clone());

    // Log some messages
    for i in 1..=5 {
        let message = format!("Hello from connected agent example! Message {}", i);

        log!(
            "/log",
            Log {
                message: message.clone(),
                ..Default::default()
            }
        );

        // Small delay to see the messages
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("Connected agent example completed!");
    Ok(())
}
