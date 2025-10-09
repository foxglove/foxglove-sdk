use crate::{FoxgloveError, FoxgloveKeyValue, FoxgloveSchema, FoxgloveString};

pub struct FoxgloveChannelDescriptor(pub(crate) foxglove::ChannelDescriptor);

/// Get the topic of a channel descriptor.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel_descriptor`.
///
/// If the passed channel is null, an empty value is returned.
///
/// The returned value is valid only for the lifetime of the channel, which is typically the
/// duration of a callback where a descriptor is passed.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_descriptor_get_topic(
    channel: Option<&FoxgloveChannelDescriptor>,
) -> FoxgloveString {
    let Some(channel) = channel else {
        return FoxgloveString::default();
    };
    FoxgloveString {
        data: channel.0.topic().as_ptr().cast(),
        len: channel.0.topic().len(),
    }
}

/// Get the message_encoding of a channel descriptor.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel_descriptor`.
///
/// If the passed channel is null, an empty value is returned.
///
/// The returned value is valid only for the lifetime of the channel, which is typically the
/// duration of a callback where a descriptor is passed.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_descriptor_get_message_encoding(
    channel: Option<&FoxgloveChannelDescriptor>,
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
/// `channel` must be a valid pointer to a `foxglove_channel_descriptor`.
/// `schema` must be a valid pointer to a `FoxgloveSchema` struct that will be filled in.
///
/// The returned value is valid only for the lifetime of the channel, which is typically the
/// duration of a callback where a descriptor is passed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_descriptor_get_schema(
    channel: Option<&FoxgloveChannelDescriptor>,
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

/// An iterator over a channel descriptor's metadata key-value pairs.
#[repr(C)]
pub struct FoxgloveChannelDescriptorMetadataIterator {
    channel: *const FoxgloveChannelDescriptor,
    index: usize,
}

/// Create an iterator over a channel descriptor's metadata.
///
/// You must later free the iterator using foxglove_channel_descriptor_metadata_iter_free.
///
/// Iterate items using foxglove_channel_descriptor_metadata_iter_next.
///
/// # Safety
/// `channel` must be a valid pointer to a `foxglove_channel_descriptor`.
/// The channel descriptor must remain valid for the lifetime of the iterator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_descriptor_metadata_iter_create(
    channel: Option<&FoxgloveChannelDescriptor>,
) -> *mut FoxgloveChannelDescriptorMetadataIterator {
    let Some(channel) = channel else {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(FoxgloveChannelDescriptorMetadataIterator {
        channel: channel as *const _,
        index: 0,
    }))
}

/// Get the next key-value pair from the metadata iterator.
///
/// Returns true if a pair was found and stored in `key_value`, false if the iterator is exhausted.
///
/// # Safety
/// `iter` must be a valid pointer to a `foxglove_channel_descriptor_metadata_iterator` created via
/// `foxglove_channel_descriptor_metadata_iter_create`.
/// `key_value` must be a valid pointer to a `FoxgloveKeyValue` that will be filled in.
/// The channel descriptor itself must still be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_descriptor_metadata_iter_next(
    iter: *mut FoxgloveChannelDescriptorMetadataIterator,
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

/// Free a metadata iterator created via `foxglove_channel_descriptor_metadata_iter_create`.
///
/// # Safety
/// `iter` must be a valid pointer to a `foxglove_channel_descriptor_metadata_iterator` created via
/// `foxglove_channel_descriptor_metadata_iter_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_channel_descriptor_metadata_iter_free(
    iter: *mut FoxgloveChannelDescriptorMetadataIterator,
) {
    if !iter.is_null() {
        // Safety: undo the Box::into_raw in foxglove_channel_descriptor_metadata_iter_create; safe
        // if this was created by that method
        drop(unsafe { Box::from_raw(iter) });
    }
}
