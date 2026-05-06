// Integration tests that validate byte stream framing, channel advertisements,
// subscriptions, and message delivery using a local LiveKit dev server.
//
// Requires a local LiveKit server via `docker compose up -d`.

#include <foxglove/channel.hpp>
#include <foxglove/connection_graph.hpp>
#include <foxglove/context.hpp>
#include <foxglove/remote_access.hpp>
#include <foxglove/schema.hpp>

#include <catch2/catch_test_macros.hpp>
#include <nlohmann/json.hpp>

#include <algorithm>
#include <string>
#include <thread>
#include <vector>

#include "frame.hpp"
#include "mock_listener.hpp"
#include "mock_server.hpp"
#include "test_gateway.hpp"
#include "test_helpers.hpp"
#include "viewer_connection.hpp"

using namespace foxglove_integration;
using namespace std::chrono_literals;

// ===========================================================================
// Core subscribe / advertise / message delivery tests
// ===========================================================================

TEST_CASE("livekit: viewer receives server info", "[integration]") {
  auto ctx = foxglove::Context::create();
  auto gw = TestGateway::start(ctx);

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  auto server_info = viewer.expect_server_info();

  REQUIRE(server_info.contains("sessionId"));
  REQUIRE(server_info.contains("metadata"));
  auto metadata = server_info["metadata"];
  REQUIRE(metadata.contains("fg-library"));
  REQUIRE(server_info.contains("supportedEncodings"));
  auto encodings = server_info["supportedEncodings"];
  bool has_json = false;
  for (const auto& enc : encodings) {
    if (enc.get<std::string>() == "json") {
      has_json = true;
    }
  }
  REQUIRE(has_json);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: viewer receives channel advertisement", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  auto server_info = viewer.expect_server_info();

  auto channel = foxglove::RawChannel::create("/test", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());
  auto channel_id = channel->id();

  auto advertise = viewer.expect_advertise();

  auto& channels = advertise["channels"];
  REQUIRE(channels.size() == 1);
  CHECK(channels[0]["topic"].get<std::string>() == "/test");
  CHECK(channels[0]["encoding"].get<std::string>() == "json");
  CHECK(channels[0]["id"].get<uint64_t>() == channel_id);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: viewer receives message after subscribe", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  // Create the channel after the viewer is fully connected so the gateway
  // publishes its data track via the SFU's real-time announce path instead of
  // the JoinResponse, which is racy through the C++ FFI bridge.
  auto channel = foxglove::RawChannel::create("/test", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.subscribe_and_wait({channel_id}, [&] {
    return channel->hasSinks();
  });

  auto ch_reader = viewer.expect_device_channel_data_track(channel_id);

  std::string payload1 = "message-1";
  channel->log(reinterpret_cast<const std::byte*>(payload1.data()), payload1.size());
  auto msg = ch_reader->next_server_message();
  CHECK(msg.value("op", "") == "messageData");
  auto data1 = msg.value("data", std::vector<uint8_t>{});
  CHECK(std::string(data1.begin(), data1.end()) == payload1);

  std::string payload2 = "message-2";
  channel->log(reinterpret_cast<const std::byte*>(payload2.data()), payload2.size());
  msg = ch_reader->next_server_message();
  CHECK(msg.value("op", "") == "messageData");
  auto data2 = msg.value("data", std::vector<uint8_t>{});
  CHECK(std::string(data2.begin(), data2.end()) == payload2);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: viewer does not receive message before subscribe", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  // Create the channel after the viewer is fully connected so the gateway
  // publishes its data track via the SFU's real-time announce path. If the
  // channel pre-exists the viewer, the data track surfaces in the
  // JoinResponse, and the C++ FFI bridge occasionally drops the
  // `DataTrackPublished` event during the join burst.
  auto channel = foxglove::RawChannel::create("/test", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  std::string before = "message-before-subscribe";
  channel->log(reinterpret_cast<const std::byte*>(before.data()), before.size());

  viewer.subscribe_and_wait({channel_id}, [&] {
    return channel->hasSinks();
  });
  viewer.ensure_device_data_track(channel_id);

  std::string after = "message-after-subscribe";
  channel->log(reinterpret_cast<const std::byte*>(after.data()), after.size());

  auto msg = viewer.expect_new_data_track_and_message_data(channel_id);
  CHECK(msg.value("op", "") == "messageData");
  auto data = msg.value("data", std::vector<uint8_t>{});
  CHECK(std::string(data.begin(), data.end()) == after);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: viewer receives unadvertise on channel close", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  auto channel = foxglove::RawChannel::create("/test", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  channel->close();

  auto unadvertise = viewer.expect_unadvertise();
  auto& ids = unadvertise["channelIds"];
  REQUIRE(ids.size() == 1);
  CHECK(ids[0].get<uint64_t>() == channel_id);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: viewer receives advertisement for late channel", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");

  viewer.expect_server_info();

  auto channel = foxglove::RawChannel::create("/late-topic", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto advertise = viewer.expect_advertise();
  CHECK(advertise["channels"].size() == 1);
  CHECK(advertise["channels"][0]["topic"].get<std::string>() == "/late-topic");
  CHECK(advertise["channels"][0]["id"].get<uint64_t>() == channel->id());

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: channel filter excludes filtered channels", "[integration]") {
  auto ctx = foxglove::Context::create();

  TestGatewayOptions opts;
  opts.channel_filter = [](const foxglove::ChannelDescriptor& ch) {
    return std::string(ch.topic()).find("/allowed") == 0;
  };
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  auto allowed = foxglove::RawChannel::create("/allowed/data", "json", std::nullopt, ctx);
  REQUIRE(allowed.has_value());
  auto blocked = foxglove::RawChannel::create("/blocked/data", "json", std::nullopt, ctx);
  REQUIRE(blocked.has_value());

  auto advertise = viewer.expect_advertise();

  REQUIRE(advertise["channels"].size() == 1);
  CHECK(advertise["channels"][0]["topic"].get<std::string>() == "/allowed/data");
  CHECK(advertise["channels"][0]["id"].get<uint64_t>() == allowed->id());

  (void)blocked;
  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: multiple participants receive messages", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  // The spirit of this test is fan-out: a single channel logged once at the
  // gateway is delivered to every subscribed viewer, and one viewer leaving
  // does not disturb the others. We deliberately classify the channel as
  // Reliable so messages flow over the control plane (WebSocket) rather than
  // the lossy data-track plane. The data-track path is exercised by other
  // tests (e.g. "viewer receives message after subscribe"). Reliable delivery
  // on the control plane keeps this fan-out test focused on what it asserts
  // (fan-out works and survives a peer disconnecting) without introducing
  // data-track timing concerns.
  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.qos_classifier = [](const foxglove::ChannelDescriptor& /*ch*/) {
    return foxglove::QosProfile{foxglove::Reliability::Reliable};
  };
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer1 = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer1.expect_server_info();

  auto channel = foxglove::RawChannel::create("/test", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto adv1 = viewer1.expect_advertise();
  REQUIRE(adv1["channels"].size() == 1);
  auto channel_id = adv1["channels"][0]["id"].get<uint64_t>();
  CHECK(
    adv1["channels"][0]
      .value("metadata", nlohmann::json::object())
      .value("foxglove.reliable", "") == "true"
  );
  viewer1.subscribe_and_wait({channel_id}, [&] {
    return channel->hasSinks();
  });

  auto viewer2 = ViewerConnection::connect(gw.room_name, "viewer-2");
  viewer2.expect_server_info();
  auto adv2 = viewer2.expect_advertise();
  CHECK(adv2["channels"][0]["id"].get<uint64_t>() == channel_id);
  viewer2.send_subscribe({channel_id});
  poll_until([&] {
    return listener.subscribed_count() == 2;
  });

  auto check_received = [channel_id](const nlohmann::json& msg, const std::string& expected) {
    CHECK(msg.value("channelId", uint64_t{0}) == channel_id);
    auto data = msg.value("data", std::vector<uint8_t>{});
    CHECK(std::string(data.begin(), data.end()) == expected);
  };

  std::string payload1 = "fanout-message-1";
  channel->log(reinterpret_cast<const std::byte*>(payload1.data()), payload1.size());
  check_received(viewer1.expect_message_data(), payload1);
  check_received(viewer2.expect_message_data(), payload1);

  viewer1.close();
  viewer2.wait_for_participant_disconnected("viewer-1");
  poll_until([&] {
    return listener.unsubscribed_count() >= 1;
  });

  std::string payload2 = "fanout-message-2";
  channel->log(reinterpret_cast<const std::byte*>(payload2.data()), payload2.size());
  check_received(viewer2.expect_message_data(), payload2);

  viewer2.close();
  gw.stop();
}

// ===========================================================================
// Video track tests
// ===========================================================================

TEST_CASE("livekit: video channel has video track metadata", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");

  viewer.expect_server_info();

  foxglove::Schema video_schema{"foxglove.RawImage", "protobuf", nullptr, 0};
  auto video_channel = foxglove::RawChannel::create("/camera", "protobuf", video_schema, ctx);
  REQUIRE(video_channel.has_value());
  auto json_channel = foxglove::RawChannel::create("/data", "json", std::nullopt, ctx);
  REQUIRE(json_channel.has_value());

  auto adv1 = viewer.expect_advertise();
  auto adv2 = viewer.expect_advertise();

  for (const auto* adv : {&adv1, &adv2}) {
    REQUIRE((*adv)["channels"].size() == 1);
    const auto& ch = (*adv)["channels"][0];
    if (ch["id"].get<uint64_t>() == video_channel->id()) {
      auto meta = ch.value("metadata", nlohmann::json::object());
      CHECK(meta.value("foxglove.hasVideoTrack", "") == "true");
    } else {
      CHECK(ch["id"].get<uint64_t>() == json_channel->id());
      auto meta = ch.value("metadata", nlohmann::json::object());
      CHECK(!meta.contains("foxglove.hasVideoTrack"));
    }
  }

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: video channel messages bypass data plane", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  // Create the channels after the viewer is fully connected so the gateway
  // publishes their data tracks via the SFU's real-time announce path. If the
  // channels pre-exist the viewer, their data tracks surface in the
  // JoinResponse, and the C++ FFI bridge occasionally drops the
  // `DataTrackPublished` event during the join burst.
  auto video_channel = foxglove::RawChannel::create(
    "/camera", "protobuf", foxglove::Schema{"foxglove.RawImage", "protobuf", nullptr, 0}, ctx
  );
  REQUIRE(video_channel.has_value());
  auto json_channel = foxglove::RawChannel::create("/data", "json", std::nullopt, ctx);
  REQUIRE(json_channel.has_value());

  // Each channel created after the viewer connects produces its own advertise.
  viewer.expect_advertise();
  viewer.expect_advertise();

  auto video_id = video_channel->id();
  auto json_id = json_channel->id();

  // Subscribe to video with requestVideoTrack, json without.
  viewer.send_subscribe_video({video_id});
  viewer.send_subscribe({json_id});
  poll_until([&] {
    return json_channel->hasSinks();
  });

  viewer.ensure_device_data_track(json_id);

  std::string video_payload = "video-frame";
  video_channel->log(
    reinterpret_cast<const std::byte*>(video_payload.data()), video_payload.size()
  );
  std::string json_payload = "json-payload";
  json_channel->log(reinterpret_cast<const std::byte*>(json_payload.data()), json_payload.size());

  // The JSON data track must carry json_payload, not the video frame.
  auto msg = viewer.expect_new_data_track_and_message_data(json_id);
  CHECK(msg.value("op", "") == "messageData");
  CHECK(msg.value("channelId", uint64_t{0}) == json_id);
  auto data = msg.value("data", std::vector<uint8_t>{});
  CHECK(std::string(data.begin(), data.end()) == json_payload);

  // We still publish a data track for the video channel,
  // because it can be subscribed to with request_video_track: false.
  CHECK(viewer.has_device_data_track(video_id));

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: video track lifecycle", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");

  viewer.expect_server_info();

  auto video_channel = foxglove::RawChannel::create(
    "/camera", "protobuf", foxglove::Schema{"foxglove.RawImage", "protobuf", nullptr, 0}, ctx
  );
  REQUIRE(video_channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.subscribe_video_and_wait({channel_id}, [&] {
    return video_channel->hasSinks();
  });
  auto expected_track_name = "video-ch-" + std::to_string(channel_id);
  auto track_name = viewer.expect_track_subscribed();
  CHECK(track_name == expected_track_name);

  viewer.send_unsubscribe({channel_id});
  track_name = viewer.expect_track_unsubscribed();
  CHECK(track_name == expected_track_name);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: video track resubscribe", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");

  viewer.expect_server_info();

  auto video_channel = foxglove::RawChannel::create(
    "/camera", "protobuf", foxglove::Schema{"foxglove.RawImage", "protobuf", nullptr, 0}, ctx
  );
  REQUIRE(video_channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.subscribe_video_and_wait({channel_id}, [&] {
    return video_channel->hasSinks();
  });
  auto expected_track_name = "video-ch-" + std::to_string(channel_id);
  CHECK(viewer.expect_track_subscribed() == expected_track_name);

  viewer.send_unsubscribe({channel_id});
  CHECK(viewer.expect_track_unsubscribed() == expected_track_name);

  // Give the gateway's spawned unpublish_track future a moment to drain
  // before we issue a publish for the same track name. Without this, the
  // LiveKit SDK can serialize the back-to-back renegotiations slowly enough
  // to exceed EVENT_TIMEOUT.
  std::this_thread::sleep_for(250ms);

  viewer.send_subscribe_video({channel_id});
  CHECK(viewer.expect_track_subscribed() == expected_track_name);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: video channel without request video track uses data plane", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  // Create the channel after the viewer is fully connected so the gateway
  // publishes its data track via the SFU's real-time announce path instead of
  // the JoinResponse, which is racy through the C++ FFI bridge.
  auto video_channel = foxglove::RawChannel::create(
    "/camera", "protobuf", foxglove::Schema{"foxglove.RawImage", "protobuf", nullptr, 0}, ctx
  );
  REQUIRE(video_channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.subscribe_and_wait({channel_id}, [&] {
    return video_channel->hasSinks();
  });
  viewer.ensure_device_data_track(channel_id);

  std::string payload = "video-frame";
  video_channel->log(reinterpret_cast<const std::byte*>(payload.data()), payload.size());
  auto msg = viewer.expect_new_data_track_and_message_data(channel_id);
  CHECK(msg.value("op", "") == "messageData");
  auto data = msg.value("data", std::vector<uint8_t>{});
  CHECK(std::string(data.begin(), data.end()) == payload);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: video resubscribe switches to data plane", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  // Create the channel after the viewer is fully connected so the gateway
  // publishes its data track via the SFU's real-time announce path instead of
  // the JoinResponse, which is racy through the C++ FFI bridge.
  auto video_channel = foxglove::RawChannel::create(
    "/camera", "protobuf", foxglove::Schema{"foxglove.RawImage", "protobuf", nullptr, 0}, ctx
  );
  REQUIRE(video_channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.subscribe_video_and_wait({channel_id}, [&] {
    return video_channel->hasSinks();
  });
  auto expected_track_name = "video-ch-" + std::to_string(channel_id);
  CHECK(viewer.expect_track_subscribed() == expected_track_name);

  // Re-subscribe without video track
  viewer.send_subscribe({channel_id});
  CHECK(viewer.expect_track_unsubscribed() == expected_track_name);

  viewer.ensure_device_data_track(channel_id);

  std::string payload = "video-frame";
  video_channel->log(reinterpret_cast<const std::byte*>(payload.data()), payload.size());
  auto msg = viewer.expect_new_data_track_and_message_data(channel_id);
  CHECK(msg.value("op", "") == "messageData");
  auto data = msg.value("data", std::vector<uint8_t>{});
  CHECK(std::string(data.begin(), data.end()) == payload);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: request video track on non-video channel sends error", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");

  viewer.expect_server_info();

  auto json_channel = foxglove::RawChannel::create("/json_data", "json", std::nullopt, ctx);
  REQUIRE(json_channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.send_subscribe_video({channel_id});

  auto status = viewer.expect_status();
  CHECK(status["level"].get<int>() == 2);  // Error level
  auto message = status["message"].get<std::string>();
  CHECK(message.find("does not support video transcoding") != std::string::npos);
  CHECK(!json_channel->hasSinks());

  viewer.close();
  gw.stop();
}

// ===========================================================================
// Listener callback tests: client advertise / unadvertise
// ===========================================================================

TEST_CASE("livekit: client advertise fires listener callback", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ClientPublish;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_client_advertise({{1, "/cmd", "json", ""}});
  poll_until([&] {
    return listener.client_advertised_count() == 1;
  });

  {
    std::lock_guard<std::mutex> lock(listener.mutex);
    REQUIRE(listener.client_advertised.size() == 1);
    CHECK(std::get<1>(listener.client_advertised[0]) == "/cmd");
  }

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: client advertise preserves schema name without schema data", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ClientPublish;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  // Advertise a channel with schema_name but no binary schema data — this is
  // what the Foxglove teleop panel sends for /cmd_vel.
  viewer.send_client_advertise({{1, "/cmd_vel", "json", "geometry_msgs/msg/Twist"}});

  poll_until([&] {
    return listener.client_advertised_count() == 1;
  });

  {
    std::lock_guard<std::mutex> lock(listener.mutex);
    REQUIRE(listener.client_advertised.size() == 1);
    CHECK(std::get<1>(listener.client_advertised[0]) == "/cmd_vel");
    CHECK(std::get<2>(listener.client_advertised[0]) == "geometry_msgs/msg/Twist");
  }

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: client unadvertise fires listener callback", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ClientPublish;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_client_advertise({{42, "/joy", "json", ""}});
  poll_until([&] {
    return listener.client_advertised_count() == 1;
  });

  viewer.send_client_unadvertise({42});
  poll_until([&] {
    return listener.client_unadvertised_count() == 1;
  });

  {
    std::lock_guard<std::mutex> lock(listener.mutex);
    REQUIRE(listener.client_unadvertised.size() == 1);
    CHECK(listener.client_unadvertised[0].second == "/joy");
  }

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: client message data for unadvertised channel sends error", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ClientPublish;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  // Send MessageData for channel 999, which was never advertised by the client.
  std::vector<uint8_t> payload = {'r', 'o', 'g', 'u', 'e'};
  viewer.send_client_message_data(999, payload);

  auto deadline = std::chrono::steady_clock::now() + EVENT_TIMEOUT;
  nlohmann::json status;
  while (std::chrono::steady_clock::now() < deadline) {
    auto msg = viewer.next_server_message();
    if (msg.value("op", "") == "status") {
      status = msg;
      break;
    }
  }
  REQUIRE(!status.empty());
  CHECK(status["level"].get<int>() == 2);  // Error
  auto message = status["message"].get<std::string>();
  CHECK(message.find("not advertised channel") != std::string::npos);

  CHECK(listener.message_data_count() == 0);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: client disconnect fires unadvertise for advertised channels", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ClientPublish;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_client_advertise({{1, "/cmd_vel", "json", ""}, {2, "/joy", "json", ""}});
  poll_until([&] {
    return listener.client_advertised_count() == 2;
  });

  viewer.close();
  poll_until([&] {
    return listener.client_unadvertised_count() == 2;
  });

  {
    std::lock_guard<std::mutex> lock(listener.mutex);
    REQUIRE(listener.client_unadvertised.size() == 2);
    std::vector<std::string> topics;
    for (const auto& [id, topic] : listener.client_unadvertised) {
      topics.push_back(topic);
    }
    CHECK(std::find(topics.begin(), topics.end(), "/cmd_vel") != topics.end());
    CHECK(std::find(topics.begin(), topics.end(), "/joy") != topics.end());
  }

  gw.stop();
}

// ===========================================================================
// Listener callback tests: subscribe / unsubscribe
// ===========================================================================

TEST_CASE("livekit: subscribe fires listener callback", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  auto channel = foxglove::RawChannel::create("/camera", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.send_subscribe({channel_id});
  poll_until([&] {
    return listener.subscribed_count() == 1;
  });

  {
    std::lock_guard<std::mutex> lock(listener.mutex);
    REQUIRE(listener.subscribed.size() == 1);
    CHECK(listener.subscribed[0].second == "/camera");
  }

  viewer.close();
  poll_until([&] {
    return listener.unsubscribed_count() == 1;
  });
  gw.stop();
}

TEST_CASE("livekit: unsubscribe fires listener callback", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  auto channel = foxglove::RawChannel::create("/lidar", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.subscribe_and_wait({channel_id}, [&] {
    return channel->hasSinks();
  });
  poll_until([&] {
    return listener.subscribed_count() == 1;
  });

  viewer.send_unsubscribe({channel_id});
  poll_until([&] {
    return listener.unsubscribed_count() == 1;
  });

  {
    std::lock_guard<std::mutex> lock(listener.mutex);
    REQUIRE(listener.unsubscribed.size() == 1);
    CHECK(listener.unsubscribed[0].second == "/lidar");
  }

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: disconnect fires unsubscribe for subscribed channels", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  auto channel = foxglove::RawChannel::create("/imu", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.subscribe_and_wait({channel_id}, [&] {
    return channel->hasSinks();
  });
  poll_until([&] {
    return listener.subscribed_count() == 1;
  });

  viewer.close();
  poll_until([&] {
    return listener.unsubscribed_count() == 1;
  });

  {
    std::lock_guard<std::mutex> lock(listener.mutex);
    REQUIRE(listener.unsubscribed.size() == 1);
    CHECK(listener.unsubscribed[0].second == "/imu");
  }

  gw.stop();
}

TEST_CASE("livekit: channel close fires unsubscribe for subscribers", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  auto channel = foxglove::RawChannel::create("/radar", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto advertise = viewer.expect_advertise();
  auto channel_id = advertise["channels"][0]["id"].get<uint64_t>();

  viewer.subscribe_and_wait({channel_id}, [&] {
    return channel->hasSinks();
  });
  poll_until([&] {
    return listener.subscribed_count() == 1;
  });

  channel->close();

  auto unadvertise = viewer.expect_unadvertise();
  CHECK(unadvertise["channelIds"][0].get<uint64_t>() == channel_id);

  poll_until([&] {
    return listener.unsubscribed_count() == 1;
  });

  {
    std::lock_guard<std::mutex> lock(listener.mutex);
    REQUIRE(listener.unsubscribed.size() == 1);
    CHECK(listener.unsubscribed[0].second == "/radar");
  }

  viewer.close();
  gw.stop();
}

// ===========================================================================
// Client publish / message data tests
// ===========================================================================

TEST_CASE("livekit: client message data fires listener callback", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ClientPublish;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_client_advertise({{1, "/cmd", "json", ""}});
  poll_until([&] {
    return listener.client_advertised_count() == 1;
  });

  std::vector<uint8_t> payload = {'{', '"', 'v', '"', ':', '1', '}'};
  viewer.send_client_message_data(1, payload);

  poll_until([&] {
    return listener.message_data_count() == 1;
  });

  {
    std::lock_guard<std::mutex> lock(listener.mutex);
    REQUIRE(listener.message_data.size() == 1);
    auto& [client_id, topic, data] = listener.message_data[0];
    CHECK(topic == "/cmd");
  }

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: client message data before advertise sends error", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ClientPublish;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  std::vector<uint8_t> payload = {'e', 'a', 'r', 'l', 'y'};
  viewer.send_client_message_data(1, payload);

  // Expect an error status message because channel 1 was never advertised.
  auto deadline = std::chrono::steady_clock::now() + EVENT_TIMEOUT;
  nlohmann::json status;
  while (std::chrono::steady_clock::now() < deadline) {
    auto msg = viewer.next_server_message();
    if (msg.value("op", "") == "status") {
      status = msg;
      break;
    }
  }
  REQUIRE(!status.empty());
  CHECK(status["level"].get<int>() == 2);  // Error
  auto message = status["message"].get<std::string>();
  CHECK(message.find("not advertised channel") != std::string::npos);

  CHECK(listener.message_data_count() == 0);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: client advertise without capability sends error", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_client_advertise({{1, "/cmd", "json", ""}});

  // Read messages until we get a status error
  auto deadline = std::chrono::steady_clock::now() + EVENT_TIMEOUT;
  nlohmann::json status;
  while (std::chrono::steady_clock::now() < deadline) {
    auto msg = viewer.next_server_message();
    if (msg.value("op", "") == "status") {
      status = msg;
      break;
    }
  }
  REQUIRE(!status.empty());
  CHECK(status["level"].get<int>() == 2);  // Error

  viewer.close();
  gw.stop();
}

// ===========================================================================
// Connection status tests
// ===========================================================================

TEST_CASE("livekit: connection status lifecycle", "[integration]") {
  auto ctx = foxglove::Context::create();

  std::mutex status_mutex;
  std::vector<foxglove::RemoteAccessConnectionStatus> statuses;

  TestGatewayOptions opts;
  opts.callbacks.onConnectionStatusChanged = [&](foxglove::RemoteAccessConnectionStatus status) {
    std::lock_guard<std::mutex> lock(status_mutex);
    statuses.push_back(status);
  };
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  poll_until([&] {
    std::lock_guard<std::mutex> lock(status_mutex);
    for (auto s : statuses) {
      if (s == foxglove::RemoteAccessConnectionStatus::Connected) {
        return true;
      }
    }
    return false;
  });
  CHECK(gw.connection_status() == foxglove::RemoteAccessConnectionStatus::Connected);

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-status");

  viewer.expect_server_info();

  auto channel = foxglove::RawChannel::create("/status-test", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  viewer.expect_advertise();

  viewer.subscribe_and_wait({channel->id()}, [&] {
    return channel->hasSinks();
  });

  viewer.close();

  gw.stop();

  std::lock_guard<std::mutex> lock(status_mutex);
  REQUIRE(statuses.size() >= 4);
  CHECK(statuses[0] == foxglove::RemoteAccessConnectionStatus::Connecting);
  CHECK(statuses[1] == foxglove::RemoteAccessConnectionStatus::Connected);
  CHECK(statuses[statuses.size() - 2] == foxglove::RemoteAccessConnectionStatus::ShuttingDown);
  CHECK(statuses[statuses.size() - 1] == foxglove::RemoteAccessConnectionStatus::Shutdown);
}

// ===========================================================================
// Connection graph tests
// ===========================================================================

TEST_CASE("livekit: connection graph subscribe receives empty initial state", "[integration]") {
  auto ctx = foxglove::Context::create();

  TestGatewayOptions opts;
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ConnectionGraph;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_subscribe_connection_graph();
  auto update = viewer.expect_connection_graph_update();

  CHECK(update.value("publishedTopics", nlohmann::json::array()).empty());
  CHECK(update.value("subscribedTopics", nlohmann::json::array()).empty());
  CHECK(update.value("advertisedServices", nlohmann::json::array()).empty());
  CHECK(update.value("removedTopics", nlohmann::json::array()).empty());
  CHECK(update.value("removedServices", nlohmann::json::array()).empty());

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: connection graph subscribe and publish", "[integration]") {
  auto ctx = foxglove::Context::create();

  TestGatewayOptions opts;
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ConnectionGraph;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_subscribe_connection_graph();
  viewer.expect_connection_graph_update();  // initial empty

  foxglove::ConnectionGraph graph;
  graph.setPublishedTopic("/camera", {"node_1"});
  graph.setSubscribedTopic("/camera", {"node_2"});
  graph.setAdvertisedService("/set_mode", {"node_1"});
  CHECK(gw.gateway().publishConnectionGraph(graph) == foxglove::FoxgloveError::Ok);

  auto update = viewer.expect_connection_graph_update();

  auto pub_topics = update.value("publishedTopics", nlohmann::json::array());
  REQUIRE(pub_topics.size() == 1);
  CHECK(pub_topics[0]["name"].get<std::string>() == "/camera");

  auto sub_topics = update.value("subscribedTopics", nlohmann::json::array());
  REQUIRE(sub_topics.size() == 1);
  CHECK(sub_topics[0]["name"].get<std::string>() == "/camera");

  auto services = update.value("advertisedServices", nlohmann::json::array());
  REQUIRE(services.size() == 1);
  CHECK(services[0]["name"].get<std::string>() == "/set_mode");

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: connection graph publish diff update", "[integration]") {
  auto ctx = foxglove::Context::create();

  TestGatewayOptions opts;
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ConnectionGraph;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_subscribe_connection_graph();
  viewer.expect_connection_graph_update();  // initial

  foxglove::ConnectionGraph graph1;
  graph1.setPublishedTopic("/camera", {"node_1"});
  graph1.setAdvertisedService("/set_mode", {"node_1"});
  CHECK(gw.gateway().publishConnectionGraph(graph1) == foxglove::FoxgloveError::Ok);
  viewer.expect_connection_graph_update();

  foxglove::ConnectionGraph graph2;
  graph2.setPublishedTopic("/lidar", {"node_2"});
  graph2.setAdvertisedService("/set_mode", {"node_2"});
  CHECK(gw.gateway().publishConnectionGraph(graph2) == foxglove::FoxgloveError::Ok);

  auto update = viewer.expect_connection_graph_update();

  auto pub_topics = update.value("publishedTopics", nlohmann::json::array());
  REQUIRE(pub_topics.size() == 1);
  CHECK(pub_topics[0]["name"].get<std::string>() == "/lidar");

  auto removed = update.value("removedTopics", nlohmann::json::array());
  REQUIRE(removed.size() == 1);
  CHECK(removed[0].get<std::string>() == "/camera");

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: connection graph unsubscribe stops updates", "[integration]") {
  auto ctx = foxglove::Context::create();

  TestGatewayOptions opts;
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ConnectionGraph;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_subscribe_connection_graph();
  viewer.expect_connection_graph_update();
  viewer.send_unsubscribe_connection_graph();

  std::this_thread::sleep_for(500ms);

  foxglove::ConnectionGraph graph;
  graph.setPublishedTopic("/camera", {"node_1"});
  CHECK(gw.gateway().publishConnectionGraph(graph) == foxglove::FoxgloveError::Ok);

  // Create the channel after the viewer is fully connected so the gateway
  // publishes its data track via the SFU's real-time announce path instead of
  // the JoinResponse, which is racy through the C++ FFI bridge.
  auto channel = foxglove::RawChannel::create("/test", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());
  viewer.expect_advertise();

  // Verify the control channel still works by subscribing and logging
  auto cg_channel_id = channel->id();
  viewer.subscribe_and_wait({cg_channel_id}, [&] {
    return channel->hasSinks();
  });
  viewer.ensure_device_data_track(cg_channel_id);
  std::string payload = "ping";
  channel->log(reinterpret_cast<const std::byte*>(payload.data()), payload.size());
  auto msg = viewer.expect_new_data_track_and_message_data(cg_channel_id);
  CHECK(msg.value("op", "") == "messageData");
  auto data = msg.value("data", std::vector<uint8_t>{});
  CHECK(std::string(data.begin(), data.end()) == payload);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: connection graph subscribe without capability sends error", "[integration]") {
  auto ctx = foxglove::Context::create();

  auto gw = TestGateway::start(ctx);
  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_subscribe_connection_graph();
  auto status = viewer.expect_status();
  CHECK(status["level"].get<int>() == 2);  // Error
  auto message = status["message"].get<std::string>();
  CHECK(message.find("connection graph") != std::string::npos);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: connection graph listener callbacks", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ConnectionGraph;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_subscribe_connection_graph();
  viewer.expect_connection_graph_update();
  poll_until([&] {
    return listener.connection_graph_subscribed.load(std::memory_order_relaxed) == 1;
  });
  CHECK(listener.connection_graph_unsubscribed.load(std::memory_order_relaxed) == 0);

  viewer.send_unsubscribe_connection_graph();
  poll_until([&] {
    return listener.connection_graph_unsubscribed.load(std::memory_order_relaxed) == 1;
  });
  CHECK(listener.connection_graph_subscribed.load(std::memory_order_relaxed) == 1);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: connection graph disconnect cleans up subscription", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ConnectionGraph;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  viewer.send_subscribe_connection_graph();
  viewer.expect_connection_graph_update();
  poll_until([&] {
    return listener.connection_graph_subscribed.load(std::memory_order_relaxed) == 1;
  });

  viewer.close();
  poll_until([&] {
    return listener.connection_graph_unsubscribed.load(std::memory_order_relaxed) == 1;
  });

  gw.stop();
}

TEST_CASE("livekit: connection graph multiple subscribers", "[integration]") {
  auto ctx = foxglove::Context::create();
  MockListener listener;

  TestGatewayOptions opts;
  opts.callbacks = listener.make_callbacks();
  opts.capabilities = foxglove::RemoteAccessGatewayCapabilities::ConnectionGraph;
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer1 = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer1.expect_server_info();
  viewer1.send_subscribe_connection_graph();
  viewer1.expect_connection_graph_update();
  poll_until([&] {
    return listener.connection_graph_subscribed.load(std::memory_order_relaxed) == 1;
  });

  auto viewer2 = ViewerConnection::connect(gw.room_name, "viewer-2");
  viewer2.expect_server_info();
  viewer2.send_subscribe_connection_graph();
  viewer2.expect_connection_graph_update();

  std::this_thread::sleep_for(500ms);
  CHECK(listener.connection_graph_subscribed.load(std::memory_order_relaxed) == 1);

  foxglove::ConnectionGraph graph;
  graph.setPublishedTopic("/camera", {"node_1"});
  CHECK(gw.gateway().publishConnectionGraph(graph) == foxglove::FoxgloveError::Ok);

  auto update1 = viewer1.expect_connection_graph_update();
  auto update2 = viewer2.expect_connection_graph_update();
  CHECK(update1.value("publishedTopics", nlohmann::json::array()).size() == 1);
  CHECK(update2.value("publishedTopics", nlohmann::json::array()).size() == 1);

  viewer1.close();
  viewer2.wait_for_participant_disconnected("viewer-1");
  std::this_thread::sleep_for(200ms);
  CHECK(listener.connection_graph_unsubscribed.load(std::memory_order_relaxed) == 0);

  viewer2.close();
  poll_until([&] {
    return listener.connection_graph_unsubscribed.load(std::memory_order_relaxed) == 1;
  });

  gw.stop();
}

// ===========================================================================
// QoS classifier tests
// ===========================================================================

TEST_CASE("livekit: reliable channel delivers via control plane", "[integration]") {
  auto ctx = foxglove::Context::create();

  TestGatewayOptions opts;
  opts.qos_classifier = [](const foxglove::ChannelDescriptor& /*ch*/) {
    return foxglove::QosProfile{foxglove::Reliability::Reliable};
  };
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  auto channel = foxglove::RawChannel::create("/config", "json", std::nullopt, ctx);
  REQUIRE(channel.has_value());

  auto advertise = viewer.expect_advertise();
  REQUIRE(advertise["channels"].size() == 1);
  auto& adv_ch = advertise["channels"][0];
  auto channel_id = adv_ch["id"].get<uint64_t>();

  auto meta = adv_ch.value("metadata", nlohmann::json::object());
  CHECK(meta.value("foxglove.reliable", "") == "true");

  viewer.subscribe_and_wait({channel_id}, [&] {
    return channel->hasSinks();
  });

  std::string payload = "config-value";
  channel->log(reinterpret_cast<const std::byte*>(payload.data()), payload.size());

  // Reliable channels deliver messages on the control plane as binary
  // MessageData frames, not via a data track.
  auto msg = viewer.expect_message_data();
  CHECK(msg.value("channelId", uint64_t{0}) == channel_id);
  auto data = msg.value("data", std::vector<uint8_t>{});
  std::string received(data.begin(), data.end());
  CHECK(received == payload);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: qos classifier per channel", "[integration]") {
  auto ctx = foxglove::Context::create();

  TestGatewayOptions opts;
  opts.qos_classifier = [](const foxglove::ChannelDescriptor& ch) {
    if (std::string(ch.topic()).rfind("/config", 0) == 0) {
      return foxglove::QosProfile{foxglove::Reliability::Reliable};
    }
    return foxglove::QosProfile{};
  };
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  // Create the channels after the viewer is fully connected so the gateway
  // publishes data tracks via the SFU's real-time announce path. If the
  // channels pre-exist the viewer, their data tracks surface in the
  // JoinResponse, and the C++ FFI bridge occasionally drops the
  // `DataTrackPublished` event during the join burst.
  auto reliable_channel = foxglove::RawChannel::create("/config", "json", std::nullopt, ctx);
  REQUIRE(reliable_channel.has_value());
  auto lossy_channel = foxglove::RawChannel::create("/data", "json", std::nullopt, ctx);
  REQUIRE(lossy_channel.has_value());

  // Each channel created after the viewer connects produces its own advertise.
  uint64_t reliable_id = 0;
  uint64_t lossy_id = 0;
  for (int i = 0; i < 2; ++i) {
    auto advertise = viewer.expect_advertise();
    for (const auto& ch : advertise["channels"]) {
      auto meta = ch.value("metadata", nlohmann::json::object());
      if (ch["topic"].get<std::string>() == "/config") {
        reliable_id = ch["id"].get<uint64_t>();
        CHECK(meta.value("foxglove.reliable", "") == "true");
      } else {
        lossy_id = ch["id"].get<uint64_t>();
        CHECK(!meta.contains("foxglove.reliable"));
      }
    }
  }
  REQUIRE(reliable_id != 0);
  REQUIRE(lossy_id != 0);

  viewer.send_subscribe({reliable_id, lossy_id});
  poll_until([&] {
    return reliable_channel->hasSinks() && lossy_channel->hasSinks();
  });

  // The lossy channel should have a data track published.
  auto data_reader = viewer.expect_device_channel_data_track(lossy_id);

  // Log to the reliable channel — should arrive on the control plane.
  std::string reliable_payload = "reliable-msg";
  reliable_channel->log(
    reinterpret_cast<const std::byte*>(reliable_payload.data()), reliable_payload.size()
  );
  auto reliable_msg = viewer.expect_message_data();
  CHECK(reliable_msg.value("channelId", uint64_t{0}) == reliable_id);
  auto reliable_data = reliable_msg.value("data", std::vector<uint8_t>{});
  std::string reliable_received(reliable_data.begin(), reliable_data.end());
  CHECK(reliable_received == reliable_payload);

  // Log to the lossy channel — should arrive via the data track.
  std::string lossy_payload = "lossy-msg";
  lossy_channel->log(
    reinterpret_cast<const std::byte*>(lossy_payload.data()), lossy_payload.size()
  );
  auto lossy_msg = data_reader->next_server_message();
  CHECK(lossy_msg.value("op", "") == "messageData");
  CHECK(lossy_msg.value("channelId", uint64_t{0}) == lossy_id);
  auto lossy_data = lossy_msg.value("data", std::vector<uint8_t>{});
  CHECK(std::string(lossy_data.begin(), lossy_data.end()) == lossy_payload);

  viewer.close();
  gw.stop();
}

TEST_CASE("livekit: video channel forces lossy over reliable classifier", "[integration]") {
  auto ctx = foxglove::Context::create();

  TestGatewayOptions opts;
  opts.qos_classifier = [](const foxglove::ChannelDescriptor& /*ch*/) {
    return foxglove::QosProfile{foxglove::Reliability::Reliable};
  };
  auto gw = TestGateway::start_with_options(ctx, std::move(opts));

  auto viewer = ViewerConnection::connect(gw.room_name, "viewer-1");
  viewer.expect_server_info();

  auto video_channel = foxglove::RawChannel::create(
    "/camera", "protobuf", foxglove::Schema{"foxglove.RawImage", "protobuf", nullptr, 0}, ctx
  );
  REQUIRE(video_channel.has_value());

  auto advertise = viewer.expect_advertise();
  REQUIRE(advertise["channels"].size() == 1);
  auto& ch = advertise["channels"][0];
  CHECK(ch["id"].get<uint64_t>() == video_channel->id());

  // Video detection takes precedence: the channel is advertised as a video
  // track and NOT as reliable, even though the classifier asked for Reliable.
  auto meta = ch.value("metadata", nlohmann::json::object());
  CHECK(meta.value("foxglove.hasVideoTrack", "") == "true");
  CHECK(!meta.contains("foxglove.reliable"));

  viewer.close();
  gw.stop();
}
