use std::sync::Mutex;

use foxglove::ChannelDescriptor;
use foxglove::remote_access::{Client, Listener};

/// A mock [`Listener`] that records `on_client_advertise` and `on_client_unadvertise` callbacks.
///
/// Each entry is stored as `(client_key, topic)`.
#[derive(Default)]
pub struct MockListener {
    pub advertised: Mutex<Vec<(String, String)>>,
    pub unadvertised: Mutex<Vec<(String, String)>>,
}

impl MockListener {
    pub fn advertised(&self) -> Vec<(String, String)> {
        self.advertised.lock().unwrap().clone()
    }

    pub fn unadvertised(&self) -> Vec<(String, String)> {
        self.unadvertised.lock().unwrap().clone()
    }
}

impl Listener for MockListener {
    fn on_client_advertise(&self, client: Client, channel: &ChannelDescriptor) {
        self.advertised
            .lock()
            .unwrap()
            .push((client.client_key().to_string(), channel.topic().to_string()));
    }

    fn on_client_unadvertise(&self, client: Client, channel: &ChannelDescriptor) {
        self.unadvertised
            .lock()
            .unwrap()
            .push((client.client_key().to_string(), channel.topic().to_string()));
    }
}
