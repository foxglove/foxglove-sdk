//! Example demonstrating the use of the ConnectedAgent sink.
//!
//! This example shows how to create a connected agent for interprocess communication.

use foxglove::schemas::Log;
use foxglove::{log, ConnectedAgent, Context, AgentSinkConfig, UnixSocketConnection};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    // Get the default context (same one used by log! macro)
    let ctx = Context::get_default();

    // Create a connected agent for interprocess communication
    let config = AgentSinkConfig::default();

    // Establish the connection first
    let connection = match UnixSocketConnection::connect(&config.socket_path).await {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Failed to connect to agent: {}", e);
            return Ok(());
        }
    };

    // Create the connected agent with the established connection
    let connected_agent = ConnectedAgent::new(config, connection);

    // Add the connected agent to the default context
    ctx.add_sink(connected_agent.clone());

    // Run the poller in a background task
    let agent_clone = connected_agent.clone();
    tokio::spawn(async move {
        agent_clone.run().await;
    });

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
