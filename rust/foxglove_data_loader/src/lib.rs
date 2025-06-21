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
            use crate::$L as LOADER;
            foxglove_data_loader::generated::export!(
                LOADER with_types_in foxglove_data_loader::generated
            );
        }
    }
}

use anyhow::anyhow;
use std::{cell::RefCell, rc::Rc};

pub use generated::exports::foxglove::loader::loader::{
    self, BackfillArgs, Channel, DataLoaderArgs, Initialization, Message, MessageIteratorArgs,
    Schema, TimeRange,
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

/// Result to initialize a data loader with a set of schemas, channels, a time range, and a set of
/// problems.
impl loader::Initialization {
    /// Create a builder interface to initialize schemas that link to channels without having to
    /// manage assigning channel and schema IDs.
    pub fn builder() -> InitializationBuilder {
        InitializationBuilder::default()
    }
}

#[derive(Debug, Clone)]
pub struct InitializationBuilder {
    next_channel_id: Rc<RefCell<u16>>,
    next_schema_id: u16,
    time_range: loader::TimeRange,
    schemas: Vec<LinkedSchema>,
    problems: Vec<String>,
}

impl Default for InitializationBuilder {
    fn default() -> Self {
        Self {
            next_channel_id: Rc::new(RefCell::new(1)),
            next_schema_id: 1,
            time_range: TimeRange {
                start_time: 0,
                end_time: 0,
            },
            problems: vec![],
            schemas: vec![],
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

    /// Add a schema from a foxglove::Schema. This adds the schema to the initialization and returns
    /// the LinkedSchema for further customization and to add channels.
    pub fn add_schema(&mut self, schema: foxglove::Schema) -> LinkedSchema {
        let schema_id = self.next_schema_id;
        self.next_schema_id += 1;
        let linked_schema = LinkedSchema {
            id: schema_id,
            next_channel_id: self.next_channel_id.clone(),
            schema,
            channels: Rc::new(RefCell::new(vec![])),
            message_encoding: String::from(""),
        };
        self.schemas.push(linked_schema.clone());
        linked_schema
    }

    /// Add a schema from an implementation of foxglove::Encode.
    /// This sets both the schema and message encoding at once, adds the schema to the
    /// initialization, and returns the LinkedSchema for further customization and to add channels.
    pub fn add_encode<T: foxglove::Encode>(&mut self) -> Result<LinkedSchema, anyhow::Error> {
        let schema = T::get_schema().ok_or(anyhow!["Failed to get schema"])?;
        let linked_schema = self
            .add_schema(schema)
            .message_encoding(T::get_message_encoding());
        Ok(linked_schema)
    }

    /// Add a problem to the initialization.
    pub fn add_problem(mut self, problem: &str) -> Self {
        self.problems.push(String::from(problem));
        self
    }

    /// Generate the initialization with assigned schema and channel IDs.
    pub fn build(self) -> loader::Initialization {
        let schemas = self
            .schemas
            .iter()
            .map(|linked_schema| {
                Schema::from_id_sdk(linked_schema.id, linked_schema.schema.clone())
            })
            .collect();
        let channels = self
            .schemas
            .iter()
            .flat_map(|linked_schema| linked_schema.channels.borrow().clone())
            .collect();
        loader::Initialization {
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
    next_channel_id: Rc<RefCell<u16>>,
    schema: foxglove::Schema,
    channels: Rc<RefCell<Vec<loader::Channel>>>,
    message_encoding: String,
}

impl LinkedSchema {
    /// Create a channel from a topic name with an assigned channel ID and message encoding from the
    /// schema default message encoding.
    pub fn add_channel(&mut self, topic_name: &str) -> loader::Channel {
        let channel_id = *self.next_channel_id.borrow();
        self.next_channel_id.replace(channel_id + 1);
        let channel = loader::Channel {
            id: channel_id,
            schema_id: Some(self.id),
            topic_name: topic_name.into(),
            message_encoding: self.message_encoding.clone(),
            message_count: None,
        };
        self.channels.borrow_mut().push(channel.clone());
        channel
    }

    /// Set the message encoding that added channels will use.
    pub fn message_encoding(mut self, message_encoding: String) -> Self {
        self.message_encoding = message_encoding;
        self
    }
}

impl loader::Channel {
    /// Set the message count for this channel.
    pub fn message_count(mut self, message_count: u64) -> Self {
        self.message_count = Some(message_count);
        self
    }

    /// Set the message encoding for the channel.
    pub fn message_encoding(mut self, message_encoding: String) -> Self {
        self.message_encoding = message_encoding;
        self
    }
}

impl Schema {
    /// Convert a schema id and foxglove::Schema to a data loader Schema.
    pub fn from_id_sdk(id: u16, schema: foxglove::Schema) -> Schema {
        Schema {
            id,
            name: schema.name,
            encoding: schema.encoding,
            data: schema.data.to_vec(),
        }
    }
}

/// Implement this trait and call `foxglove::data_loader_export()` on your loader.
pub trait DataLoader: 'static + Sized {
    // Consolidates the Guest and GuestDataLoader traits into a single trait.
    // Wraps new() and create_iterator() to user-defined structs so that users don't need to wrap
    // their types into `loader::DataLoader::new()` or `loader::MessageIterator::new()`.
    type MessageIterator: loader::GuestMessageIterator;
    type Error: Into<Box<dyn std::error::Error>>;

    /// Create a new DataLoader.
    fn new(args: DataLoaderArgs) -> Self;

    /// Initialize your DataLoader, reading enough of the file to generate counts, channels, and
    /// schemas for the `Initialization` result.
    fn initialize(&self) -> Result<Initialization, Self::Error>;

    /// Create a MessageIterator for this DataLoader.
    fn create_iter(
        &self,
        args: loader::MessageIteratorArgs,
    ) -> Result<Self::MessageIterator, Self::Error>;

    /// Backfill results starting from `args.time` for `args.channels`. The backfill results are the
    /// first message looking backwards in time so that panels won't be empty before playback
    /// begins.
    fn get_backfill(&self, args: loader::BackfillArgs)
        -> Result<Vec<loader::Message>, Self::Error>;
}

/// Implement MessageIterator for your loader iterator.
pub trait MessageIterator: 'static + Sized {
    type Error: Into<Box<dyn std::error::Error>>;
    fn next(&self) -> Option<Result<Message, Self::Error>>;
}

impl<T: DataLoader> loader::Guest for T {
    type DataLoader = Self;
    type MessageIterator = T::MessageIterator;
}

impl<T: DataLoader> loader::GuestDataLoader for T {
    fn new(args: loader::DataLoaderArgs) -> T {
        T::new(args)
    }

    fn initialize(&self) -> Result<loader::Initialization, String> {
        T::initialize(self).map_err(|e| e.into().to_string())
    }

    fn create_iterator(
        &self,
        args: loader::MessageIteratorArgs,
    ) -> Result<loader::MessageIterator, String> {
        T::create_iter(self, args)
            .map(loader::MessageIterator::new)
            .map_err(|err| err.into().to_string())
    }

    fn get_backfill(&self, args: loader::BackfillArgs) -> Result<Vec<loader::Message>, String> {
        T::get_backfill(self, args).map_err(|err| err.into().to_string())
    }
}

impl<T: MessageIterator> loader::GuestMessageIterator for T {
    fn next(&self) -> Option<Result<loader::Message, String>> {
        T::next(self).map(|r| r.map_err(|err| err.into().to_string()))
    }
}
