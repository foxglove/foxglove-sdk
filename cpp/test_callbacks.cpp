#include <foxglove/context.hpp>
#include <foxglove/server.hpp>

#include <atomic>
#include <chrono>
#include <iostream>
#include <thread>

std::atomic<int> connect_count{0};
std::atomic<int> disconnect_count{0};

int main() {
  std::cout << "Testing client connect/disconnect callbacks..." << std::endl;

  // Create context
  auto context = foxglove::Context::create();

  // Setup server options with callbacks
  foxglove::WebSocketServerOptions options;
  options.context = context;
  options.name = "Test Server";
  options.host = "127.0.0.1";
  options.port = 8765;
  options.capabilities = foxglove::WebSocketServerCapabilities::ClientPublish;

  // Set up client connect/disconnect callbacks
  options.callbacks.onClientConnect = []() {
    connect_count++;
    std::cout << "Client connected! Total connections: " << connect_count.load() << std::endl;
  };

  options.callbacks.onClientDisconnect = []() {
    disconnect_count++;
    std::cout << "Client disconnected! Total disconnections: " << disconnect_count.load()
              << std::endl;
  };

  // Create and start server
  auto server_result = foxglove::WebSocketServer::create(std::move(options));
  if (!server_result.has_value()) {
    std::cerr << "Failed to create server: " << static_cast<int>(server_result.error())
              << std::endl;
    return 1;
  }

  auto server = std::move(server_result.value());
  std::cout << "Server started on port: " << server.port() << std::endl;

  // Run for 30 seconds to allow manual testing with a WebSocket client
  std::cout << "Server running... Connect some clients to test callbacks." << std::endl;
  std::cout
    << "You can use the Foxglove app or any WebSocket client to connect to ws://localhost:8765"
    << std::endl;

  for (int i = 0; i < 30; i++) {
    std::this_thread::sleep_for(std::chrono::seconds(1));

    // Print current client count every 5 seconds
    if (i % 5 == 0) {
      std::cout << "Current client count: " << server.clientCount() << std::endl;
      std::cout << "Total connects: " << connect_count.load()
                << ", Total disconnects: " << disconnect_count.load() << std::endl;
    }
  }

  std::cout << "Test completed!" << std::endl;
  std::cout << "Final stats - Connects: " << connect_count.load()
            << ", Disconnects: " << disconnect_count.load()
            << ", Current clients: " << server.clientCount() << std::endl;

  // Stop the server
  auto stop_result = server.stop();
  if (stop_result != foxglove::FoxgloveError::Ok) {
    std::cerr << "Error stopping server: " << static_cast<int>(stop_result) << std::endl;
  }

  return 0;
}