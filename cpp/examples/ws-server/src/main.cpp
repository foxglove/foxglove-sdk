#include <foxglove/channel.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/server.hpp>

#include <atomic>
#include <chrono>
#include <csignal>
#include <fstream>
#include <functional>
#include <iostream>
#include <memory>
#include <thread>

using namespace std::chrono_literals;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

std::vector<std::byte> readFile(const std::string& filepath) {
  std::filesystem::path path(filepath);
  size_t length = std::filesystem::file_size(path);
  if (length == 0) {
    throw std::runtime_error("File is empty: " + filepath);
  }

  std::ifstream file(path, std::ios::binary);
  if (!file.is_open()) {
    throw std::runtime_error("Failed to open file: " + filepath);
  }

  std::vector<std::byte> buffer(length);
  if (!file.read(reinterpret_cast<char*>(buffer.data()), length)) {
    throw std::runtime_error("Failed to read file: " + filepath);
  }

  return buffer;
}

// NOLINTNEXTLINE(bugprone-exception-escape)
int main(int argc, char* argv[]) {
  std::string cert_path;
  std::string key_path;
  for (int i = 1; i < argc; ++i) {
    std::string arg = argv[i];
    if (arg == "--cert" && i + 1 < argc) {
      cert_path = argv[++i];
    } else if (arg == "--key" && i + 1 < argc) {
      key_path = argv[++i];
    } else {
      std::cerr << "Unknown argument: " << arg << "\n";
      std::exit(1);
    }
  }

  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  foxglove::WebSocketServerOptions options = {};
  options.name = "ws-demo-cpp";
  options.host = "127.0.0.1";
  options.port = 8765;
  options.capabilities = foxglove::WebSocketServerCapabilities::ClientPublish;
  options.supported_encodings = {"json"};
  options.callbacks.onSubscribe = [](uint64_t channel_id, const foxglove::ClientMetadata& client) {
    std::cerr << "Client " << client.id << " subscribed to channel " << channel_id << '\n';
  };
  options.callbacks.onUnsubscribe =
    [](uint64_t channel_id, const foxglove::ClientMetadata& client) {
      std::cerr << "Client " << client.id << " unsubscribed from channel " << channel_id << '\n';
    };
  options.callbacks.onClientAdvertise = [](
                                          uint32_t client_id, const foxglove::ClientChannel& channel
                                        ) {
    std::cerr << "Client " << client_id << " advertised channel " << channel.id << ":\n";
    std::cerr << "  Topic: " << channel.topic << '\n';
    std::cerr << "  Encoding: " << channel.encoding << '\n';
    std::cerr << "  Schema name: " << channel.schema_name << '\n';
    std::cerr << "  Schema encoding: "
              << (!channel.schema_encoding.empty() ? channel.schema_encoding : "(none)") << '\n';
    std::cerr << "  Schema: "
              << (channel.schema != nullptr
                    ? std::string(reinterpret_cast<const char*>(channel.schema), channel.schema_len)
                    : "(none)")
              << '\n';
  };
  options.callbacks.onMessageData =
    [](uint32_t client_id, uint32_t client_channel_id, const std::byte* data, size_t data_len) {
      std::cerr << "Client " << client_id << " published on channel " << client_channel_id << ": "
                << std::string(reinterpret_cast<const char*>(data), data_len) << '\n';
    };
  options.callbacks.onClientUnadvertise = [](uint32_t client_id, uint32_t client_channel_id) {
    std::cerr << "Client " << client_id << " unadvertised channel " << client_channel_id << '\n';
  };

  // Read TLS certificate and key files if provided
  std::vector<std::byte> cert_data;
  std::vector<std::byte> key_data;
  if (!cert_path.empty() && !key_path.empty()) {
    try {
      cert_data = readFile(cert_path);
      key_data = readFile(key_path);

      foxglove::TlsIdentity tls_identity;
      tls_identity.cert = cert_data.data();
      tls_identity.cert_len = cert_data.size();
      tls_identity.key = key_data.data();
      tls_identity.key_len = key_data.size();
      options.tls_identity = std::move(tls_identity);
    } catch (const std::exception& e) {
      std::cerr << "Error reading TLS files: " << e.what() << '\n';
      return 1;
    }
  } else if (!cert_path.empty() || !key_path.empty()) {
    std::cerr << "Error: Both --cert and --key must be provided for TLS\n";
    return 1;
  }

  auto server_result = foxglove::WebSocketServer::create(std::move(options));
  if (!server_result.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(server_result.error()) << '\n';
    return 1;
  }
  auto server = std::move(server_result.value());

  std::atomic_bool done = false;
  sigint_handler = [&] {
    std::cerr << "Shutting down...\n";
    server.stop();
    done = true;
  };

  foxglove::Schema schema;
  schema.name = "Test";
  schema.encoding = "jsonschema";
  std::string schema_data = R"({
    "type": "object",
    "properties": {
      "val": { "type": "number" }
    }
  })";
  schema.data = reinterpret_cast<const std::byte*>(schema_data.data());
  schema.data_len = schema_data.size();
  auto channel_result = foxglove::RawChannel::create("example", "json", std::move(schema));
  if (!channel_result.has_value()) {
    std::cerr << "Failed to create channel: " << foxglove::strerror(channel_result.error()) << '\n';
    return 1;
  }
  auto channel = std::move(channel_result.value());

  uint32_t i = 0;
  while (!done) {
    std::this_thread::sleep_for(100ms);
    std::string msg = "{\"val\": " + std::to_string(i) + "}";
    auto now =
      std::chrono::nanoseconds(std::chrono::system_clock::now().time_since_epoch()).count();
    channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size(), now);
    ++i;
  }

  std::cerr << "Done\n";
  return 0;
}
