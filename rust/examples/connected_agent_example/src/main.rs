//! Example demonstrating the use of the ConnectedAgent sink.
//!
//! This example shows how to create a connected agent for interprocess communication.

use foxglove::schemas::Log;
use foxglove::{log, ConnectedAgent, Context};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a context
    let ctx = Context::new();

    // Create a connected agent for interprocess communication
    let connected_agent = ConnectedAgent::new();

    // Add the connected agent to the context
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
