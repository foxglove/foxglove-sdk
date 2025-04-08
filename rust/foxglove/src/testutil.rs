//! Test utilities.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use parking_lot::Mutex;

use crate::websocket::{
    ChannelView, Client, ClientChannel, ClientChannelId, ClientId, Parameter, ServerListener,
};
use crate::ChannelId;

mod global_context;
mod sink;

pub use global_context::GlobalContextTest;
pub use sink::{ErrorSink, MockSink, RecordingSink};

#[allow(dead_code)]
pub(crate) struct ClientChannelInfo {
    pub(crate) id: ClientChannelId,
    pub(crate) topic: String,
}

impl From<&ClientChannel> for ClientChannelInfo {
    fn from(channel: &ClientChannel) -> Self {
        Self {
            id: channel.id,
            topic: channel.topic.to_string(),
        }
    }
}

pub(crate) struct ChannelInfo {
    pub(crate) id: ChannelId,
    pub(crate) topic: String,
}

impl From<ChannelView<'_>> for ChannelInfo {
    fn from(channel: ChannelView) -> Self {
        Self {
            id: channel.id(),
            topic: channel.topic().to_string(),
        }
    }
}

pub(crate) struct MessageData {
    #[allow(dead_code)]
    pub client_id: ClientId,
    pub channel: ClientChannelInfo,
    pub data: Vec<u8>,
}

pub(crate) struct GetParameters {
    #[allow(dead_code)]
    pub client_id: ClientId,
    pub param_names: Vec<String>,
    pub request_id: Option<String>,
}

pub(crate) struct SetParameters {
    #[allow(dead_code)]
    pub client_id: ClientId,
    pub parameters: Vec<Parameter>,
    pub request_id: Option<String>,
}

pub(crate) struct RecordingServerListener {
    message_data: Mutex<Vec<MessageData>>,
    subscribe: Mutex<Vec<(ClientId, ChannelInfo)>>,
    unsubscribe: Mutex<Vec<(ClientId, ChannelInfo)>>,
    client_advertise: Mutex<Vec<(ClientId, ClientChannelInfo)>>,
    client_unadvertise: Mutex<Vec<(ClientId, ClientChannelInfo)>>,
    parameters_subscribe: Mutex<Vec<Vec<String>>>,
    parameters_unsubscribe: Mutex<Vec<Vec<String>>>,
    parameters_get: Mutex<Vec<GetParameters>>,
    parameters_set: Mutex<Vec<SetParameters>>,
    parameters_get_result: Mutex<Vec<Parameter>>,
    connection_graph_subscribe: AtomicUsize,
    connection_graph_unsubscribe: AtomicUsize,
}

impl RecordingServerListener {
    pub fn new() -> Self {
        Self {
            message_data: Mutex::default(),
            subscribe: Mutex::default(),
            unsubscribe: Mutex::default(),
            client_advertise: Mutex::default(),
            client_unadvertise: Mutex::default(),
            parameters_subscribe: Mutex::default(),
            parameters_unsubscribe: Mutex::default(),
            parameters_get: Mutex::default(),
            parameters_set: Mutex::default(),
            parameters_get_result: Mutex::default(),
            connection_graph_subscribe: AtomicUsize::default(),
            connection_graph_unsubscribe: AtomicUsize::default(),
        }
    }

    pub fn message_data_len(&self) -> usize {
        self.message_data.lock().len()
    }

    pub fn client_advertise_len(&self) -> usize {
        self.client_advertise.lock().len()
    }

    pub fn client_unadvertise_len(&self) -> usize {
        self.client_unadvertise.lock().len()
    }

    pub fn parameters_subscribe_len(&self) -> usize {
        self.parameters_subscribe.lock().len()
    }

    pub fn parameters_unsubscribe_len(&self) -> usize {
        self.parameters_unsubscribe.lock().len()
    }

    pub fn take_message_data(&self) -> Vec<MessageData> {
        std::mem::take(&mut self.message_data.lock())
    }

    pub fn take_subscribe(&self) -> Vec<(ClientId, ChannelInfo)> {
        std::mem::take(&mut self.subscribe.lock())
    }

    pub fn take_unsubscribe(&self) -> Vec<(ClientId, ChannelInfo)> {
        std::mem::take(&mut self.unsubscribe.lock())
    }

    pub fn take_client_advertise(&self) -> Vec<(ClientId, ClientChannelInfo)> {
        std::mem::take(&mut self.client_advertise.lock())
    }

    pub fn take_client_unadvertise(&self) -> Vec<(ClientId, ClientChannelInfo)> {
        std::mem::take(&mut self.client_unadvertise.lock())
    }

    pub fn take_parameters_subscribe(&self) -> Vec<Vec<String>> {
        std::mem::take(&mut self.parameters_subscribe.lock())
    }

    pub fn take_parameters_unsubscribe(&self) -> Vec<Vec<String>> {
        std::mem::take(&mut self.parameters_unsubscribe.lock())
    }

    pub fn take_parameters_get(&self) -> Vec<GetParameters> {
        std::mem::take(&mut self.parameters_get.lock())
    }

    pub fn set_parameters_get_result(&self, result: Vec<Parameter>) {
        *self.parameters_get_result.lock() = result;
    }

    pub fn take_parameters_set(&self) -> Vec<SetParameters> {
        std::mem::take(&mut self.parameters_set.lock())
    }

    fn inc_connection_graph_subscribe(&self) {
        self.connection_graph_subscribe
            .fetch_add(1, Ordering::AcqRel);
    }

    pub fn take_connection_graph_subscribe(&self) -> usize {
        self.connection_graph_subscribe.swap(0, Ordering::AcqRel)
    }

    fn inc_connection_graph_unsubscribe(&self) {
        self.connection_graph_unsubscribe
            .fetch_add(1, Ordering::AcqRel);
    }

    pub fn take_connection_graph_unsubscribe(&self) -> usize {
        self.connection_graph_unsubscribe.swap(0, Ordering::AcqRel)
    }
}

impl ServerListener for RecordingServerListener {
    fn on_message_data(&self, client: Client, channel: &ClientChannel, payload: &[u8]) {
        let mut data = self.message_data.lock();
        data.push(MessageData {
            client_id: client.id(),
            channel: channel.into(),
            data: payload.to_vec(),
        });
    }

    fn on_subscribe(&self, client: Client, channel: ChannelView) {
        let mut subs = self.subscribe.lock();
        subs.push((client.id(), channel.into()));
    }

    fn on_unsubscribe(&self, client: Client, channel: ChannelView) {
        let mut unsubs = self.unsubscribe.lock();
        unsubs.push((client.id(), channel.into()));
    }

    fn on_client_advertise(&self, client: Client, channel: &ClientChannel) {
        let mut adverts = self.client_advertise.lock();
        adverts.push((client.id(), channel.into()));
    }

    fn on_client_unadvertise(&self, client: Client, channel: &ClientChannel) {
        let mut unadverts = self.client_unadvertise.lock();
        unadverts.push((client.id(), channel.into()));
    }

    fn on_get_parameters(
        &self,
        client: Client,
        param_names: Vec<String>,
        request_id: Option<&str>,
    ) -> Vec<Parameter> {
        let mut gets = self.parameters_get.lock();
        gets.push(GetParameters {
            client_id: client.id(),
            param_names: param_names.clone(),
            request_id: request_id.map(|s| s.to_string()),
        });
        self.parameters_get_result.lock().clone()
    }

    fn on_set_parameters(
        &self,
        client: Client,
        parameters: Vec<Parameter>,
        request_id: Option<&str>,
    ) -> Vec<Parameter> {
        let mut sets = self.parameters_set.lock();
        sets.push(SetParameters {
            client_id: client.id(),
            parameters: parameters.clone(),
            request_id: request_id.map(|s| s.to_string()),
        });
        parameters
    }

    fn on_parameters_subscribe(&self, param_names: Vec<String>) {
        let mut subs = self.parameters_subscribe.lock();
        subs.push(param_names.clone());
    }

    fn on_parameters_unsubscribe(&self, param_names: Vec<String>) {
        let mut unsubs = self.parameters_unsubscribe.lock();
        unsubs.push(param_names.clone());
    }

    fn on_connection_graph_subscribe(&self) {
        self.inc_connection_graph_subscribe();
    }

    fn on_connection_graph_unsubscribe(&self) {
        self.inc_connection_graph_unsubscribe();
    }
}

/// Asserts that `cond` returns true within 50ms, polling every 1ms.
///
/// When using this, you probably want to wrap the things you're testing in `dbg!()` macros so that
/// you get helpful debug logs when a test fails:
///
/// ```
/// assert_eventually(|| dbg!(x.len()) == 2);
/// ```
pub async fn assert_eventually(cond: impl Fn() -> bool) {
    let timeout = Duration::from_millis(50);
    let poll_interval = Duration::from_millis(1);
    let result = tokio::time::timeout(timeout, async {
        while !cond() {
            tokio::time::sleep(poll_interval).await;
        }
    })
    .await;
    assert!(result.is_ok(), "condition not met within {timeout:?}");
}
