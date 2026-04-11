#pragma once

#include "frame.hpp"

#include <livekit/livekit.h>

#include <nlohmann/json.hpp>

#include <chrono>
#include <condition_variable>
#include <deque>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

namespace foxglove_integration {

/// Reads chunks from a LiveKit byte stream and parses byte stream frames.
class FrameReader {
public:
  explicit FrameReader(std::shared_ptr<livekit::ByteStreamReader> reader);

  /// Reads chunks until a complete frame is available and returns it.
  /// Blocks up to READ_TIMEOUT.
  Frame next_frame();

  /// Reads the next frame and parses it as a JSON message.
  nlohmann::json next_server_message();

private:
  std::shared_ptr<livekit::ByteStreamReader> reader_;
  std::vector<uint8_t> buf_;
};

/// Events pushed to a thread-safe queue for test consumption.
struct ViewerEvent {
  enum class Type {
    ByteStreamOpened,
    TrackSubscribed,
    TrackUnsubscribed,
    ParticipantDisconnected,
    RoomEos,
  };
  Type type;
  std::string topic;
  std::string identity;
  std::string track_name;
  std::shared_ptr<livekit::ByteStreamReader> reader;
};

/// RoomDelegate that pushes track and participant events into a queue.
class TestRoomDelegate : public livekit::RoomDelegate {
public:
  void onTrackSubscribed(livekit::Room& room, const livekit::TrackSubscribedEvent& event) override;
  void onTrackUnsubscribed(livekit::Room& room, const livekit::TrackUnsubscribedEvent& event)
    override;
  void onParticipantDisconnected(
    livekit::Room& room, const livekit::ParticipantDisconnectedEvent& event
  ) override;
  void onRoomEos(livekit::Room& room, const livekit::RoomEosEvent& event) override;

  /// Wait for an event matching the predicate, up to the given timeout.
  std::optional<ViewerEvent> wait_for_event(
    const std::function<bool(const ViewerEvent&)>& predicate,
    std::chrono::milliseconds timeout
  );

  /// Push an event from an external source (e.g. byte stream handler).
  void push_event(ViewerEvent event);

private:
  std::mutex mutex_;
  std::condition_variable cv_;
  std::deque<ViewerEvent> events_;
};

/// A viewer connected to a LiveKit room with an open control channel byte stream.
class ViewerConnection {
public:
  /// Connects a viewer to the LiveKit room and waits for the control channel
  /// byte stream to open. Retries if the gateway hasn't joined yet.
  static ViewerConnection connect(const std::string& room_name, const std::string& identity);

  /// Reads and validates the initial ServerInfo message.
  nlohmann::json expect_server_info();

  /// Reads and returns the next Advertise message.
  nlohmann::json expect_advertise();

  /// Reads and returns the next Unadvertise message.
  nlohmann::json expect_unadvertise();

  /// Reads and returns the next Status message.
  nlohmann::json expect_status();

  /// Reads and returns the next MessageData from the control stream.
  nlohmann::json expect_message_data();

  /// Reads and returns the next ConnectionGraphUpdate message.
  nlohmann::json expect_connection_graph_update();

  /// Reads the next server message (any type).
  nlohmann::json next_server_message();

  /// Sends a Subscribe message for the given channel IDs.
  void send_subscribe(const std::vector<uint64_t>& channel_ids);

  /// Sends a Subscribe with video requested for the given channel IDs.
  void send_subscribe_video(const std::vector<uint64_t>& channel_ids);

  /// Sends a Subscribe and waits for the channel to have at least one sink.
  void subscribe_and_wait(
    const std::vector<uint64_t>& channel_ids, const std::function<bool()>& has_sinks
  );

  /// Sends a Subscribe with video requested and waits for the channel to have sinks.
  void subscribe_video_and_wait(
    const std::vector<uint64_t>& channel_ids, const std::function<bool()>& has_sinks
  );

  /// Sends an Unsubscribe message for the given channel IDs.
  void send_unsubscribe(const std::vector<uint64_t>& channel_ids);

  /// Sends a client Advertise message.
  void send_client_advertise(
    const std::vector<std::tuple<uint32_t, std::string, std::string>>& channels
  );

  /// Sends a client Unadvertise message.
  void send_client_unadvertise(const std::vector<uint32_t>& channel_ids);

  /// Sends a client MessageData on a per-channel topic.
  void send_client_message_data(uint32_t channel_id, const std::vector<uint8_t>& data);

  /// Sends a subscribeConnectionGraph message.
  void send_subscribe_connection_graph();

  /// Sends an unsubscribeConnectionGraph message.
  void send_unsubscribe_connection_graph();

  /// Waits for a per-channel byte stream to open and returns a FrameReader.
  FrameReader expect_channel_byte_stream();

  /// Waits for a per-channel byte stream and reads the next MessageData from it.
  nlohmann::json expect_new_bytestream_and_message_data();

  /// Waits for a TrackSubscribed event and returns the track name.
  std::string expect_track_subscribed();

  /// Waits for a TrackUnsubscribed event and returns the track name.
  std::string expect_track_unsubscribed();

  /// Waits for a ParticipantDisconnected event for the given identity.
  void wait_for_participant_disconnected(const std::string& identity);

  /// Close the viewer connection.
  void close();

private:
  ViewerConnection(
    std::unique_ptr<livekit::Room> room, std::shared_ptr<TestRoomDelegate> delegate,
    FrameReader control_reader
  );

  void send_framed_text(const std::string& json);
  void ensure_device_ch_handlers();

  std::unique_ptr<livekit::Room> room_;
  std::shared_ptr<TestRoomDelegate> delegate_;
  FrameReader control_reader_;
  std::unique_ptr<livekit::ByteStreamWriter> control_writer_;
  bool device_ch_handlers_registered_ = false;
};

}  // namespace foxglove_integration
