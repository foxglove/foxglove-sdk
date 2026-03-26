use std::sync::Mutex;

use foxglove::ChannelDescriptor;
use foxglove::remote_access::{Client, Listener};

/// A mock [`Listener`] that records `on_client_advertise`, `on_client_unadvertise`,
/// and `on_message_data` callbacks.
///
/// Advertise/unadvertise entries are stored as `(participant_id, topic)`.
/// Message data entries are stored as `(client_id, topic, payload)`.
#[derive(Default)]
pub struct MockListener {
    pub advertised: Mutex<Vec<(String, String)>>,
    pub unadvertised: Mutex<Vec<(String, String)>>,
    pub message_data: Mutex<Vec<(String, String, Vec<u8>)>>,
}

impl MockListener {
    pub fn advertised(&self) -> Vec<(String, String)> {
        self.advertised.lock().unwrap().clone()
    }

    pub fn unadvertised(&self) -> Vec<(String, String)> {
        self.unadvertised.lock().unwrap().clone()
    }

    pub fn message_data(&self) -> Vec<(String, String, Vec<u8>)> {
        self.message_data.lock().unwrap().clone()
    }
}

impl Listener for MockListener {
    fn on_client_advertise(&self, client: Client, channel: &ChannelDescriptor) {
        self.advertised.lock().unwrap().push((
            client.participant_id().to_string(),
            channel.topic().to_string(),
        ));
    }

    fn on_client_unadvertise(&self, client: Client, channel: &ChannelDescriptor) {
        self.unadvertised.lock().unwrap().push((
            client.participant_id().to_string(),
            channel.topic().to_string(),
        ));
    }

    fn on_message_data(&self, client: Client, channel: &ChannelDescriptor, payload: &[u8]) {
        self.message_data.lock().unwrap().push((
            client.participant_id().to_string(),
            channel.topic().to_string(),
            payload.to_vec(),
        ));
    }
}
