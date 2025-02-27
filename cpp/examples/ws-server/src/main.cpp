#include <foxglove/server.hpp>

#include <atomic>
#include <chrono>
#include <csignal>
#include <functional>
#include <iostream>
#include <memory>
#include <thread>

using namespace std::chrono_literals;

static std::function<void()> sigintHandler;

int main(int argc, const char* argv[]) {
  std::signal(SIGINT, [](int) {
    if (sigintHandler) sigintHandler();
  });

  foxglove::WebSocketServerOptions options;
  options.name = "ws-demo-cpp";
  options.host = "127.0.0.1";
  options.port = 8765;
  foxglove::WebSocketServer server{options};
  std::cerr << "Started server" << std::endl;

  std::atomic_bool done = false;
  sigintHandler = [&] {
    std::cerr << "Shutting down..." << std::endl;
    server.stop();
    done = true;
  };

  while (!done) {
    std::this_thread::sleep_for(10ms);
  }

  std::cerr << "Done" << std::endl;
  return 0;
}
