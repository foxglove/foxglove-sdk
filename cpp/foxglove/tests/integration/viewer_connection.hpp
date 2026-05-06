#pragma once

#include <livekit/livekit.h>
#include <nlohmann/json.hpp>

#include <chrono>
#include <condition_variable>
#include <deque>
#include <functional>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

#include "frame.hpp"

namespace foxglove_integration {

/// Reads chunks from a LiveKit byte stream and parses byte stream frames.
class FrameReader {
public:
  explicit FrameReader(std::shared_ptr<livekit::ByteStreamReader> reader);

  /// Reads chunks until a complete frame is available and returns it.
  /// Blocks up to READ_TIMEOUT.
  ByteStreamFrame next_frame();

  /// Reads the next frame and parses it as a JSON message.
  nlohmann::json next_server_message();

private:
  std::shared_ptr<livekit::ByteStreamReader> reader_;
  std::vector<uint8_t> buf_;
};

/// Reads framed data-track frames for a single device channel and exposes them
/// as JSON messages. Each frame payload currently begins with an 8-byte header (u16 LE
/// flags, u16 LE data_offset, u32 LE sequence) followed by the raw message
/// bytes. The channel identity is determined by the data track name. The header could
/// have more fields added in the future, so we always read the data_offset at offset 2
/// and add it to the message start to get the payload start.
class DeviceChannelReader {
public:
  DeviceChannelReader(std::shared_ptr<livekit::DataTrackStream> stream, uint64_t channel_id);

  /// Reads the next data-track frame and returns a JSON message shaped like
  /// the control-plane messageData: {"op":"messageData","channelId":<id>,
  /// "timestamp":<ts>,"data":[...bytes...]}. Blocks up to READ_TIMEOUT.
  nlohmann::json next_server_message();

private:
  std::shared_ptr<livekit::DataTrackStream> stream_;
  uint64_t channel_id_;
};

/// Events pushed to a thread-safe queue for test consumption.
struct ViewerEvent {
  enum class Type {
    ByteStreamOpened,
    TrackSubscribed,
    TrackUnsubscribed,
    ParticipantDisconnected,
    RoomEos,
    DataTrackPublished,
  };
  Type type;
  std::string topic;
  std::string identity;
  std::string track_name;
  std::shared_ptr<livekit::ByteStreamReader> reader;
  std::shared_ptr<livekit::RemoteDataTrack> data_track;
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
  void onDataTrackPublished(livekit::Room& room, const livekit::DataTrackPublishedEvent& event)
    override;

  /// Wait for an event matching the predicate, up to the given timeout.
  std::optional<ViewerEvent> wait_for_event(
    const std::function<bool(const ViewerEvent&)>& predicate, std::chrono::milliseconds timeout
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

  /// Description of a client-advertised channel.
  struct ClientChannelDesc {
    uint32_t id;
    std::string topic;
    std::string encoding;
    std::string schema_name;
  };

  /// Sends a client Advertise message.
  void send_client_advertise(const std::vector<ClientChannelDesc>& channels);

  /// Sends a client Unadvertise message.
  void send_client_unadvertise(const std::vector<uint32_t>& channel_ids);

  /// Sends a client MessageData on the control stream as a binary-framed v2
  /// ClientMessageData.
  void send_client_message_data(uint32_t channel_id, const std::vector<uint8_t>& data);

  /// Sends a subscribeConnectionGraph message.
  void send_subscribe_connection_graph();

  /// Sends an unsubscribeConnectionGraph message.
  void send_unsubscribe_connection_graph();

  /// Waits for a `DataTrackPublished` event whose track name matches
  /// `data-ch-{channel_id}`, subscribes to it, and returns a reader.
  std::shared_ptr<DeviceChannelReader> expect_device_channel_data_track(uint64_t channel_id);

  /// Ensures a device channel data track reader exists for `channel_id`. No-op
  /// if one already exists; otherwise waits for and subscribes to the track.
  void ensure_device_data_track(uint64_t channel_id);

  /// Ensures a data-track reader exists for the given channel, then reads the
  /// next MessageData from it. Subsequent calls reuse the same reader.
  nlohmann::json expect_new_data_track_and_message_data(uint64_t channel_id);

  /// Returns true if a DataTrackPublished event for `data-ch-{channel_id}` is
  /// seen within `timeout`. Use a short timeout for negative assertions — by
  /// the time this is called the event (if any) is already in the queue.
  /// Note: this uses wait_for_event and consumes the event from the queue.
  bool has_device_data_track(
    uint64_t channel_id, std::chrono::milliseconds timeout = std::chrono::milliseconds(500)
  );

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
  void send_framed_binary(const std::vector<uint8_t>& data);
  void ensure_control_writer();

  std::unique_ptr<livekit::Room> room_;
  std::shared_ptr<TestRoomDelegate> delegate_;
  FrameReader control_reader_;
  std::unique_ptr<livekit::ByteStreamWriter> control_writer_;
  std::map<uint64_t, std::shared_ptr<DeviceChannelReader>> device_channel_readers_;
};

}  // namespace foxglove_integration
