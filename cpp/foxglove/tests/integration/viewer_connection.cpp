#include "viewer_connection.hpp"

#include "livekit_token.hpp"
#include "mock_server.hpp"
#include "test_helpers.hpp"

#include <cstring>
#include <stdexcept>

namespace foxglove_integration {

// FrameReader

FrameReader::FrameReader(std::shared_ptr<livekit::ByteStreamReader> reader)
    : reader_(std::move(reader)) {}

Frame FrameReader::next_frame() {
  auto deadline = std::chrono::steady_clock::now() + READ_TIMEOUT;
  while (true) {
    auto result = try_parse_frame(buf_.data(), buf_.size());
    if (result) {
      buf_.erase(buf_.begin(), buf_.begin() + static_cast<ptrdiff_t>(result->bytes_consumed));
      return std::move(result->frame);
    }
    if (std::chrono::steady_clock::now() >= deadline) {
      throw std::runtime_error("timeout reading byte stream frame");
    }
    std::vector<uint8_t> chunk;
    if (!reader_->readNext(chunk)) {
      throw std::runtime_error("byte stream ended unexpectedly");
    }
    buf_.insert(buf_.end(), chunk.begin(), chunk.end());
  }
}

nlohmann::json FrameReader::next_server_message() {
  auto frame = next_frame();
  if (frame.op_code == OpCode::Text) {
    auto json_str = std::string(frame.payload.begin(), frame.payload.end());
    return nlohmann::json::parse(json_str);
  }
  // Binary frames: build a JSON object with the binary payload info.
  if (frame.payload.empty()) {
    throw std::runtime_error("empty binary frame");
  }
  nlohmann::json msg;
  msg["_binary"] = true;
  msg["_opcode"] = frame.payload[0];

  // Parse known binary message types
  uint8_t bin_op = frame.payload[0];
  // v2 MessageData binary: opcode(1) + channel_id(u64) + log_time(u64) + data
  if (bin_op == 1 && frame.payload.size() >= 17) {
    uint64_t channel_id = 0;
    std::memcpy(&channel_id, frame.payload.data() + 1, 8);
    uint64_t timestamp = 0;
    std::memcpy(&timestamp, frame.payload.data() + 9, 8);
    msg["op"] = "messageData";
    msg["channelId"] = channel_id;
    msg["timestamp"] = timestamp;
    msg["data"] = std::vector<uint8_t>(frame.payload.begin() + 17, frame.payload.end());
  }
  return msg;
}

// TestRoomDelegate

void TestRoomDelegate::onTrackSubscribed(
  livekit::Room& /*room*/, const livekit::TrackSubscribedEvent& event
) {
  ViewerEvent ve;
  ve.type = ViewerEvent::Type::TrackSubscribed;
  ve.track_name = event.publication->name();
  push_event(std::move(ve));
}

void TestRoomDelegate::onTrackUnsubscribed(
  livekit::Room& /*room*/, const livekit::TrackUnsubscribedEvent& event
) {
  ViewerEvent ve;
  ve.type = ViewerEvent::Type::TrackUnsubscribed;
  ve.track_name = event.publication->name();
  push_event(std::move(ve));
}

void TestRoomDelegate::onParticipantDisconnected(
  livekit::Room& /*room*/, const livekit::ParticipantDisconnectedEvent& event
) {
  ViewerEvent ve;
  ve.type = ViewerEvent::Type::ParticipantDisconnected;
  ve.identity = event.participant->identity();
  push_event(std::move(ve));
}

void TestRoomDelegate::push_event(ViewerEvent event) {
  {
    std::lock_guard<std::mutex> lock(mutex_);
    events_.push_back(std::move(event));
  }
  cv_.notify_all();
}

std::optional<ViewerEvent> TestRoomDelegate::wait_for_event(
  const std::function<bool(const ViewerEvent&)>& predicate, std::chrono::milliseconds timeout
) {
  std::unique_lock<std::mutex> lock(mutex_);
  auto deadline = std::chrono::steady_clock::now() + timeout;
  while (true) {
    for (auto it = events_.begin(); it != events_.end(); ++it) {
      if (predicate(*it)) {
        auto event = std::move(*it);
        events_.erase(it);
        return event;
      }
    }
    if (cv_.wait_until(lock, deadline) == std::cv_status::timeout) {
      return std::nullopt;
    }
  }
}

// ViewerConnection

ViewerConnection::ViewerConnection(
  std::unique_ptr<livekit::Room> room, std::shared_ptr<TestRoomDelegate> delegate,
  FrameReader control_reader
)
    : room_(std::move(room))
    , delegate_(std::move(delegate))
    , control_reader_(std::move(control_reader)) {}

ViewerConnection ViewerConnection::connect(
  const std::string& room_name, const std::string& identity
) {
  auto outer_deadline = std::chrono::steady_clock::now() + EVENT_TIMEOUT;

  while (true) {
    auto token = generate_token(room_name, identity);
    auto delegate = std::make_shared<TestRoomDelegate>();
    auto room = std::make_unique<livekit::Room>();
    room->setDelegate(delegate.get());

    auto delegate_weak = std::weak_ptr<TestRoomDelegate>(delegate);
    room->registerByteStreamHandler(
      "control",
      [delegate_weak](
        std::shared_ptr<livekit::ByteStreamReader> reader,
        const std::string& participant_identity
      ) {
        if (auto d = delegate_weak.lock()) {
          ViewerEvent ve;
          ve.type = ViewerEvent::Type::ByteStreamOpened;
          ve.topic = reader->info().topic;
          ve.identity = participant_identity;
          ve.reader = std::move(reader);
          d->push_event(std::move(ve));
        }
      }
    );

    livekit::RoomOptions options;
    options.auto_subscribe = true;
    bool connected = room->Connect(livekit_url(), token, options);
    if (!connected) {
      throw std::runtime_error("viewer Room::Connect() returned false for " + identity);
    }

    auto inner_timeout = std::min(
      CONNECT_RETRY_TIMEOUT,
      std::chrono::duration_cast<std::chrono::seconds>(
        outer_deadline - std::chrono::steady_clock::now()
      )
    );

    auto event = delegate->wait_for_event(
      [](const ViewerEvent& e) {
        return e.type == ViewerEvent::Type::ByteStreamOpened && e.topic == "control";
      },
      std::chrono::duration_cast<std::chrono::milliseconds>(inner_timeout)
    );

    if (event) {
      FrameReader reader(event->reader);
      return ViewerConnection(std::move(room), std::move(delegate), std::move(reader));
    }

    // Gateway not ready yet - destroy room and retry
    room.reset();
    if (std::chrono::steady_clock::now() >= outer_deadline) {
      throw std::runtime_error("timeout waiting for gateway to open byte stream");
    }
  }
}

nlohmann::json ViewerConnection::expect_server_info() {
  auto msg = control_reader_.next_server_message();
  if (msg.value("op", "") != "serverInfo") {
    throw std::runtime_error("expected serverInfo, got: " + msg.dump());
  }
  return msg;
}

nlohmann::json ViewerConnection::expect_advertise() {
  auto msg = control_reader_.next_server_message();
  if (msg.value("op", "") != "advertise") {
    throw std::runtime_error("expected advertise, got: " + msg.dump());
  }
  return msg;
}

nlohmann::json ViewerConnection::expect_unadvertise() {
  auto msg = control_reader_.next_server_message();
  if (msg.value("op", "") != "unadvertise") {
    throw std::runtime_error("expected unadvertise, got: " + msg.dump());
  }
  return msg;
}

nlohmann::json ViewerConnection::expect_status() {
  auto msg = control_reader_.next_server_message();
  if (msg.value("op", "") != "status") {
    throw std::runtime_error("expected status, got: " + msg.dump());
  }
  return msg;
}

nlohmann::json ViewerConnection::expect_message_data() {
  auto msg = control_reader_.next_server_message();
  if (msg.value("op", "") != "messageData") {
    throw std::runtime_error("expected messageData, got: " + msg.dump());
  }
  return msg;
}

nlohmann::json ViewerConnection::expect_connection_graph_update() {
  auto msg = control_reader_.next_server_message();
  if (msg.value("op", "") != "connectionGraphUpdate") {
    throw std::runtime_error("expected connectionGraphUpdate, got: " + msg.dump());
  }
  return msg;
}

nlohmann::json ViewerConnection::next_server_message() {
  return control_reader_.next_server_message();
}

void ViewerConnection::send_framed_text(const std::string& json) {
  auto framed = frame_text_message(json);
  if (!control_writer_) {
    control_writer_ = std::make_unique<livekit::ByteStreamWriter>(
      *room_->localParticipant(), "unused", "control",
      std::map<std::string, std::string>{}, "", std::nullopt, "application/octet-stream",
      std::vector<std::string>{TEST_DEVICE_ID}
    );
  }
  control_writer_->write(framed);
}

void ViewerConnection::send_subscribe(const std::vector<uint64_t>& channel_ids) {
  ensure_device_ch_handlers();
  nlohmann::json channels = nlohmann::json::array();
  for (auto id : channel_ids) {
    channels.push_back({{"id", id}});
  }
  nlohmann::json msg = {{"op", "subscribe"}, {"channels", channels}};
  send_framed_text(msg.dump());
}

void ViewerConnection::send_subscribe_video(const std::vector<uint64_t>& channel_ids) {
  ensure_device_ch_handlers();
  nlohmann::json channels = nlohmann::json::array();
  for (auto id : channel_ids) {
    channels.push_back({{"id", id}, {"requestVideoTrack", true}});
  }
  nlohmann::json msg = {{"op", "subscribe"}, {"channels", channels}};
  send_framed_text(msg.dump());
}

void ViewerConnection::subscribe_and_wait(
  const std::vector<uint64_t>& channel_ids, const std::function<bool()>& has_sinks
) {
  send_subscribe(channel_ids);
  poll_until(has_sinks);
}

void ViewerConnection::subscribe_video_and_wait(
  const std::vector<uint64_t>& channel_ids, const std::function<bool()>& has_sinks
) {
  send_subscribe_video(channel_ids);
  poll_until(has_sinks);
}

void ViewerConnection::send_unsubscribe(const std::vector<uint64_t>& channel_ids) {
  nlohmann::json ids = nlohmann::json::array();
  for (auto id : channel_ids) {
    ids.push_back(id);
  }
  nlohmann::json msg = {{"op", "unsubscribe"}, {"channelIds", ids}};
  send_framed_text(msg.dump());
}

void ViewerConnection::send_client_advertise(
  const std::vector<std::tuple<uint32_t, std::string, std::string>>& channels
) {
  nlohmann::json ch_arr = nlohmann::json::array();
  for (const auto& [id, topic, encoding] : channels) {
    ch_arr.push_back({
      {"id", id},
      {"topic", topic},
      {"encoding", encoding},
      {"schemaName", ""},
    });
  }
  nlohmann::json msg = {{"op", "advertise"}, {"channels", ch_arr}};
  send_framed_text(msg.dump());
}

void ViewerConnection::send_client_unadvertise(const std::vector<uint32_t>& channel_ids) {
  nlohmann::json ids = nlohmann::json::array();
  for (auto id : channel_ids) {
    ids.push_back(id);
  }
  nlohmann::json msg = {{"op", "unadvertise"}, {"channelIds", ids}};
  send_framed_text(msg.dump());
}

void ViewerConnection::send_client_message_data(
  uint32_t channel_id, const std::vector<uint8_t>& data
) {
  // ClientMessageData binary format: opcode 1 + channel_id u32 LE + data
  std::vector<uint8_t> inner;
  inner.push_back(1);
  inner.push_back(static_cast<uint8_t>(channel_id & 0xFF));
  inner.push_back(static_cast<uint8_t>((channel_id >> 8) & 0xFF));
  inner.push_back(static_cast<uint8_t>((channel_id >> 16) & 0xFF));
  inner.push_back(static_cast<uint8_t>((channel_id >> 24) & 0xFF));
  inner.insert(inner.end(), data.begin(), data.end());

  auto framed = frame_binary_message(inner.data(), inner.size());

  auto topic = "client-ch-" + std::to_string(channel_id);
  livekit::ByteStreamWriter writer(
    *room_->localParticipant(), "unused", topic, std::map<std::string, std::string>{}, "",
    std::nullopt, "application/octet-stream", std::vector<std::string>{TEST_DEVICE_ID}
  );
  writer.write(framed);
  writer.close();
}

void ViewerConnection::send_subscribe_connection_graph() {
  send_framed_text(R"({"op":"subscribeConnectionGraph"})");
}

void ViewerConnection::send_unsubscribe_connection_graph() {
  send_framed_text(R"({"op":"unsubscribeConnectionGraph"})");
}

FrameReader ViewerConnection::expect_channel_byte_stream() {
  // Per-channel byte streams have topics like "device-ch-{subscription_id}".
  // We register handlers dynamically and poll for events from them.
  // Register a batch of handlers for potential subscription IDs if not already done.
  ensure_device_ch_handlers();

  auto event = delegate_->wait_for_event(
    [](const ViewerEvent& e) {
      return e.type == ViewerEvent::Type::ByteStreamOpened &&
             e.topic.find("device-ch-") == 0;
    },
    std::chrono::duration_cast<std::chrono::milliseconds>(READ_TIMEOUT)
  );
  if (!event) {
    throw std::runtime_error("timeout waiting for channel byte stream");
  }
  return FrameReader(event->reader);
}

void ViewerConnection::ensure_device_ch_handlers() {
  if (device_ch_handlers_registered_) {
    return;
  }
  device_ch_handlers_registered_ = true;

  auto delegate_weak = std::weak_ptr<TestRoomDelegate>(delegate_);
  // Register handlers for subscription IDs 0-99 to cover typical test scenarios.
  for (int i = 0; i < 100; ++i) {
    auto topic = "device-ch-" + std::to_string(i);
    room_->registerByteStreamHandler(
      topic,
      [delegate_weak, topic](
        std::shared_ptr<livekit::ByteStreamReader> reader,
        const std::string& participant_identity
      ) {
        if (auto d = delegate_weak.lock()) {
          ViewerEvent ve;
          ve.type = ViewerEvent::Type::ByteStreamOpened;
          ve.topic = topic;
          ve.identity = participant_identity;
          ve.reader = std::move(reader);
          d->push_event(std::move(ve));
        }
      }
    );
  }
}

nlohmann::json ViewerConnection::expect_new_bytestream_and_message_data() {
  auto reader = expect_channel_byte_stream();
  auto msg = reader.next_server_message();
  if (msg.value("op", "") != "messageData") {
    throw std::runtime_error("expected messageData on channel stream, got: " + msg.dump());
  }
  return msg;
}

std::string ViewerConnection::expect_track_subscribed() {
  auto event = delegate_->wait_for_event(
    [](const ViewerEvent& e) { return e.type == ViewerEvent::Type::TrackSubscribed; },
    std::chrono::duration_cast<std::chrono::milliseconds>(EVENT_TIMEOUT)
  );
  if (!event) {
    throw std::runtime_error("timeout waiting for TrackSubscribed event");
  }
  return event->track_name;
}

std::string ViewerConnection::expect_track_unsubscribed() {
  auto event = delegate_->wait_for_event(
    [](const ViewerEvent& e) { return e.type == ViewerEvent::Type::TrackUnsubscribed; },
    std::chrono::duration_cast<std::chrono::milliseconds>(EVENT_TIMEOUT)
  );
  if (!event) {
    throw std::runtime_error("timeout waiting for TrackUnsubscribed event");
  }
  return event->track_name;
}

void ViewerConnection::wait_for_participant_disconnected(const std::string& identity) {
  auto event = delegate_->wait_for_event(
    [&identity](const ViewerEvent& e) {
      return e.type == ViewerEvent::Type::ParticipantDisconnected && e.identity == identity;
    },
    std::chrono::duration_cast<std::chrono::milliseconds>(EVENT_TIMEOUT)
  );
  if (!event) {
    throw std::runtime_error("timeout waiting for participant disconnected: " + identity);
  }
}

void ViewerConnection::close() {
  if (control_writer_) {
    control_writer_->close();
    control_writer_.reset();
  }
  if (room_) {
    room_.reset();
  }
}

}  // namespace foxglove_integration
