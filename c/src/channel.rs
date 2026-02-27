use std::{ffi::c_void, fs::File, io::BufWriter, mem::ManuallyDrop, sync::Arc};

use crate::{
    FoxgloveChannelMetadata, FoxgloveError, FoxgloveKeyValue, FoxgloveSchema, FoxgloveSinkId,
    FoxgloveString, channel_descriptor::FoxgloveChannelDescriptor, result_to_c,
    sink_channel_filter::ChannelFilter,
};
use mcap::{Compression, WriteOptions};
use std::io::{Seek, SeekFrom, Write};

#[repr(u8)]
pub enum FoxgloveMcapCompression {
    None,
    Zstd,
    Lz4,
}

/// Custom writer function pointers for MCAP writing.
/// write_fn and flush_fn must be non-null. Seek_fn may be null iff `disable_seeking` is set to true.
/// These function pointers may be called from multiple threads.
/// These functions are called synchronously with respect to each other within the SDK, but these
/// calls are not synchronized with other SDK function calls.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FoxgloveCustomWriter {
    /// User-provided context pointer, passed to all callback functions
    pub context: *mut std::ffi::c_void,
    /// Write function: write data to the custom destination
    /// Returns number of bytes written, or sets error on failure
    pub write_fn: Option<
        unsafe extern "C" fn(
            context: *mut std::ffi::c_void,
            data: *const u8,
            len: usize,
            error: *mut i32,
        ) -> usize,
    >,
    /// Flush function: ensure all buffered data is written
    pub flush_fn: Option<unsafe extern "C" fn(context: *mut std::ffi::c_void) -> i32>,

    /// Seek function: change the current position in the stream
    /// whence: 0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END
    pub seek_fn: Option<
        unsafe extern "C" fn(
            context: *mut std::ffi::c_void,
            pos: i64,
            whence: std::ffi::c_int,
            new_pos: *mut u64,
        ) -> i32,
    >,
}

struct CustomWriter {
    callbacks: FoxgloveCustomWriter,
}

impl Write for CustomWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let write_fn = self
            .callbacks
            .write_fn
            .expect("write_fn checked in do_foxglove_mcap_open");

        let mut error = 0;
        let written = unsafe {
            write_fn(
                self.callbacks.context,
                buf.as_ptr(),
                buf.len(),
                &raw mut error,
            )
        };

        if error != 0 {
            return Err(std::io::Error::from_raw_os_error(error));
        }

        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let flush_fn = self
            .callbacks
            .flush_fn
            .expect("flush_fn checked in do_foxglove_mcap_open");

        let error = unsafe { flush_fn(self.callbacks.context) };
        if error != 0 {
            return Err(std::io::Error::from_raw_os_error(error));
        }

        Ok(())
    }
}

impl Seek for CustomWriter {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let seek_fn = self
            .callbacks
            .seek_fn
            .expect("seek_fn checked in do_foxglove_mcap_open");

        let (offset, whence) = match pos {
            SeekFrom::Start(n) => (n as i64, 0), // SEEK_SET
            SeekFrom::End(n) => (n, 2),          // SEEK_END
            SeekFrom::Current(n) => (n, 1),      // SEEK_CUR
        };

        let mut new_pos = 0u64;
        let error = unsafe { seek_fn(self.callbacks.context, offset, whence, &raw mut new_pos) };

        if error != 0 {
            return Err(std::io::Error::from_raw_os_error(error));
        }

        Ok(new_pos)
    }
}

#[repr(C)]
pub struct FoxgloveMcapOptions {
    /// `context` can be null, or a valid pointer to a context created via `foxglove_context_new`.
    /// If it's null, the mcap file will be created with the default context.
    pub context: *const FoxgloveContext,
    pub path: FoxgloveString,
    pub truncate: bool,
    /// Custom writer for arbitrary destinations. If non-null, `path` is ignored.
    pub custom_writer: *const FoxgloveCustomWriter,
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
    /// Context provided to the `sink_channel_filter` callback.
    pub sink_channel_filter_context: *const c_void,
    /// A filter for channels that can be used to subscribe to or unsubscribe from channels.
    ///
    /// This can be used to omit one or more channels from a sink, but still log all channels to another
    /// sink in the same context. Return false to disable logging of this channel.
    ///
    /// This method is invoked from the client's main poll loop and must not block.
    ///
    /// # Safety
    /// - If provided, the handler callback must be a pointer to the filter callback function,
    ///   and must remain valid until the MCAP sink is dropped.
    pub sink_channel_filter: Option<
        unsafe extern "C" fn(
            context: *const c_void,
            channel: *const FoxgloveChannelDescriptor,
        ) -> bool,
    >,
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

// Safety: The user is responsible for ensuring the function pointers and user_data
// are safe to use across threads.
unsafe impl Send for CustomWriter {}

enum McapWriterVariant {
    File(foxglove::McapWriterHandle<BufWriter<File>>),
    Custom(foxglove::McapWriterHandle<CustomWriter>),
}

pub struct FoxgloveMcapWriter(Option<McapWriterVariant>);

impl FoxgloveMcapWriter {
    fn take(&mut self) -> Option<McapWriterVariant> {
        self.0.take()
    }
}

impl McapWriterVariant {
    fn write_metadata(
        &self,
        name: &str,
        metadata: std::collections::BTreeMap<String, String>,
    ) -> Result<(), foxglove::FoxgloveError> {
        match self {
            McapWriterVariant::File(writer) => writer.write_metadata(name, metadata),
            McapWriterVariant::Custom(writer) => writer.write_metadata(name, metadata),
        }
    }

    fn attach(&self, attachment: &mcap::Attachment<'_>) -> Result<(), foxglove::FoxgloveError> {
        match self {
            McapWriterVariant::File(writer) => writer.attach(attachment),
            McapWriterVariant::Custom(writer) => writer.attach(attachment),
        }
    }
}

/// An MCAP attachment to store in an MCAP file.
///
/// Attachments are arbitrary binary data that can be stored alongside messages.
/// Common uses include storing configuration files, calibration data, or other
/// reference material related to the recording.
#[repr(C)]
pub struct FoxgloveMcapAttachment {
    /// Timestamp at which the attachment was recorded, in nanoseconds.
    pub log_time: u64,
    /// Timestamp at which the attachment was created, in nanoseconds.
    /// If not available, set to 0.
    pub create_time: u64,
    /// Name of the attachment, e.g. "config.json".
    pub name: FoxgloveString,
    /// Media type of the attachment, e.g. "application/json".
    pub media_type: FoxgloveString,
    /// Pointer to the attachment data.
    pub data: *const u8,
    /// Length of the attachment data in bytes.
    pub data_len: usize,
}

/// Create or open an MCAP writer for writing to a file or custom destination.
/// Resources must later be freed with `foxglove_mcap_close`.
///
/// If `custom_writer` is provided, the MCAP data will be written using the provided
/// function pointers instead of to a file.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// `path` and `profile` must contain valid UTF8. If `context` is non-null,
/// it must have been created by `foxglove_context_new`.
/// If `custom_writer` is non-null, its function pointers must be valid and
/// all `context` pointers must remain valid for the lifetime of the writer.
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
    let context = options.context;

    // Safety: this is safe if the options struct contains valid strings
    let mcap_options = unsafe { options.to_write_options() }?;

    let mut builder = foxglove::McapWriter::with_options(mcap_options);
    if !context.is_null() {
        let context = ManuallyDrop::new(unsafe { Arc::from_raw(context) });
        builder = builder.context(&context);
    }

    let writer_variant = if !options.custom_writer.is_null() {
        // Use custom writer
        let custom_writer_callbacks = unsafe { *options.custom_writer };
        if custom_writer_callbacks.seek_fn.is_none() && !options.disable_seeking {
            return Err(foxglove::FoxgloveError::ValueError(
                "seek_fn is null but disable_seeking is false".to_string(),
            ));
        }
        if custom_writer_callbacks.write_fn.is_none() || custom_writer_callbacks.flush_fn.is_none()
        {
            return Err(foxglove::FoxgloveError::ValueError(
                "write_fn and flush_fn must be provided".to_string(),
            ));
        }
        let custom_writer = CustomWriter {
            callbacks: custom_writer_callbacks,
        };

        let writer = builder.create(custom_writer)?;

        McapWriterVariant::Custom(writer)
    } else {
        // Use file path (existing behavior)
        let path = unsafe { options.path.as_utf8_str() }
            .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("path is invalid: {e}")))?;

        if path.is_empty() {
            return Err(foxglove::FoxgloveError::ValueError(
                "Either path must be non-empty or custom_writer must be provided".to_string(),
            ));
        }

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
        if let Some(sink_channel_filter) = options.sink_channel_filter {
            builder = builder.channel_filter(Arc::new(ChannelFilter::new(
                options.sink_channel_filter_context,
                sink_channel_filter,
            )));
        }
        let writer = builder.create(BufWriter::new(file))?;
        McapWriterVariant::File(writer)
    };

    // We can avoid this double indirection if we refactor McapWriterHandle to move the context into the Arc
    // and then add into_raw and from_raw methods to convert the Arc to and from a pointer.
    // This is the simplest solution, and we don't call methods on this, so the double indirection doesn't matter much.
    Ok(Box::into_raw(Box::new(FoxgloveMcapWriter(Some(
        writer_variant,
    )))))
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
    let Some(writer_variant) = writer.take() else {
        tracing::error!("foxglove_mcap_close called with writer already closed");
        return FoxgloveError::SinkClosed;
    };

    let result = match writer_variant {
        McapWriterVariant::File(writer) => writer.close().map(|_| ()),
        McapWriterVariant::Custom(writer) => writer.close().map(|_| ()),
    };

    // We don't care about the return value
    unsafe { result_to_c(result, std::ptr::null_mut()) }
}

/// Write metadata to an MCAP file.
///
/// Metadata consists of key-value string pairs associated with a name.
/// If the metadata has no key-value pairs, this method does nothing.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// `writer` must be a valid pointer to a `FoxgloveMcapWriter` created via `foxglove_mcap_open`.
/// `name` must be a valid UTF-8 string.
/// `metadata` must be a valid pointer to an array of `foxglove_key_value` with length `metadata_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_mcap_write_metadata(
    writer: Option<&mut FoxgloveMcapWriter>,
    name: &FoxgloveString,
    metadata: *const FoxgloveKeyValue,
    metadata_len: usize,
) -> FoxgloveError {
    let Some(writer) = writer else {
        tracing::error!("foxglove_mcap_write_metadata called with null writer");
        return FoxgloveError::ValueError;
    };

    match unsafe { do_foxglove_mcap_write_metadata(writer, name, metadata, metadata_len) } {
        Ok(()) => FoxgloveError::Ok,
        Err(e) => e.into(),
    }
}

unsafe fn do_foxglove_mcap_write_metadata(
    writer: &mut FoxgloveMcapWriter,
    name: &FoxgloveString,
    metadata: *const FoxgloveKeyValue,
    metadata_len: usize,
) -> Result<(), foxglove::FoxgloveError> {
    let name = unsafe { name.as_utf8_str() }.map_err(|e| {
        foxglove::FoxgloveError::Utf8Error(format!("metadata name is invalid: {e}"))
    })?;

    let Some(writer_handle) = writer.0.as_ref() else {
        return Err(foxglove::FoxgloveError::SinkClosed);
    };

    if !metadata.is_null() {
        // Convert C key-value array to BTreeMap
        let mut metadata_map = std::collections::BTreeMap::new();
        for i in 0..metadata_len {
            let kv = unsafe { &*metadata.add(i) };
            let key = unsafe { kv.key.as_utf8_str() }?;
            let value = unsafe { kv.value.as_utf8_str() }?;
            metadata_map.insert(key.to_string(), value.to_string());
        }
        writer_handle.write_metadata(name, metadata_map)?;
    }

    Ok(())
}

/// Write an attachment to an MCAP file.
///
/// Attachments are arbitrary binary data that can be stored alongside messages.
/// Common uses include storing configuration files, calibration data, or other
/// reference material related to the recording.
///
/// Returns 0 on success, or returns a FoxgloveError code on error.
///
/// # Safety
/// `writer` must be a valid pointer to a `FoxgloveMcapWriter` created via `foxglove_mcap_open`.
/// `attachment` must be a valid pointer to a `FoxgloveMcapAttachment`.
/// The `name` and `media_type` fields of the attachment must be valid UTF-8 strings.
/// The `data` field must be a valid pointer to an array of bytes with length `data_len`,
/// or null if `data_len` is 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_mcap_attach(
    writer: Option<&mut FoxgloveMcapWriter>,
    attachment: &FoxgloveMcapAttachment,
) -> FoxgloveError {
    let Some(writer) = writer else {
        tracing::error!("foxglove_mcap_attach called with null writer");
        return FoxgloveError::ValueError;
    };

    match unsafe { do_foxglove_mcap_attach(writer, attachment) } {
        Ok(()) => FoxgloveError::Ok,
        Err(e) => {
            tracing::error!("foxglove_mcap_attach failed: {e}");
            e.into()
        }
    }
}

unsafe fn do_foxglove_mcap_attach(
    writer: &mut FoxgloveMcapWriter,
    attachment: &FoxgloveMcapAttachment,
) -> Result<(), foxglove::FoxgloveError> {
    let name = unsafe { attachment.name.as_utf8_str() }.map_err(|e| {
        foxglove::FoxgloveError::Utf8Error(format!("attachment name is invalid: {e}"))
    })?;

    let media_type = unsafe { attachment.media_type.as_utf8_str() }.map_err(|e| {
        foxglove::FoxgloveError::Utf8Error(format!("attachment media_type is invalid: {e}"))
    })?;

    let Some(writer_handle) = writer.0.as_ref() else {
        return Err(foxglove::FoxgloveError::SinkClosed);
    };

    let data = if attachment.data.is_null() || attachment.data_len == 0 {
        if attachment.data_len != 0 {
            return Err(foxglove::FoxgloveError::ValueError(
                "attachment data is null but data_len is not 0".to_string(),
            ));
        }
        // We'll allow a non-null data pointer with zero length. e.g. empty string
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(attachment.data, attachment.data_len) }
    };

    let mcap_attachment = mcap::Attachment {
        log_time: attachment.log_time,
        create_time: attachment.create_time,
        name: name.to_string(),
        media_type: media_type.to_string(),
        data: std::borrow::Cow::Borrowed(data),
    };

    writer_handle.attach(&mcap_attachment)
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
