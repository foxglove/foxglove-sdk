#include "viewer_connection.hpp"

#include <livekit/data_track_stream.h>
#include <livekit/remote_data_track.h>

#include <chrono>
#include <future>
#include <stdexcept>
#include <thread>

#include "livekit_token.hpp"
#include "mock_server.hpp"
#include "test_helpers.hpp"

namespace foxglove_integration {

// FrameReader

FrameReader::FrameReader(std::shared_ptr<livekit::ByteStreamReader> reader)
    : reader_(std::move(reader)) {}

namespace {

/// Reads the next chunk from a ByteStreamReader with a timeout by running
/// readNext() on a worker thread. ByteStreamReader has no close() method and
/// the underlying readNext() blocks unconditionally on a condition variable
/// until a chunk arrives or the SFU signals end-of-stream, so on timeout we
/// have no way to wake the worker. We detach it instead, leaking the OS
/// thread until process exit, and throw so the test fails cleanly rather
/// than wedging on the join. The detached lambda owns `reader` (shared_ptr)
/// and `promise` (moved), so nothing in the parent scope dangles.
std::vector<uint8_t> read_byte_stream_chunk_with_timeout(
  const std::shared_ptr<livekit::ByteStreamReader>& reader, std::chrono::milliseconds timeout
) {
  std::promise<std::vector<uint8_t>> chunk_promise;
  auto future = chunk_promise.get_future();

  std::thread worker([reader, promise = std::move(chunk_promise)]() mutable {
    try {
      std::vector<uint8_t> chunk;
      if (!reader->readNext(chunk)) {
        throw std::runtime_error("byte stream ended unexpectedly");
      }
      promise.set_value(std::move(chunk));
    } catch (...) {
      promise.set_exception(std::current_exception());
    }
  });

  auto status = future.wait_for(timeout);
  if (status != std::future_status::ready) {
    worker.detach();
    throw std::runtime_error("timeout reading byte stream chunk");
  }

  worker.join();
  return future.get();
}
}  // namespace

ByteStreamFrame FrameReader::next_frame() {
  auto deadline = std::chrono::steady_clock::now() + READ_TIMEOUT;
  while (true) {
    auto result = try_parse_bytestream_frame(buf_.data(), buf_.size());
    if (result) {
      buf_.erase(buf_.begin(), buf_.begin() + static_cast<ptrdiff_t>(result->bytes_consumed));
      return std::move(result->frame);
    }
    auto now = std::chrono::steady_clock::now();
    if (now >= deadline) {
      throw std::runtime_error("timeout reading byte stream frame");
    }
    auto remaining = std::chrono::duration_cast<std::chrono::milliseconds>(deadline - now);
    auto chunk = read_byte_stream_chunk_with_timeout(reader_, remaining);
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

  uint8_t bin_op = frame.payload[0];
  // v2 MessageData binary: opcode(1) + channel_id(u64 LE) + log_time(u64 LE) + data
  if (bin_op == 1 && frame.payload.size() >= 17) {
    uint64_t channel_id = read_u64_le(frame.payload.data() + 1);
    uint64_t timestamp = read_u64_le(frame.payload.data() + 9);
    msg["op"] = "messageData";
    msg["channelId"] = channel_id;
    msg["timestamp"] = timestamp;
    msg["data"] = std::vector<uint8_t>(frame.payload.begin() + 17, frame.payload.end());
  }
  return msg;
}

// DeviceChannelReader

namespace {
/// Bounded grace period to wait for read() to unblock after close() is called.
/// If the worker hasn't returned by then, we assume close() failed to wake it
/// (FFI bug, half-closed stream, race with the SFU, etc.) and detach the
/// thread so the test fails instead of hanging the runner.
constexpr auto CLOSE_GRACE = std::chrono::seconds(2);

/// Reads the next DataTrackFrame from a DataTrackStream with a timeout by
/// running read() on a worker thread and closing the stream if it doesn't
/// complete in time. If close() fails to unblock read() within CLOSE_GRACE,
/// the worker is detached (the lambda owns `stream` and `promise`, so the
/// detached thread keeps everything it touches alive) and a runtime_error
/// is thrown so the test fails cleanly. The OS thread leaks until process
/// exit, which is acceptable for an integration test runner.
livekit::DataTrackFrame read_data_track_frame_with_timeout(
  const std::shared_ptr<livekit::DataTrackStream>& stream, std::chrono::milliseconds timeout
) {
  std::promise<livekit::DataTrackFrame> frame_promise;
  auto future = frame_promise.get_future();

  std::thread reader([stream, promise = std::move(frame_promise)]() mutable {
    try {
      livekit::DataTrackFrame frame;
      if (!stream->read(frame)) {
        throw std::runtime_error("data track stream ended before a frame arrived");
      }
      promise.set_value(std::move(frame));
    } catch (...) {
      promise.set_exception(std::current_exception());
    }
  });

  if (future.wait_for(timeout) != std::future_status::ready) {
    stream->close();
    if (future.wait_for(CLOSE_GRACE) != std::future_status::ready) {
      reader.detach();
      throw std::runtime_error("data track stream did not respond to close() within grace period");
    }
  }

  reader.join();
  return future.get();
}
}  // namespace

DeviceChannelReader::DeviceChannelReader(
  std::shared_ptr<livekit::DataTrackStream> stream, uint64_t channel_id
)
    : stream_(std::move(stream))
    , channel_id_(channel_id) {}

nlohmann::json DeviceChannelReader::next_server_message() {
  auto frame = read_data_track_frame_with_timeout(
    stream_, std::chrono::duration_cast<std::chrono::milliseconds>(READ_TIMEOUT)
  );
  if (frame.payload.size() < 4) {
    throw std::runtime_error(
      "data track frame too small (" + std::to_string(frame.payload.size()) + " bytes)"
    );
  }
  uint16_t data_offset = read_u16_le(frame.payload.data() + 2);
  if (frame.payload.size() < data_offset) {
    throw std::runtime_error(
      "data track frame size (" + std::to_string(frame.payload.size()) +
      " bytes) is smaller than data_offset (" + std::to_string(data_offset) + ")"
    );
  }
  nlohmann::json msg;
  msg["op"] = "messageData";
  msg["channelId"] = channel_id_;
  msg["timestamp"] = frame.user_timestamp.value_or(0);
  msg["data"] = std::vector<uint8_t>(frame.payload.begin() + data_offset, frame.payload.end());
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

void TestRoomDelegate::onDataTrackPublished(
  livekit::Room& /*room*/, const livekit::DataTrackPublishedEvent& event
) {
  if (!event.track) {
    return;
  }
  ViewerEvent ve;
  ve.type = ViewerEvent::Type::DataTrackPublished;
  ve.track_name = event.track->info().name;
  ve.data_track = event.track;
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
  // The C++ LiveKit SDK's Room::Connect() registers its FFI event listener
  // AFTER the connection completes. Events emitted by the Rust side between
  // connection establishment and listener registration are dropped. If the
  // gateway sends the control byte-stream header during that window, the
  // ByteStreamOpened event is lost. Retry with a short inner timeout so we can
  // reconnect and catch the byte stream on a subsequent attempt. The byte
  // stream header typically arrives 2-3s after Room::Connect() returns, so
  // the inner timeout must exceed that.
  constexpr auto INNER_TIMEOUT = std::chrono::seconds(5);
  constexpr auto CONNECT_TIMEOUT = std::chrono::seconds(15);
  auto outer_deadline = std::chrono::steady_clock::now() + CONNECT_TIMEOUT;

  while (true) {
    auto token = generate_token(room_name, identity);
    auto delegate = std::make_shared<TestRoomDelegate>();
    auto room = std::make_unique<livekit::Room>();
    room->setDelegate(delegate.get());

    auto delegate_weak = std::weak_ptr<TestRoomDelegate>(delegate);
    room->registerByteStreamHandler(
      "control",
      [delegate_weak](
        std::shared_ptr<livekit::ByteStreamReader> reader, const std::string& participant_identity
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
    if (!room->Connect(livekit_url(), token, options)) {
      throw std::runtime_error("viewer Room::Connect() returned false for " + identity);
    }

    auto now = std::chrono::steady_clock::now();
    auto inner_deadline = std::min(now + INNER_TIMEOUT, outer_deadline);
    auto wait_ms = std::chrono::duration_cast<std::chrono::milliseconds>(inner_deadline - now);

    auto event = delegate->wait_for_event(
      [](const ViewerEvent& e) {
        return e.type == ViewerEvent::Type::ByteStreamOpened && e.topic == "control";
      },
      wait_ms
    );

    if (event) {
      FrameReader reader(event->reader);
      return ViewerConnection(std::move(room), std::move(delegate), std::move(reader));
    }

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

void ViewerConnection::ensure_control_writer() {
  if (!control_writer_) {
    control_writer_ = std::make_unique<livekit::ByteStreamWriter>(
      *room_->localParticipant(),
      "unused",
      "control",
      std::map<std::string, std::string>{},
      "",
      std::nullopt,
      "application/octet-stream",
      std::vector<std::string>{TEST_DEVICE_ID}
    );
  }
}

void ViewerConnection::send_framed_text(const std::string& json) {
  ensure_control_writer();
  auto framed = bytestream_frame_text_message(json);
  control_writer_->write(framed);
}

void ViewerConnection::send_framed_binary(const std::vector<uint8_t>& data) {
  ensure_control_writer();
  auto framed = bytestream_frame_binary_message(data.data(), data.size());
  control_writer_->write(framed);
}

void ViewerConnection::send_subscribe(const std::vector<uint64_t>& channel_ids) {
  nlohmann::json channels = nlohmann::json::array();
  for (auto id : channel_ids) {
    channels.push_back({{"id", id}});
  }
  nlohmann::json msg = {{"op", "subscribe"}, {"channels", channels}};
  send_framed_text(msg.dump());
}

void ViewerConnection::send_subscribe_video(const std::vector<uint64_t>& channel_ids) {
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

void ViewerConnection::send_client_advertise(const std::vector<ClientChannelDesc>& channels) {
  nlohmann::json ch_arr = nlohmann::json::array();
  for (const auto& ch : channels) {
    ch_arr.push_back({
      {"id", ch.id},
      {"topic", ch.topic},
      {"encoding", ch.encoding},
      {"schemaName", ch.schema_name},
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
  // v2 ClientMessageData binary framing: opcode(1) + channel_id(u32 LE) + data
  std::vector<uint8_t> inner;
  inner.push_back(1);
  inner.push_back(static_cast<uint8_t>(channel_id & 0xFF));
  inner.push_back(static_cast<uint8_t>((channel_id >> 8) & 0xFF));
  inner.push_back(static_cast<uint8_t>((channel_id >> 16) & 0xFF));
  inner.push_back(static_cast<uint8_t>((channel_id >> 24) & 0xFF));
  inner.insert(inner.end(), data.begin(), data.end());

  send_framed_binary(inner);
}

void ViewerConnection::send_subscribe_connection_graph() {
  send_framed_text(R"({"op":"subscribeConnectionGraph"})");
}

void ViewerConnection::send_unsubscribe_connection_graph() {
  send_framed_text(R"({"op":"unsubscribeConnectionGraph"})");
}

std::shared_ptr<DeviceChannelReader> ViewerConnection::expect_device_channel_data_track(
  uint64_t channel_id
) {
  auto expected_name = "data-ch-" + std::to_string(channel_id);
  auto event = delegate_->wait_for_event(
    [&expected_name](const ViewerEvent& e) {
      return e.type == ViewerEvent::Type::DataTrackPublished && e.track_name == expected_name;
    },
    std::chrono::duration_cast<std::chrono::milliseconds>(DATA_TRACK_PUBLISH_TIMEOUT)
  );
  if (!event) {
    throw std::runtime_error("timeout waiting for device channel data track: " + expected_name);
  }
  auto sub = event->data_track->subscribe();
  if (!sub) {
    throw std::runtime_error(
      "failed to subscribe to data track " + expected_name + ": " + sub.error().message
    );
  }
  // The C++ subscribe() returns immediately, but the underlying Rust FFI
  // subscription is asynchronous: the SFU must add a data downtrack and send
  // `DataTrackSubscriberHandles` back before any frames will be routed. If we
  // publish a frame from the SDK before that handshake completes, the SFU has
  // no downtrack and silently drops the packet. Sleep briefly to let the
  // subscription activate. The Rust test helper avoids this race with
  // `subscribe().await`, but the C++ FFI does not currently expose an
  // "active" signal.
  std::this_thread::sleep_for(std::chrono::milliseconds(250));
  return std::make_shared<DeviceChannelReader>(sub.value(), channel_id);
}

void ViewerConnection::ensure_device_data_track(uint64_t channel_id) {
  if (device_channel_readers_.count(channel_id) > 0) {
    return;
  }
  auto reader = expect_device_channel_data_track(channel_id);
  device_channel_readers_.emplace(channel_id, std::move(reader));
}

nlohmann::json ViewerConnection::expect_new_data_track_and_message_data(uint64_t channel_id) {
  ensure_device_data_track(channel_id);
  return device_channel_readers_.at(channel_id)->next_server_message();
}

bool ViewerConnection::has_device_data_track(
  uint64_t channel_id, std::chrono::milliseconds timeout
) {
  auto expected_name = "data-ch-" + std::to_string(channel_id);
  auto event = delegate_->wait_for_event(
    [&expected_name](const ViewerEvent& e) {
      return e.type == ViewerEvent::Type::DataTrackPublished && e.track_name == expected_name;
    },
    timeout
  );
  return event.has_value();
}

std::string ViewerConnection::expect_track_subscribed() {
  auto event = delegate_->wait_for_event(
    [](const ViewerEvent& e) {
      return e.type == ViewerEvent::Type::TrackSubscribed;
    },
    std::chrono::duration_cast<std::chrono::milliseconds>(EVENT_TIMEOUT)
  );
  if (!event) {
    throw std::runtime_error("timeout waiting for TrackSubscribed event");
  }
  return event->track_name;
}

std::string ViewerConnection::expect_track_unsubscribed() {
  auto event = delegate_->wait_for_event(
    [](const ViewerEvent& e) {
      return e.type == ViewerEvent::Type::TrackUnsubscribed;
    },
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

}  // namespace foxglove_integration
