//! Example demonstrating the use of the ConnectedAgent sink.
//!
//! This example shows how to create a connected agent for interprocess communication.

use foxglove::schemas::Log;
use foxglove::{log, ConnectedAgent, Context, AgentSinkConfig, UnixSocketConnection};
use tokio::signal;
use tracing::{info, error};

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
        Ok(conn) => {
            info!("Successfully connected to agent at {}", config.socket_path.display());
            conn
        }
        Err(e) => {
            error!("Failed to connect to agent: {}", e);
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

    // Log some messages with signal handling
    // let mut i = 0;
    for i in 1..=1 {
        tokio::select! {
            // Handle Ctrl+C (SIGINT)
            _ = signal::ctrl_c() => {
                println!("\nReceived SIGINT, shutting down gracefully...");
                break;
            }

            // Continue with message logging
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(1000)) => {
                let message = format!("Hello from connected agent example! Message {}", i);

                log!(
                    "/log",
                    Log {
                        message: message.clone(),
                        ..Default::default()
                    }
                );
            }
        }
    }

    // tokio::select! {
    //     // Handle Ctrl+C (SIGINT)
    //     _ = signal::ctrl_c() => {
    //         println!("\nReceived SIGINT, shutting down gracefully...");
    //     }
    //     _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {
    //         println!("Timeout, shutting down gracefully...");
    //     }
    // }

    println!("Connected agent example completed!");
    Ok(())
}
