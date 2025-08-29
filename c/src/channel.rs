use std::{fs::File, io::BufWriter, mem::ManuallyDrop, sync::Arc};

use crate::{
    result_to_c, FoxgloveChannelMetadata, FoxgloveError, FoxgloveKeyValue, FoxgloveSchema,
    FoxgloveSinkId, FoxgloveString,
};
use mcap::{Compression, WriteOptions};

#[repr(u8)]
pub enum FoxgloveMcapCompression {
    None,
    Zstd,
    Lz4,
}

#[repr(C)]
pub struct FoxgloveMcapOptions {
    /// `context` can be null, or a valid pointer to a context created via `foxglove_context_new`.
    /// If it's null, the mcap file will be created with the default context.
    pub context: *const FoxgloveContext,
    pub path: FoxgloveString,
    pub truncate: bool,
    pub compression: FoxgloveMcapCompression,
    pub profile: FoxgloveString,
    // The library option is not provided here, because it is ignored by our Rust SDK
    /// chunk_size of 0 is treated as if it was omitted (None)
    pub chunk_size: u64,
    pub use_chunks: bool,
    pub disable_seeking: bool,
    pub emit_statistics: bool,
    pub emit_summary_offsets: bool,
    pub emit_message_indexes: bool,
    pub emit_chunk_indexes: bool,
    pub emit_attachment_indexes: bool,
    pub emit_metadata_indexes: bool,
    pub repeat_channels: bool,
    pub repeat_schemas: bool,
}

impl FoxgloveMcapOptions {
    unsafe fn to_write_options(&self) -> Result<WriteOptions, foxglove::FoxgloveError> {
        let profile = unsafe { self.profile.as_utf8_str() }
            .map_err(|e| foxglove::FoxgloveError::ValueError(format!("profile is invalid: {e}")))?;

        let compression = match self.compression {
            FoxgloveMcapCompression::Zstd => Some(Compression::Zstd),
            FoxgloveMcapCompression::Lz4 => Some(Compression::Lz4),
            _ => None,
        };

        Ok(WriteOptions::default()
            .profile(profile)
            .compression(compression)
            .chunk_size(if self.chunk_size > 0 {
                Some(self.chunk_size)
            } else {
                None
            })
            .use_chunks(self.use_chunks)
            .disable_seeking(self.disable_seeking)
            .emit_statistics(self.emit_statistics)
            .emit_summary_offsets(self.emit_summary_offsets)
            .emit_message_indexes(self.emit_message_indexes)
            .emit_chunk_indexes(self.emit_chunk_indexes)
            .emit_attachment_indexes(self.emit_attachment_indexes)
            .emit_metadata_indexes(self.emit_metadata_indexes)
            .repeat_channels(self.repeat_channels)
            .repeat_schemas(self.repeat_schemas))
    }
}

pub struct FoxgloveMcapWriter(Option<foxglove::McapWriterHandle<BufWriter<File>>>);

impl FoxgloveMcapWriter {
    fn take(&mut self) -> Option<foxglove::McapWriterHandle<BufWriter<File>>> {
        self.0.take()
    }
}

/// Create or open an MCAP file for writing.
/// Resources must later be freed with `foxglove_mcap_close`.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// `path` and `profile` must contain valid UTF8. If `context` is non-null,
/// it must have been created by `foxglove_context_new`.
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn foxglove_mcap_open(
    options: &FoxgloveMcapOptions,
    writer: *mut *mut FoxgloveMcapWriter,
) -> FoxgloveError {
    unsafe {
        let result = do_foxglove_mcap_open(options);
        result_to_c(result, writer)
    }
}

unsafe fn do_foxglove_mcap_open(
    options: &FoxgloveMcapOptions,
) -> Result<*mut FoxgloveMcapWriter, foxglove::FoxgloveError> {
    let path = unsafe { options.path.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("path is invalid: {e}")))?;
    let context = options.context;

    // Safety: this is safe if the options struct contains valid strings
    let mcap_options = unsafe { options.to_write_options() }?;

    let mut file_options = File::options();
    if options.truncate {
        file_options.create(true).truncate(true);
    } else {
        file_options.create_new(true);
    }
    let file = file_options
        .write(true)
        .open(path)
        .map_err(foxglove::FoxgloveError::IoError)?;

    let mut builder = foxglove::McapWriter::with_options(mcap_options);
    if !context.is_null() {
        let context = ManuallyDrop::new(unsafe { Arc::from_raw(context) });
        builder = builder.context(&context);
    }
    let writer = builder
        .create(BufWriter::new(file))
        .expect("Failed to create writer");
    // We can avoid this double indirection if we refactor McapWriterHandle to move the context into the Arc
    // and then add into_raw and from_raw methods to convert the Arc to and from a pointer.
    // This is the simplest solution, and we don't call methods on this, so the double indirection doesn't matter much.
    Ok(Box::into_raw(Box::new(FoxgloveMcapWriter(Some(writer)))))
}

/// Close an MCAP file writer created via `foxglove_mcap_open`.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// `writer` must be a valid pointer to a `FoxgloveMcapWriter` created via `foxglove_mcap_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_mcap_close(
    writer: Option<&mut FoxgloveMcapWriter>,
) -> FoxgloveError {
    let Some(writer) = writer else {
        tracing::error!("foxglove_mcap_close called with null writer");
        return FoxgloveError::ValueError;
    };
    // Safety: undo the Box::into_raw in foxglove_mcap_open, safe if this was created by that method
    let mut writer = unsafe { Box::from_raw(writer) };
    let Some(writer) = writer.take() else {
        tracing::error!("foxglove_mcap_close called with writer already closed");
        return FoxgloveError::SinkClosed;
    };
    let result = writer.close();
    // We don't care about the return value
    unsafe { result_to_c(result, std::ptr::null_mut()) }
}

pub struct FoxgloveChannel(foxglove::RawChannel);

/// Create a new channel. The channel must later be freed with `foxglove_channel_free`.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// `topic` and `message_encoding` must contain valid UTF8.
/// `schema` is an optional pointer to a schema. The schema and the data it points to
/// need only remain alive for the duration of this function call (they will be copied).
/// `context` can be null, or a valid pointer to a context created via `foxglove_context_new`.
/// `metadata` can be null, or a valid pointer to a collection of key/value pairs. If keys are
///     duplicated in the collection, the last value for each key will be used.
/// `channel` is an out **FoxgloveChannel pointer, which will be set to the created channel
/// if the function returns success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_raw_channel_create(
    topic: FoxgloveString,
    message_encoding: FoxgloveString,
    schema: *const FoxgloveSchema,
    context: *const FoxgloveContext,
    metadata: *const FoxgloveChannelMetadata,
    channel: *mut *const FoxgloveChannel,
) -> FoxgloveError {
    if channel.is_null() {
        tracing::error!("channel cannot be null");
        return FoxgloveError::ValueError;
    }
    unsafe {
        let result =
            do_foxglove_raw_channel_create(topic, message_encoding, schema, context, metadata);
        result_to_c(result, channel)
    }
}

unsafe fn do_foxglove_raw_channel_create(
    topic: FoxgloveString,
    message_encoding: FoxgloveString,
    schema: *const FoxgloveSchema,
    context: *const FoxgloveContext,
    metadata: *const FoxgloveChannelMetadata,
) -> Result<*const FoxgloveChannel, foxglove::FoxgloveError> {
    let topic = unsafe { topic.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("topic invalid: {e}")))?;
    let message_encoding = unsafe { message_encoding.as_utf8_str() }.map_err(|e| {
        foxglove::FoxgloveError::Utf8Error(format!("message_encoding invalid: {e}"))
    })?;

    let mut maybe_schema = None;
    if let Some(schema) = unsafe { schema.as_ref() } {
        let schema = unsafe { schema.to_native() }?;
        maybe_schema = Some(schema);
    }

    let mut builder = foxglove::ChannelBuilder::new(topic)
        .message_encoding(message_encoding)
        .schema(maybe_schema);
    if !context.is_null() {
        let context = ManuallyDrop::new(unsafe { Arc::from_raw(context) });
        builder = builder.context(&context);
    }
    if !metadata.is_null() {
        let metadata = ManuallyDrop::new(unsafe { Arc::from_raw(metadata) });
        for i in 0..metadata.count {
            let item = unsafe { metadata.items.add(i) };
            let key = unsafe { (*item).key.as_utf8_str() }.map_err(|e| {
                foxglove::FoxgloveError::Utf8Error(format!("invalid metadata key: {e}"))
            })?;
            let value = unsafe { (*item).value.as_utf8_str() }.map_err(|e| {
                foxglove::FoxgloveError::Utf8Error(format!("invalid metadata value: {e}"))
            })?;
            builder = builder.add_metadata(key, value);
        }
    }
    builder
        .build_raw()
        .map(|raw_channel| Arc::into_raw(raw_channel) as *const FoxgloveChannel)
}

pub(crate) unsafe fn do_foxglove_channel_create<T: foxglove::Encode>(
    topic: FoxgloveString,
    context: *const FoxgloveContext,
) -> Result<*const FoxgloveChannel, foxglove::FoxgloveError> {
    let topic_str = unsafe {
        topic
            .as_utf8_str()
            .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("topic invalid: {e}")))?
    };

    let mut builder = foxglove::ChannelBuilder::new(topic_str);
    if !context.is_null() {
        let context = ManuallyDrop::new(unsafe { Arc::from_raw(context) });
        builder = builder.context(&context);
    }
    Ok(Arc::into_raw(builder.build::<T>().into_inner()) as *const FoxgloveChannel)
}

/// Close a channel.
///
/// You can use this to explicitly unadvertise the channel to sinks that subscribe to channels
/// dynamically, such as the WebSocketServer.
///
/// Attempts to log on a closed channel will elicit a throttled warning message.
///
/// Note this *does not* free the channel.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
/// If channel is null, this does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_close(channel: Option<&FoxgloveChannel>) {
    let Some(channel) = channel else {
        return;
    };
    channel.0.close();
}

/// Free a channel created via `foxglove_channel_create`.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
/// If channel is null, this does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_free(channel: Option<&FoxgloveChannel>) {
    let Some(channel) = channel else {
        return;
    };
    drop(unsafe { Arc::from_raw(channel) });
}

/// Get the ID of a channel.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
///
/// If the passed channel is null, an invalid id of 0 is returned.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_get_id(channel: Option<&FoxgloveChannel>) -> u64 {
    let Some(channel) = channel else {
        return 0;
    };
    u64::from(channel.0.id())
}

/// Get the topic of a channel.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
///
/// If the passed channel is null, an empty value is returned.
///
/// The returned value is valid only for the lifetime of the channel.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_get_topic(channel: Option<&FoxgloveChannel>) -> FoxgloveString {
    let Some(channel) = channel else {
        return FoxgloveString::default();
    };
    FoxgloveString {
        data: channel.0.topic().as_ptr().cast(),
        len: channel.0.topic().len(),
    }
}

/// Get the message_encoding of a channel.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
///
/// If the passed channel is null, an empty value is returned.
///
/// The returned value is valid only for the lifetime of the channel.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_get_message_encoding(
    channel: Option<&FoxgloveChannel>,
) -> FoxgloveString {
    let Some(channel) = channel else {
        return FoxgloveString::default();
    };
    FoxgloveString {
        data: channel.0.message_encoding().as_ptr().cast(),
        len: channel.0.message_encoding().len(),
    }
}

/// Get the schema of a channel.
///
/// If the passed channel is null or has no schema, returns `FoxgloveError::ValueError`.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
/// `schema` must be a valid pointer to a `FoxgloveSchema` struct that will be filled in.
///
/// The returned value is valid only for the lifetime of the channel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_get_schema(
    channel: Option<&FoxgloveChannel>,
    schema: *mut FoxgloveSchema,
) -> FoxgloveError {
    let Some(channel) = channel else {
        return FoxgloveError::ValueError;
    };
    if schema.is_null() {
        return FoxgloveError::ValueError;
    }
    let Some(schema_data) = channel.0.schema() else {
        return FoxgloveError::ValueError;
    };

    unsafe {
        (*schema).name = FoxgloveString {
            data: schema_data.name.as_ptr().cast(),
            len: schema_data.name.len(),
        };
        (*schema).encoding = FoxgloveString {
            data: schema_data.encoding.as_ptr().cast(),
            len: schema_data.encoding.len(),
        };
        (*schema).data = schema_data.data.as_ptr().cast();
        (*schema).data_len = schema_data.data.len();
    }

    FoxgloveError::Ok
}

/// Find out if any sinks have been added to a channel.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
///
/// If the passed channel is null, false is returned.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_has_sinks(channel: Option<&FoxgloveChannel>) -> bool {
    let Some(channel) = channel else {
        return false;
    };
    channel.0.has_sinks()
}

/// An iterator over channel metadata key-value pairs.
#[repr(C)]
pub struct FoxgloveChannelMetadataIterator {
    /// The channel with metadata to iterate
    channel: *const FoxgloveChannel,
    /// Current index
    index: usize,
}

/// Create an iterator over a channel's metadata.
///
/// You must later free the iterator using foxglove_channel_metadata_iter_free.
///
/// Iterate items using foxglove_channel_metadata_iter_next.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel` created via `foxglove_channel_create`.
/// The channel must remain valid for the lifetime of the iterator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_metadata_iter_create(
    channel: Option<&FoxgloveChannel>,
) -> *mut FoxgloveChannelMetadataIterator {
    let Some(channel) = channel else {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(FoxgloveChannelMetadataIterator {
        channel: channel as *const _,
        index: 0,
    }))
}

/// Get the next key-value pair from the metadata iterator.
///
/// Returns true if a pair was found and stored in `key_value`, false if the iterator is exhausted.
///
/// # Safety
/// `iter` must be a valid pointer to a `FoxgloveChannelMetadataIterator` created via
/// `foxglove_channel_metadata_iter_create`.
/// `key_value` must be a valid pointer to a `FoxgloveKeyValue` that will be filled in.
/// The channel itself must still be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_metadata_iter_next(
    iter: *mut FoxgloveChannelMetadataIterator,
    key_value: *mut FoxgloveKeyValue,
) -> bool {
    if iter.is_null() || key_value.is_null() {
        return false;
    }
    let iter = unsafe { &mut *iter };
    let channel = unsafe { &*iter.channel };
    let metadata = channel.0.metadata();

    if iter.index >= metadata.len() {
        return false;
    }

    let Some((key, value)) = metadata.iter().nth(iter.index) else {
        return false;
    };

    unsafe {
        *key_value = FoxgloveKeyValue {
            key: FoxgloveString::from(key),
            value: FoxgloveString::from(value),
        };
    }

    iter.index += 1;
    true
}

/// Free a metadata iterator created via `foxglove_channel_metadata_iter_create`.
///
/// # Safety
/// `iter` must be a valid pointer to a `FoxgloveChannelMetadataIterator` created via
/// `foxglove_channel_metadata_iter_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_metadata_iter_free(
    iter: *mut FoxgloveChannelMetadataIterator,
) {
    if !iter.is_null() {
        // Safety: undo the Box::into_raw in foxglove_channel_metadata_iter_create; safe if this was
        // created by that method
        drop(unsafe { Box::from_raw(iter) });
    }
}

/// Log a message on a channel.
///
/// # Safety
/// `data` must be non-null, and the range `[data, data + data_len)` must contain initialized data
/// contained within a single allocated object.
///
/// `log_time` Some(nanoseconds since epoch timestamp) or None to use the current time.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_log(
    channel: Option<&FoxgloveChannel>,
    data: *const u8,
    data_len: usize,
    log_time: Option<&u64>,
    sink_id: FoxgloveSinkId,
) -> FoxgloveError {
    // An assert might be reasonable under different circumstances, but here
    // we don't want to crash the program using the library, on a robot in the field,
    // because it called log incorrectly. It's safer to warn about it and do nothing.
    let Some(channel) = channel else {
        tracing::error!("foxglove_channel_log called with null channel");
        return FoxgloveError::ValueError;
    };
    if data.is_null() || data_len == 0 {
        tracing::error!("foxglove_channel_log called with null or empty data");
        return FoxgloveError::ValueError;
    }
    // avoid decrementing ref count
    let channel = ManuallyDrop::new(unsafe {
        Arc::from_raw(channel as *const _ as *const foxglove::RawChannel)
    });

    let sink_id = std::num::NonZeroU64::new(sink_id).map(foxglove::SinkId::new);

    channel.log_with_meta_to_sink(
        unsafe { std::slice::from_raw_parts(data, data_len) },
        foxglove::PartialMetadata {
            log_time: log_time.copied(),
        },
        sink_id,
    );
    FoxgloveError::Ok
}

// This generates a `typedef struct foxglove_context foxglove_context`
// for the opaque type that we want to expose to C, but does so under
// a module to avoid collision with the actual type.
pub mod export {
    pub struct FoxgloveContext;
}

// This aligns our internal type name with the exported opaque type.
use foxglove::Context as FoxgloveContext;

/// Create a new context. This never fails.
/// You must pass this to `foxglove_context_free` when done with it.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_context_new() -> *const FoxgloveContext {
    let context = foxglove::Context::new();
    Arc::into_raw(context)
}

/// Free a context created via `foxglove_context_new` or `foxglove_context_free`.
///
/// # Safety
/// `context` must be a valid pointer to a context created via `foxglove_context_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_context_free(context: *const FoxgloveContext) {
    if context.is_null() {
        return;
    }
    drop(unsafe { Arc::from_raw(context) });
}

pub(crate) fn log_msg_to_channel<T: foxglove::Encode>(
    channel: Option<&FoxgloveChannel>,
    msg: &T,
    log_time: Option<&u64>,
    sink_id: FoxgloveSinkId,
) -> FoxgloveError {
    let Some(channel) = channel else {
        tracing::error!("log called with null channel");
        return FoxgloveError::ValueError;
    };
    let channel = ManuallyDrop::new(unsafe {
        // Safety: we're restoring the Arc<RawChannel> we leaked into_raw in foxglove_channel_create
        let channel_arc = Arc::from_raw(channel as *const _ as *mut foxglove::RawChannel);
        // We can safely create a Channel from any Arc<RawChannel>
        foxglove::Channel::<T>::from_raw_channel(channel_arc)
    });

    let sink_id = std::num::NonZeroU64::new(sink_id).map(foxglove::SinkId::new);

    channel.log_with_meta_to_sink(
        msg,
        foxglove::PartialMetadata {
            log_time: log_time.copied(),
        },
        sink_id,
    );
    FoxgloveError::Ok
}
