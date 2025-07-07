pub mod generated {
    // Confine the mess of the things that generate defines to a dedicated namespace with this
    // inline module.
    wit_bindgen::generate!({
        world: "host",
        export_macro_name: "export",
        pub_export_macro: true,
        path: "./wit",
    });
}

/// Export a data loader to wasm output with this macro.
#[macro_export]
#[allow(clippy::crate_in_macro_def)]
macro_rules! export {
    ( $L:ident ) => {
        mod __foxglove_data_loader_export {
            // Put these in a temp module so none of these pollute the current namespace.
            // This whole thing could probably be a proc macro.
            use crate::$L as Loader;
            use std::cell::RefCell;
            use foxglove_data_loader::{loader, DataLoader, MessageIterator};
            foxglove_data_loader::generated::export!(
                DataLoaderWrapper with_types_in foxglove_data_loader::generated
            );

            struct DataLoaderWrapper {
                loader: RefCell<Loader>,
            }

            impl loader::Guest for DataLoaderWrapper {
                type DataLoader = Self;
                type MessageIterator = MessageIteratorWrapper;
            }

            impl loader::GuestDataLoader for DataLoaderWrapper {
                fn new(args: loader::DataLoaderArgs) -> Self {
                    Self { loader: RefCell::new(<Loader as DataLoader>::new(args)) }
                }

                fn initialize(&self) -> Result<loader::Initialization, String> {
                    self.loader.borrow_mut()
                        .initialize()
                        .map(|init| init.into())
                        .map_err(|err| err.to_string())
                }

                fn create_iterator(
                    &self,
                    args: loader::MessageIteratorArgs,
                ) -> Result<loader::MessageIterator, String> {
                    let message_iterator = self.loader.borrow_mut()
                        .create_iter(args)
                        .map_err(|err| err.to_string())?;
                    Ok(loader::MessageIterator::new(MessageIteratorWrapper {
                        message_iterator: RefCell::new(message_iterator),
                    }))
                }

                fn get_backfill(&self, args: loader::BackfillArgs) -> Result<Vec<loader::Message>, String> {
                    self.loader.borrow_mut()
                        .get_backfill(args)
                        .map_err(|err| err.to_string())
                }
            }

            struct MessageIteratorWrapper {
                message_iterator: RefCell<<Loader as DataLoader>::MessageIterator>,
            }

            impl loader::GuestMessageIterator for MessageIteratorWrapper {
                fn next(&self) -> Option<Result<loader::Message, String>> {
                    self.message_iterator.borrow_mut()
                        .next()
                        .map(|r| r.map_err(|err| err.to_string()))
                }
            }
        }
    }
}

use anyhow::anyhow;
use std::collections::BTreeMap;
use std::{cell::RefCell, rc::Rc};

pub use generated::exports::foxglove::loader::loader::{
    self, BackfillArgs, Channel, ChannelId, DataLoaderArgs, Message, MessageIteratorArgs, Schema,
    SchemaId, TimeRange,
};
pub use generated::foxglove::loader::console;
pub use generated::foxglove::loader::reader;

impl std::io::Read for reader::Reader {
    fn read(&mut self, dst: &mut [u8]) -> Result<usize, std::io::Error> {
        Ok(reader::Reader::read(self, dst) as usize)
    }
}

impl std::io::Seek for reader::Reader {
    fn seek(&mut self, seek: std::io::SeekFrom) -> Result<u64, std::io::Error> {
        match seek {
            std::io::SeekFrom::Start(offset) => {
                reader::Reader::seek(self, offset);
            }
            std::io::SeekFrom::End(offset) => {
                let end = reader::Reader::size(self) as i64;
                reader::Reader::seek(self, (end - offset) as u64);
            }
            std::io::SeekFrom::Current(offset) => {
                let pos = reader::Reader::position(self) as i64;
                reader::Reader::seek(self, (pos + offset) as u64);
            }
        }
        Ok(reader::Reader::position(self))
    }
}

/// Initializations are returned by DataLoader::initialize() and hold the set of channels and their
/// corresponding schemas, the time range, and a set of problem messages.
#[derive(Debug, Clone, Default)]
pub struct Initialization {
    channels: Vec<loader::Channel>,
    schemas: Vec<loader::Schema>,
    time_range: TimeRange,
    problems: Vec<String>,
}

impl From<Initialization> for loader::Initialization {
    fn from(init: Initialization) -> loader::Initialization {
        loader::Initialization {
            channels: init.channels,
            schemas: init.schemas,
            time_range: init.time_range,
            problems: init.problems,
        }
    }
}

/// Result to initialize a data loader with a set of schemas, channels, a time range, and a set of
/// problems.
impl Initialization {
    /// Create a builder interface to initialize schemas that link to channels without having to
    /// manage assigning channel and schema IDs.
    pub fn builder() -> InitializationBuilder {
        InitializationBuilder::default()
    }
}

#[derive(Debug)]
struct SchemaManager {
    next_schema_id: u16,
    schemas: BTreeMap<u16, LinkedSchema>,
}

impl Default for SchemaManager {
    fn default() -> Self {
        Self {
            next_schema_id: 1,
            schemas: Default::default(),
        }
    }
}

impl SchemaManager {
    /// Find the next available schema id. This method ensures no other schemas are using this id.
    fn get_free_id(&mut self) -> u16 {
        loop {
            let current_id = self.next_schema_id;
            self.next_schema_id += 1;

            if self.schemas.contains_key(&current_id) {
                continue;
            }

            return current_id;
        }
    }

    /// Add a [`foxglove::Schema`] to the manager using a certain id, returning a [`LinkedSchema`].
    /// This method will return None if the id is being used by another schema.
    fn add_schema(
        &mut self,
        id: u16,
        schema: foxglove::Schema,
        channels: &Rc<RefCell<ChannelManager>>,
    ) -> Option<LinkedSchema> {
        if self.schemas.contains_key(&id) {
            return None;
        }

        let schema = LinkedSchema {
            id,
            schema,
            channels: channels.clone(),
            message_encoding: String::from(""),
        };

        self.schemas.insert(id, schema.clone());

        Some(schema)
    }
}

#[derive(Debug)]
struct ChannelManager {
    next_channel_id: u16,
    channels: BTreeMap<u16, LinkedChannel>,
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self {
            next_channel_id: 1,
            channels: Default::default(),
        }
    }
}

impl ChannelManager {
    /// Add a new channel to the manager by id and return a [`LinkedChannel`]. If there is already
    /// a channel using this ID this method will return None.
    fn add_channel(&mut self, id: u16, topic_name: impl Into<String>) -> Option<LinkedChannel> {
        if self.channels.contains_key(&id) {
            return None;
        }

        let channel = LinkedChannel {
            id,
            schema_id: Rc::new(RefCell::new(None)),
            topic_name: topic_name.into(),
            message_encoding: Rc::new(RefCell::new("".into())),
            message_count: Rc::new(RefCell::new(None)),
        };

        self.channels.insert(id, channel.clone());

        Some(channel)
    }

    /// Get the next available channel ID. This method ensures no other channel is currently using
    /// this ID.
    fn get_free_id(&mut self) -> u16 {
        loop {
            let current_id = self.next_channel_id;
            self.next_channel_id += 1;

            if self.channels.contains_key(&current_id) {
                continue;
            }

            return current_id;
        }
    }
}

/// Builder interface for creating an Initialization with schemas and channels using automatically-
/// assigned IDs.
#[derive(Debug, Clone)]
pub struct InitializationBuilder {
    channels: Rc<RefCell<ChannelManager>>,
    schemas: Rc<RefCell<SchemaManager>>,
    time_range: loader::TimeRange,
    problems: Vec<String>,
}

impl Default for InitializationBuilder {
    fn default() -> Self {
        Self {
            schemas: Rc::new(RefCell::new(SchemaManager::default())),
            channels: Rc::new(RefCell::new(ChannelManager::default())),
            time_range: TimeRange::default(),
            problems: vec![],
        }
    }
}

// TimeRange is defined by the macro, so we can't use the derived Default impl
#[allow(clippy::derivable_impls)]
impl Default for TimeRange {
    fn default() -> Self {
        TimeRange {
            start_time: 0,
            end_time: 0,
        }
    }
}

/// Builder to make Initializations.
impl InitializationBuilder {
    /// Set the initialization's time range.
    pub fn time_range(mut self, time_range: TimeRange) -> Self {
        self.time_range = time_range;
        self
    }

    /// Set the start time for the initialization's time range.
    pub fn start_time(mut self, start_time: u64) -> Self {
        self.time_range.start_time = start_time;
        self
    }

    /// Set the end time for the initialization's time range.
    pub fn end_time(mut self, end_time: u64) -> Self {
        self.time_range.end_time = end_time;
        self
    }

    /// Add a channel by topic string.
    pub fn add_channel(&mut self, topic_name: &str) -> LinkedChannel {
        let id = { self.channels.borrow_mut().get_free_id() };
        self.add_channel_with_id(id, topic_name)
            .expect("id was checked to be free above")
    }

    /// Add a channel by topic string and a certain ID.
    ///
    /// This method will return None if the ID is being used by another channel.
    pub fn add_channel_with_id(&mut self, id: u16, topic_name: &str) -> Option<LinkedChannel> {
        let mut channels = self.channels.borrow_mut();
        channels.add_channel(id, topic_name)
    }

    /// Add a schema from a foxglove::Schema. This adds the schema to the initialization and returns
    /// the [`LinkedSchema`] for further customization and to add channels.
    pub fn add_schema(&mut self, schema: foxglove::Schema) -> LinkedSchema {
        let id = { self.schemas.borrow_mut().get_free_id() };
        self.add_schema_with_id(id, schema)
            .expect("id was checked to be free above")
    }

    /// Add a schema from a [`foxglove::Schema`] and ID. This adds the schema to the initialization and returns
    /// the [`LinkedSchema`] for further customization and to add channels.
    ///
    /// This method will return None if the ID is being used by another channel.
    pub fn add_schema_with_id(
        &mut self,
        id: u16,
        schema: foxglove::Schema,
    ) -> Option<LinkedSchema> {
        assert!(id > 0, "schema id cannot be zero");
        let mut schemas = self.schemas.borrow_mut();
        schemas.add_schema(id, schema, &self.channels)
    }

    /// Add a schema from an implementation of [`foxglove::Encode`].
    /// This sets both the schema and message encoding at once, adds the schema to the
    /// initialization, and returns the LinkedSchema for further customization and to add channels.
    pub fn add_encode<T: foxglove::Encode>(&mut self) -> Result<LinkedSchema, anyhow::Error> {
        let schema_id = { self.schemas.borrow_mut().get_free_id() };
        Ok(self
            .add_encode_with_id::<T>(schema_id)?
            .expect("id was checked to be free above"))
    }

    /// Add a schema from an implementation of [`foxglove::Encode`] and an ID.
    /// This sets both the schema and message encoding at once, adds the schema to the
    /// initialization, and returns the LinkedSchema for further customization and to add channels.
    ///
    /// This method will return None if the ID is being used by another channel.
    pub fn add_encode_with_id<T: foxglove::Encode>(
        &mut self,
        id: u16,
    ) -> Result<Option<LinkedSchema>, anyhow::Error> {
        let schema = T::get_schema().ok_or(anyhow!["Failed to get schema"])?;
        let linked_schema = self
            .add_schema_with_id(id, schema)
            .map(|s| s.message_encoding(T::get_message_encoding()));
        Ok(linked_schema)
    }

    /// Add a problem to the initialization.
    pub fn add_problem(mut self, problem: &str) -> Self {
        self.problems.push(String::from(problem));
        self
    }

    /// Generate the initialization with assigned schema and channel IDs.
    pub fn build(self) -> Initialization {
        let schemas = self
            .schemas
            .borrow()
            .schemas
            .values()
            .cloned()
            .map(Schema::from)
            .collect();

        let channels = self
            .channels
            .borrow()
            .channels
            .values()
            .cloned()
            .map(Channel::from)
            .collect();

        Initialization {
            channels,
            schemas,
            time_range: self.time_range,
            problems: self.problems,
        }
    }
}

/// A LinkedSchema holds a foxglove::Schema plus the Channels that use this schema and message
/// encoding.
#[derive(Debug, Clone)]
pub struct LinkedSchema {
    id: u16,
    schema: foxglove::Schema,
    channels: Rc<RefCell<ChannelManager>>,
    message_encoding: String,
}

impl LinkedSchema {
    /// Create a channel from a topic name with a certain channel ID and message encoding from the
    /// schema default message encoding.
    ///
    /// This method will return None if the ID is being used by another channel.
    pub fn add_channel_with_id(&self, id: u16, topic_name: &str) -> Option<LinkedChannel> {
        let mut channels = self.channels.borrow_mut();
        channels.add_channel(id, topic_name).map(|channel| {
            channel
                .message_encoding(self.message_encoding.clone())
                .schema(self)
        })
    }

    /// Create a channel from a topic name with an assigned channel ID and message encoding from the
    /// schema default message encoding.
    pub fn add_channel(&self, topic_name: &str) -> LinkedChannel {
        let next_id = { self.channels.borrow_mut().get_free_id() };
        self.add_channel_with_id(next_id, topic_name)
            .expect("id was checked to be free above")
    }

    /// Set the message encoding that added channels will use.
    ///
    /// Ensure this method is called before adding channels. Calling this method after channels
    /// have been added may result in incorrect message encodings.
    pub fn message_encoding(mut self, message_encoding: impl Into<String>) -> Self {
        self.message_encoding = message_encoding.into();
        self
    }
}

/// Builder interface that links back to the originating LinkedSchema and InitializationBuilder
#[derive(Debug, Clone)]
pub struct LinkedChannel {
    id: ChannelId,
    schema_id: Rc<RefCell<Option<SchemaId>>>,
    topic_name: String,
    message_encoding: Rc<RefCell<String>>,
    message_count: Rc<RefCell<Option<u64>>>,
}

impl LinkedChannel {
    /// Set the message count for this channel.
    pub fn message_count(self, message_count: u64) -> Self {
        self.message_count.replace(Some(message_count));
        self
    }

    /// Set the message encoding for the channel.
    pub fn message_encoding(self, message_encoding: impl Into<String>) -> Self {
        self.message_encoding.replace(message_encoding.into());
        self
    }

    /// Set the schema id for the channel from a LinkedSchema.
    pub fn schema(self, linked_schema: &LinkedSchema) -> Self {
        self.schema_id.replace(Some(linked_schema.id));
        self
    }
}

impl From<LinkedChannel> for loader::Channel {
    fn from(ch: LinkedChannel) -> loader::Channel {
        loader::Channel {
            id: ch.id,
            schema_id: *ch.schema_id.borrow(),
            topic_name: ch.topic_name.clone(),
            message_encoding: ch.message_encoding.borrow().clone(),
            message_count: *ch.message_count.borrow(),
        }
    }
}

impl From<LinkedSchema> for loader::Schema {
    fn from(value: LinkedSchema) -> Self {
        loader::Schema {
            id: value.id,
            name: value.schema.name,
            encoding: value.schema.encoding,
            data: value.schema.data.to_vec(),
        }
    }
}

/// Implement this trait and call `foxglove::data_loader_export()` on your loader.
pub trait DataLoader: 'static + Sized {
    // Consolidates the Guest and GuestDataLoader traits into a single trait.
    // Wraps new() and create_iterator() to user-defined structs so that users don't need to wrap
    // their types into `loader::DataLoader::new()` or `loader::MessageIterator::new()`.
    type MessageIterator: MessageIterator;
    type Error: Into<Box<dyn std::error::Error>>;

    /// Create a new DataLoader.
    fn new(args: DataLoaderArgs) -> Self;

    /// Initialize your DataLoader, reading enough of the file to generate counts, channels, and
    /// schemas for the `Initialization` result.
    fn initialize(&mut self) -> Result<Initialization, Self::Error>;

    /// Create a MessageIterator for this DataLoader.
    fn create_iter(
        &mut self,
        args: loader::MessageIteratorArgs,
    ) -> Result<Self::MessageIterator, Self::Error>;

    /// Backfill results starting from `args.time` for `args.channels`. The backfill results are the
    /// first message looking backwards in time so that panels won't be empty before playback
    /// begins.
    fn get_backfill(
        &mut self,
        args: loader::BackfillArgs,
    ) -> Result<Vec<loader::Message>, Self::Error>;
}

/// Implement MessageIterator for your loader iterator.
pub trait MessageIterator: 'static + Sized {
    type Error: Into<Box<dyn std::error::Error>>;
    fn next(&mut self) -> Option<Result<Message, Self::Error>>;
}

#[cfg(test)]
mod tests;
