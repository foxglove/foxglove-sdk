use crate::{Encode, Schema};

pub mod generated {
    // Confine the mess of the things that generate defines to a dedicated namespace with this
    // inline module.
    wit_bindgen::generate!({
        world: "host",
        export_macro_name: "export",
        pub_export_macro: true,
        path: "../../wit",
    });
}

/// Export a data loader to wasm output with this macro.
#[macro_export]
macro_rules! data_loader_export {
    ( $L:ident ) => {
        mod __foxglove_data_loader_export {
            // Put these in a temp module so none of these pollute the current namespace.
            // This whole thing could probably be a proc macro.
            use crate::$L as LOADER;
            foxglove::data_loader::generated::export!(
                LOADER with_types_in foxglove::data_loader::generated
            );
        }
    }
}

pub use generated::exports::foxglove::loader::loader::{
    self, BackfillArgs, Channel, InitializeResult, Message, MessageIteratorArgs, TimeRange,
};
pub use generated::foxglove::loader::console;
pub use generated::foxglove::loader::reader;

impl std::io::Read for reader::Reader {
    fn read(&mut self, dst: &mut [u8]) -> Result<usize, std::io::Error> {
        Ok(reader::Reader::read(&self, dst) as usize)
    }
}

impl std::io::Seek for reader::Reader {
    fn seek(&mut self, seek: std::io::SeekFrom) -> Result<u64, std::io::Error> {
        match seek {
            std::io::SeekFrom::Start(offset) => {
                reader::Reader::seek(&self, offset);
            }
            std::io::SeekFrom::End(offset) => {
                let end = reader::Reader::size(&self) as i64;
                reader::Reader::seek(&self, (end - offset) as u64);
            }
            std::io::SeekFrom::Current(offset) => {
                let pos = reader::Reader::position(&self) as i64;
                reader::Reader::seek(&self, (pos + offset) as u64);
            }
        }
        Ok(reader::Reader::position(&self))
    }
}

impl Channel {
    /// Return a ChannelBuilder to set properties for a Channel.
    pub fn builder() -> ChannelBuilder {
        ChannelBuilder::default()
    }
}

/// Builder interface to create a Channel.
#[derive(Default)]
pub struct ChannelBuilder {
    id: u16,
    topic_name: String,
    schema_name: String,
    message_encoding: String,
    schema_encoding: String,
    schema_data: Vec<u8>,
    message_count: Option<u64>,
}

impl ChannelBuilder {
    /// Set the channel id.
    pub fn id(mut self, id: u16) -> Self {
        self.id = id;
        self
    }

    /// Set the channel topic name.
    pub fn topic(mut self, topic_name: &str) -> Self {
        self.topic_name = topic_name.to_string();
        self
    }

    /// Set the schema and message encoding from a foxglove::Encode.
    /// Panics if T::get_schema() is None.
    pub fn encode<T: Encode>(self) -> Self {
        let schema = T::get_schema().expect("failed to get schema");
        self.schema(schema)
            .message_encoding(&T::get_message_encoding())
    }

    /// Set the channel schema name, schema encoding, and schema data from a foxglove::Schema.
    pub fn schema(mut self, schema: Schema) -> Self {
        self.schema_name = schema.name;
        self.schema_encoding = schema.encoding;
        self.schema_data = schema.data.into();
        self
    }

    pub fn schema_name(mut self, schema_name: &str) -> Self {
        self.schema_name = schema_name.to_string();
        self
    }

    pub fn schema_encoding(mut self, schema_encoding: &str) -> Self {
        self.schema_encoding = schema_encoding.to_string();
        self
    }

    pub fn schema_data(mut self, schema_data: Vec<u8>) -> Self {
        self.schema_data = schema_data;
        self
    }

    /// Set the channel message encoding.
    pub fn message_encoding(mut self, message_encoding: &str) -> Self {
        self.message_encoding = message_encoding.to_string();
        self
    }

    /// Set the message count.
    pub fn message_count(mut self, message_count: Option<u64>) -> Self {
        self.message_count = message_count;
        self
    }

    /// Turn this ChannelBuilder into a Channel.
    pub fn build(self) -> Channel {
        Channel {
            id: self.id,
            topic_name: self.topic_name,
            message_encoding: self.message_encoding,
            message_count: self.message_count,
            schema_name: self.schema_name,
            schema_encoding: self.schema_encoding,
            schema_data: self.schema_data,
        }
    }
}

/// Implement this trait along with MessageIterator, then call `foxglove::data_loader_export()` on
/// your loader.
pub trait DataLoader: 'static + Sized {
    // Consolidates the Guest and GuestDataLoader traits into a single trait.
    // Wraps create() and create_iter() to user-defined structs so that users don't need to wrap
    // their types into `loader::DataLoader::new()` or `loader::MessageIterator::new()`.
    type MessageIterator: loader::GuestMessageIterator;
    type Error: Into<Box<dyn std::error::Error>>;

    fn from_paths(inputs: Vec<String>) -> Result<Self, Self::Error>;
    fn initialize(&self) -> loader::InitializeResult;

    fn create_iter(
        &self,
        args: loader::MessageIteratorArgs,
    ) -> Result<Self::MessageIterator, Self::Error>;
    fn get_backfill(&self, args: loader::BackfillArgs)
        -> Result<Vec<loader::Message>, Self::Error>;
}

pub trait MessageIterator: 'static + Sized {
    type Error: Into<Box<dyn std::error::Error>>;
    fn next(&self) -> Option<Result<Message, Self::Error>>;
}

impl<T: DataLoader> loader::Guest for T {
    type DataLoader = Self;
    type MessageIterator = T::MessageIterator;

    fn from_paths(inputs: Vec<String>) -> Result<loader::DataLoader, String> {
        T::from_paths(inputs)
            .map(|loader| loader::DataLoader::new(loader))
            .map_err(|e| e.into().to_string())
    }
}

impl<T: DataLoader> loader::GuestDataLoader for T {
    fn initialize(&self) -> InitializeResult {
        T::initialize(self)
    }

    fn create_iter(
        &self,
        args: loader::MessageIteratorArgs,
    ) -> Result<loader::MessageIterator, String> {
        T::create_iter(self, args)
            .map(|iter| loader::MessageIterator::new(iter))
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
