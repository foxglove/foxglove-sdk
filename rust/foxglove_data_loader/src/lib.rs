//! Data Loaders are an experimental [Extension API] that allow you to support more file formats in
//! Foxglove. You build a Data Loader as a Foxglove Extension using WASM.
//!
//! To create a Data Loader, implement the [`DataLoader`] and [`MessageIterator`] traits, and make
//! your Data Loader available using the [`export`] macro.
//!
//! The compiled WASM blob can then be bundled with a Foxglove Extension to be installed in the
//! Foxglove app. View the [Data Loader template] for an end-to-end example of bundling a Data
//! Loader for use in Foxglove.
//!
//! [Extension API]: https://docs.foxglove.dev/docs/extensions
//! [data loader template]: https://github.com/foxglove/create-foxglove-extension/tree/main/examples/rust-data-loader-template
//!
//! # Example
//!
//! ```rust
//! use foxglove_data_loader::*;
//! use std::io::{ BufReader, Read };
//!
//! struct MyDataLoader { paths: Vec<String> }
//!
//! impl DataLoader for MyDataLoader {
//!     // Your error type can be anything that can be converted into a boxed error.
//!     // This could be an `anyhow` or `thiserror` error, or even a `String`.
//!     type Error = String;
//!     // Foxglove will ask your Data Loader for multiple message iterators to fill the Foxglove
//!     // panels with data. You define a struct that implements [`MessageIterator`] to do this.
//!     type MessageIterator = MyMessageIterator;
//!
//!     fn new(args: DataLoaderArgs) -> Self {
//!         // Data Loaders are created with a list of paths selected from the Foxglove app.
//!         // Keep them around so you can open them using the `reader` interface later.
//!         let DataLoaderArgs { paths } = args;
//!         Self { paths }
//!     }
//!
//!     fn initialize(&mut self) -> Result<Initialization, Self::Error> {
//!         // Return an `Initialization` to tell Foxglove what your data looks like.
//!         let mut builder = Initialization::builder();
//!
//!         // Open one of the provided paths:
//!         let mut file = BufReader::new(reader::open(&self.paths[0]));
//!         let mut buf = vec![ 0; 1024 ];
//!         // Read some data from your file:
//!         file.read(&mut buf)
//!             .expect("should be able to read");
//!
//!         let great_channel = builder
//!             .add_channel("/my-great-channel")
//!             .message_count(10);
//!
//!         // Keep track of this so you know when Foxglove requests `/my-great-channel`.
//!         let great_channel_id = great_channel.id();
//!
//!         let init = builder
//!             .add_problem(Problem::warn("this is a warning about the data"))
//!             .build();
//!
//!         Ok(init)
//!     }
//!
//!     fn create_iter(&mut self, args: MessageIteratorArgs) ->
//!         Result<Self::MessageIterator, Self::Error> {
//!         // Return an iterator that will return messages for the requested channels
//!         // and time range.
//!         Ok(MyMessageIterator {
//!             current_nanos: args.start_time.unwrap_or(0),
//!             end_nanos: args.start_time.unwrap_or(u64::MAX),
//!             channels: args.channels
//!         })
//!     }
//! }
//!
//! struct MyMessageIterator {
//!     channels: Vec<u16>,
//!     current_nanos: u64,
//!     end_nanos: u64
//! }
//!
//! impl MessageIterator for MyMessageIterator {
//!     type Error = String;
//!
//!     fn next(&mut self) -> Option<Result<Message, Self::Error>> {
//!         // When all the data for the time range has been read, return None.
//!         if self.current_nanos > self.end_nanos {
//!             return None;
//!         }
//!
//!         // Return a message containing data from your file format.
//!         Some(Ok(Message {
//!             channel_id: 1,
//!             data: vec![ 1, 2, 3, 4 ],
//!             log_time: 100,
//!             publish_time: 100
//!         }))
//!     }
//! }
//! ```

/// Export a data loader to WASM output with this macro.
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
            foxglove_data_loader::__generated::export!(
                DataLoaderWrapper with_types_in foxglove_data_loader::__generated
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

#[doc(inline)]
pub use __generated::exports::foxglove::loader::loader::{
    BackfillArgs, Channel, ChannelId, DataLoaderArgs, Message, MessageIteratorArgs, Schema,
    SchemaId, Severity, TimeRange,
};

#[doc(inline)]
pub use __generated::foxglove::loader::{console, reader};

// This is used by the export macro but shouldn't be accessed directly.
#[doc(hidden)]
pub use __generated::exports::foxglove::loader::loader;

impl std::io::Read for reader::Reader {
    fn read(&mut self, dst: &mut [u8]) -> Result<usize, std::io::Error> {
        // The WIT read requires the target pointer and length to write into.
        // This allows us to read into a slice without copying.
        let ptr = dst.as_ptr() as _;
        let len = dst.len() as _;
        Ok(reader::Reader::read(self, ptr, len) as usize)
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
    channels: Vec<loader::Channel>,
    schemas: Vec<loader::Schema>,
    time_range: TimeRange,
    problems: Vec<Problem>,
}

impl From<Initialization> for loader::Initialization {
    fn from(init: Initialization) -> loader::Initialization {
        loader::Initialization {
            channels: init.channels,
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
    problems: Vec<Problem>,
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

/// Builder to make an [`Initialization`].
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

/// A [`LinkedSchema`] holds a [`foxglove::Schema`] plus the Channels that use this schema and message
/// encoding.
#[derive(Debug, Clone)]
pub struct LinkedSchema {
    id: SchemaId,
    schema: foxglove::Schema,
    channels: Rc<RefCell<ChannelManager>>,
    message_encoding: String,
}

impl LinkedSchema {
    /// Get the ID of the schema
    pub fn id(&self) -> SchemaId {
        self.id
    }

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

/// Builder interface that links back to the originating [`LinkedSchema`] and [`InitializationBuilder`]
#[derive(Debug, Clone)]
pub struct LinkedChannel {
    id: ChannelId,
    schema_id: Rc<RefCell<Option<SchemaId>>>,
    topic_name: String,
    message_encoding: Rc<RefCell<String>>,
    message_count: Rc<RefCell<Option<u64>>>,
}

impl LinkedChannel {
    /// Get the ID of the current channel
    pub fn id(&self) -> ChannelId {
        self.id
    }

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

    /// Set the schema ID for the channel from a [`LinkedSchema`].
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

    /// Create a new [`DataLoader`].
    fn new(args: DataLoaderArgs) -> Self;

    /// Initialize your [`DataLoader`], reading enough of the file to generate counts, channels, and
    /// schemas for the `Initialization` result.
    fn initialize(&mut self) -> Result<Initialization, Self::Error>;

    /// Create a [`MessageIterator`] for this [`DataLoader`] for the requested channels and time range.
    fn create_iter(
        &mut self,
        args: loader::MessageIteratorArgs,
    ) -> Result<Self::MessageIterator, Self::Error>;

    /// Return the most recent message for each of the requested channels at a particular point in
    /// time.
    ///
    /// Backfill is the first message looking backwards in time from a particular point in time for
    /// a channel. These messages are used when beginning playback from a certain time so that
    /// Foxglove panels are not empty as playback begins.
    ///
    /// This trait has a default implementation that returns no backfill messages. Implement this
    /// method with backfill logic specific to your data loader to give users the best experience
    /// when seeking a recording.
    fn get_backfill(
        &mut self,
        _args: loader::BackfillArgs,
    ) -> Result<Vec<loader::Message>, Self::Error> {
        Ok(Vec::new())
    }
}

/// Implement [`MessageIterator`] for your loader iterator.
pub trait MessageIterator: 'static + Sized {
    type Error: Into<Box<dyn std::error::Error>>;
    fn next(&mut self) -> Option<Result<Message, Self::Error>>;
}

#[doc(hidden)]
pub mod __generated {
    // Confine the mess of the things that generate defines to a dedicated namespace with this
    // inline module.
    wit_bindgen::generate!({
        world: "host",
        export_macro_name: "export",
        pub_export_macro: true,
        path: "./wit",
    });
}

#[cfg(test)]
mod tests;
