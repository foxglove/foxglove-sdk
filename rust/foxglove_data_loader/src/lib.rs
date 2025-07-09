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
use std::collections::HashMap;
use std::{cell::RefCell, rc::Rc};

pub use generated::exports::foxglove::loader::loader::{
    self, BackfillArgs, Channel, ChannelId, DataLoaderArgs, Message, MessageIteratorArgs, Schema,
    SchemaId, Severity, TimeRange,
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
                reader::Reader::seek(self, (end + offset) as u64);
            }
            std::io::SeekFrom::Current(offset) => {
                let pos = reader::Reader::position(self) as i64;
                reader::Reader::seek(self, (pos + offset) as u64);
            }
        }
        Ok(reader::Reader::position(self))
    }
}

/// Problems can be used to display info in the "problems" panel during playback.
///
/// They are for non-fatal issues that the user should be aware of.
#[derive(Clone, Debug)]
pub struct Problem(loader::Problem);

impl Problem {
    /// Create a new [`Problem`] with the provided [`Severity`] and message.
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self(loader::Problem {
            severity,
            message: message.into(),
            tip: None,
        })
    }

    /// Add additional context to the problem.
    pub fn tip(mut self, tip: impl Into<String>) -> Self {
        self.0.tip = Some(tip.into());
        self
    }

    /// Create a new error [`Problem`] with the provided message.
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    /// Create a new warn [`Problem`] with the provided message.
    pub fn warn(message: impl Into<String>) -> Self {
        Self::new(Severity::Warn, message)
    }

    /// Create a new info [`Problem`] with the provided message.
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(Severity::Info, message)
    }

    fn into_inner(self) -> loader::Problem {
        self.0
    }
}

impl<T: Into<String>> From<T> for Problem {
    fn from(value: T) -> Self {
        Self::error(value)
    }
}

/// Initializations are returned by DataLoader::initialize() and hold the set of channels and their
/// corresponding schemas, the time range, and a set of problem messages.
#[derive(Debug, Clone, Default)]
pub struct Initialization {
    channels_by_topic: HashMap<String, Rc<Channel>>,
    schemas: Vec<loader::Schema>,
    time_range: TimeRange,
    problems: Vec<Problem>,
}

impl From<Initialization> for loader::Initialization {
    fn from(init: Initialization) -> loader::Initialization {
        loader::Initialization {
            channels: init
                .channels_by_topic
                .values()
                .map(|ch| (**ch).clone())
                .collect(),
            schemas: init.schemas,
            time_range: init.time_range,
            problems: init.problems.into_iter().map(|p| p.into_inner()).collect(),
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

    pub fn get_channel(&self, topic_name: &str) -> Option<Rc<loader::Channel>> {
        self.channels_by_topic.get(topic_name).cloned()
    }
}

/// Builder interface for creating an Initialization with schemas and channels using automatically-
/// assigned IDs.
#[derive(Debug, Clone)]
pub struct InitializationBuilder {
    next_channel_id: Rc<RefCell<u16>>,
    next_schema_id: u16,
    time_range: loader::TimeRange,
    schemas: Vec<LinkedSchema>,
    problems: Vec<Problem>,
}

impl Default for InitializationBuilder {
    fn default() -> Self {
        Self {
            next_channel_id: Rc::new(RefCell::new(1)),
            next_schema_id: 1,
            time_range: TimeRange::default(),
            problems: vec![],
            schemas: vec![],
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
    pub fn add_channel(&self, topic_name: &str) -> LinkedChannel {
        let channel_id = *self.next_channel_id.borrow();
        self.next_channel_id.replace(channel_id + 1);
        LinkedChannel {
            id: channel_id,
            schema_id: Rc::new(RefCell::new(None)),
            topic_name: topic_name.into(),
            message_encoding: Rc::new(RefCell::new("".into())),
            message_count: Rc::new(RefCell::new(None)),
        }
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

    /// Add a [`Problem`] to the initialization.
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Create an initialization with a bunch of problems:
    /// # use foxglove_data_loader::*;
    /// let init = Initialization::builder()
    ///     // You can add an "error" with a &str:
    ///     .add_problem("The provided file was invalid")
    ///     // You can also add an error like this:
    ///     .add_problem(Problem::error("The provided file was invalid"))
    ///     // You can add an error with a tip, like this:
    ///     .add_problem(
    ///         Problem::error("file was invalid")
    ///             .tip("The provided file could not be read. Ensure it is valid.")
    ///     )
    ///     // You can also add warning and info problems:
    ///     .add_problem(Problem::warn("The file contained some empty topics"))
    ///     .add_problem(Problem::info("The file contained some empty topics"))
    ///     .build();
    /// ```
    ///
    pub fn add_problem(mut self, problem: impl Into<Problem>) -> Self {
        self.problems.push(problem.into());
        self
    }

    /// Generate the initialization with assigned schema and channel IDs.
    pub fn build(self) -> Initialization {
        let schemas = self
            .schemas
            .iter()
            .map(|linked_schema| {
                Schema::from_id_sdk(linked_schema.id, linked_schema.schema.clone())
            })
            .collect();
        let channels_by_topic = self
            .schemas
            .iter()
            .flat_map(|linked_schema| {
                let channels = linked_schema.channels.borrow();
                channels
                    .iter()
                    .map(|ch| (ch.topic_name.to_string(), Rc::new(ch.clone().into())))
                    .collect::<Vec<_>>()
            })
            .collect();
        Initialization {
            channels_by_topic,
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
    channels: Rc<RefCell<Vec<LinkedChannel>>>,
    message_encoding: String,
}

impl LinkedSchema {
    /// Create a channel from a topic name with an assigned channel ID and message encoding from the
    /// schema default message encoding.
    pub fn add_channel(&self, topic_name: &str) -> LinkedChannel {
        let channel_id = *self.next_channel_id.borrow();
        self.next_channel_id.replace(channel_id + 1);
        let channel = LinkedChannel {
            id: channel_id,
            schema_id: Rc::new(RefCell::new(Some(self.id))),
            topic_name: topic_name.into(),
            message_encoding: Rc::new(RefCell::new(self.message_encoding.clone())),
            message_count: Rc::new(RefCell::new(None)),
        };
        self.channels.borrow_mut().push(channel.clone());
        channel
    }

    /// Add a LinkedChannel to this schema, assigning the schema id and schema encoding onto the channel.
    pub fn add_linked_channel(&self, linked_channel: LinkedChannel) {
        self.channels.borrow_mut().push(
            linked_channel
                .schema_id(self.id)
                .message_encoding(self.message_encoding.clone()),
        );
    }

    /// Set the message encoding that added channels will use.
    pub fn message_encoding(mut self, message_encoding: String) -> Self {
        self.message_encoding = message_encoding;
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
    pub fn message_encoding(self, message_encoding: String) -> Self {
        self.message_encoding.replace(message_encoding);
        self
    }

    /// Set the schema id for the channel from a LinkedSchema.
    pub fn schema(self, linked_schema: &LinkedSchema) -> Self {
        linked_schema.add_linked_channel(self.clone());
        self
    }

    /// Assign a schema id only to the channel.
    pub fn schema_id(self, schema_id: SchemaId) -> Self {
        self.schema_id.replace(Some(schema_id));
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
