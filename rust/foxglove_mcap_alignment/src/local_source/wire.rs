//! Native MCAP traversal for the single-file example.
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use mcap::records::{ChunkHeader, Record, op};
use mcap::sans_io::linear_reader::{LinearReadEvent, LinearReader, LinearReaderOptions};

use crate::local_source::{
    FileInventory, SchemaDescriptor, SourceLimits, error, io_error, push_record,
};
use crate::{Error, ErrorKind, Locator, SourceIdentity, SourceRecord};

fn bad(message: &'static str, offset: u64) -> Error {
    error(ErrorKind::InvalidSource, message, offset)
}

fn alloc(length: usize) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|e| error(ErrorKind::ResourceLimit, "record allocation", e))?;
    bytes.resize(length, 0);
    Ok(bytes)
}

struct Body<'a> {
    rest: &'a [u8],
}
impl<'a> Body<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let result = self
            .rest
            .get(..len)
            .ok_or_else(|| bad("truncated record field", 0))?;
        self.rest = &self.rest[len..];
        Ok(result)
    }
    fn u32(&mut self) -> Result<usize, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("field length")) as usize)
    }
    fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("field length"),
        ))
    }
    fn string(&mut self) -> Result<(), Error> {
        let len = self.u32()?;
        std::str::from_utf8(self.take(len)?)
            .map_err(|e| error(ErrorKind::InvalidSource, "invalid UTF-8", e))?;
        Ok(())
    }
    fn map(&mut self, width: Option<usize>) -> Result<(), Error> {
        let len = self.u32()?;
        let bytes = self.take(len)?;
        if let Some(width) = width {
            if len % width != 0 {
                return Err(bad("invalid index/map byte length", 0));
            }
        } else {
            let mut inner = Body { rest: bytes };
            while !inner.rest.is_empty() {
                inner.string()?;
                inner.string()?;
            }
        }
        Ok(())
    }
}
/// Preflight every attacker-controlled length before binrw can allocate strings/maps.
/// The maintained MCAP parser then handles the record's actual interpretation.
pub(super) fn parse(opcode: u8, bytes: &[u8], offset: u64) -> Result<Record<'_>, Error> {
    let mut body = Body { rest: bytes };
    let validation = (|| {
        match opcode {
            op::HEADER => {
                body.string()?;
                body.string()?;
            }
            op::FOOTER => {
                body.take(20)?;
            }
            op::SCHEMA => {
                body.take(2)?;
                body.string()?;
                body.string()?;
                let len = body.u32()?;
                body.take(len)?;
            }
            op::CHANNEL => {
                body.take(4)?;
                body.string()?;
                body.string()?;
                body.map(None)?;
            }
            op::MESSAGE => {
                body.take(22)?;
                body.rest = &[];
            }
            op::CHUNK => {
                body.take(28)?;
                let compression_length = body.u32()?;
                if compression_length > 4 {
                    return Err(bad("unsupported chunk compression name length", offset));
                }
                std::str::from_utf8(body.take(compression_length)?).map_err(|e| {
                    error(
                        ErrorKind::InvalidSource,
                        "invalid chunk compression UTF-8",
                        e,
                    )
                })?;
                let len = body.u64()?;
                body.take(usize::try_from(len).map_err(|_| bad("chunk size overflow", offset))?)?;
            }
            op::MESSAGE_INDEX => {
                body.take(2)?;
                body.map(Some(16))?;
            }
            op::CHUNK_INDEX => {
                body.take(32)?;
                body.map(Some(10))?;
                body.take(8)?;
                body.string()?;
                body.take(16)?;
            }
            op::ATTACHMENT => {
                body.take(16)?;
                body.string()?;
                body.string()?;
                let len = body.u64()?;
                body.take(
                    usize::try_from(len).map_err(|_| bad("attachment size overflow", offset))?,
                )?;
                body.take(4)?;
            }
            op::ATTACHMENT_INDEX => {
                body.take(40)?;
                body.string()?;
                body.string()?;
            }
            op::STATISTICS => {
                body.take(42)?;
                body.map(Some(10))?;
            }
            op::METADATA => {
                body.string()?;
                body.map(None)?;
            }
            op::METADATA_INDEX => {
                body.take(16)?;
                body.string()?;
            }
            op::SUMMARY_OFFSET => {
                body.take(17)?;
            }
            op::DATA_END => {
                body.take(4)?;
            }
            _ => {
                return Err(error(
                    ErrorKind::UnsupportedRecord,
                    "unsupported record opcode",
                    format!("opcode {opcode:#x}, byte offset {offset}"),
                ));
            }
        }
        if !body.rest.is_empty() {
            return Err(bad("trailing bytes in record", offset));
        }
        Ok(())
    })();
    validation.map_err(|mut e| {
        e.detail = Some(format!(
            "record opcode {opcode:#x}, byte offset {offset}: {}",
            e.detail.as_deref().unwrap_or(e.message)
        ));
        e
    })?;
    mcap::read::parse_record(opcode, bytes).map_err(|e| {
        error(
            ErrorKind::InvalidSource,
            "MCAP record parse",
            format!("byte offset {offset}: {e}"),
        )
    })
}
pub(super) fn frame(
    file: &mut File,
    offset: u64,
    end: u64,
    limits: &SourceLimits,
) -> Result<(u8, Vec<u8>, u64), Error> {
    if end.saturating_sub(offset) < 9 {
        return Err(bad("truncated record header", offset));
    }
    file.seek(SeekFrom::Start(offset)).map_err(io_error)?;
    let mut header = [0u8; 9];
    file.read_exact(&mut header).map_err(io_error)?;
    let len = u64::from_le_bytes(header[1..].try_into().expect("frame header"));
    let next = offset
        .checked_add(9)
        .and_then(|v| v.checked_add(len))
        .ok_or_else(|| bad("record length overflow", offset))?;
    if next > end {
        return Err(bad("record extends beyond file section", offset));
    }
    let cap = if header[0] == op::CHUNK {
        limits.max_chunk_bytes
    } else {
        limits.max_record_bytes
    };
    if len > cap as u64 {
        return Err(error(
            ErrorKind::ResourceLimit,
            "record/chunk length limit",
            format!("byte offset {offset}, length {len}, cap {cap}"),
        ));
    }
    let mut body = alloc(len as usize)?;
    file.read_exact(&mut body).map_err(io_error)?;
    Ok((header[0], body, next))
}
pub(super) fn decompress(
    header: &ChunkHeader,
    bytes: &[u8],
    limits: &SourceLimits,
    offset: u64,
) -> Result<Vec<u8>, Error> {
    if header.uncompressed_size > limits.max_decompressed_chunk_bytes as u64 {
        return Err(error(
            ErrorKind::ResourceLimit,
            "decompressed chunk length limit",
            format!("byte offset {offset}, length {}", header.uncompressed_size),
        ));
    }
    let mut output = alloc(header.uncompressed_size as usize)?;
    let backend_error = |e: std::io::Error| {
        error(
            ErrorKind::InvalidSource,
            "chunk decompression",
            format!("byte offset {offset}: {e}"),
        )
    };
    match header.compression.as_str() {
        "" => {
            if bytes.len() != output.len() {
                return Err(bad("uncompressed chunk size mismatch", offset));
            }
            output.copy_from_slice(bytes);
        }
        "zstd" => {
            let mut decoder = zstd::stream::read::Decoder::with_buffer(Cursor::new(bytes))
                .map_err(backend_error)?;
            decoder.window_log_max(23).map_err(backend_error)?;
            decoder.read_exact(&mut output).map_err(backend_error)?;
            let mut extra = [0u8; 1];
            if decoder.read(&mut extra).map_err(backend_error)? != 0 {
                return Err(bad("decompressed chunk exceeds declared size", offset));
            }
            if decoder.finish().position() != bytes.len() as u64 {
                return Err(bad("trailing compressed chunk bytes", offset));
            }
        }
        "lz4" => {
            let mut decoder = lz4::Decoder::new(Cursor::new(bytes)).map_err(backend_error)?;
            decoder.read_exact(&mut output).map_err(backend_error)?;
            let mut extra = [0u8; 1];
            if decoder.read(&mut extra).map_err(backend_error)? != 0 {
                return Err(bad("decompressed chunk exceeds declared size", offset));
            }
            let (cursor, finish) = decoder.finish();
            finish.map_err(backend_error)?;
            if cursor.position() != bytes.len() as u64 {
                return Err(bad("trailing compressed chunk bytes", offset));
            }
        }
        _ => {
            return Err(error(
                ErrorKind::InvalidSource,
                "unsupported chunk compression",
                format!("byte offset {offset}: {}", header.compression),
            ));
        }
    }
    if header.uncompressed_crc != 0 && crc32fast::hash(&output) != header.uncompressed_crc {
        return Err(bad("chunk CRC mismatch", offset));
    }
    Ok(output)
}
fn charge(inventory_used: &mut usize, bytes: usize, limits: &SourceLimits) -> Result<(), Error> {
    *inventory_used = inventory_used
        .checked_add(bytes)
        .ok_or_else(|| error(ErrorKind::ResourceLimit, "inventory memory overflow", bytes))?;
    if *inventory_used > limits.inventory_memory_bytes {
        return Err(error(
            ErrorKind::ResourceLimit,
            "inventory memory limit",
            *inventory_used,
        ));
    }
    Ok(())
}
fn definition(
    record: Record<'_>,
    inventory: &mut FileInventory,
    used: &mut usize,
    limits: &SourceLimits,
    offset: u64,
) -> Result<(), Error> {
    match record {
        Record::Schema { header, data } => {
            if header.id == 0 {
                return Err(bad("schema id zero is reserved", offset));
            }
            if let Some(previous) = inventory.schemas.get(&header.id) {
                if previous.name != header.name
                    || previous.encoding != header.encoding
                    || previous.data != data.as_ref()
                {
                    return Err(bad("schema id redefined", offset));
                }
            } else {
                charge(
                    used,
                    256 + header.name.len() + header.encoding.len() + data.len(),
                    limits,
                )?;
                inventory.schemas.insert(
                    header.id,
                    SchemaDescriptor {
                        name: header.name,
                        encoding: header.encoding,
                        data: data.into_owned(),
                    },
                );
            }
        }
        Record::Channel(channel) => {
            if channel.schema_id != 0 && !inventory.schemas.contains_key(&channel.schema_id) {
                return Err(bad("channel references unknown schema", offset));
            }
            if let Some(previous) = inventory.channels.get(&channel.id) {
                if previous != &channel {
                    return Err(bad("channel id redefined", offset));
                }
            } else {
                let bytes = channel
                    .metadata
                    .iter()
                    .map(|(k, v)| 128 + k.len() + v.len())
                    .sum::<usize>();
                charge(
                    used,
                    1024 + channel.topic.len() + channel.message_encoding.len() + bytes,
                    limits,
                )?;
                inventory.channels.insert(channel.id, channel);
            }
        }
        _ => return Err(bad("invalid definition", offset)),
    }
    Ok(())
}

fn walk(
    mut input: impl Read,
    options: LinearReaderOptions,
    mut offset: u64,
    mut visit: impl FnMut(u8, &[u8], u64) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut reader = LinearReader::new_with_options(options);
    while let Some(event) = reader.next_event() {
        match event.map_err(|e| error(ErrorKind::InvalidSource, "MCAP traversal", e))? {
            LinearReadEvent::ReadRequest(need) => {
                let read = input
                    .read(reader.insert(need.min(64 * 1024)))
                    .map_err(io_error)?;
                reader.notify_read(read);
            }
            LinearReadEvent::Record { data, opcode } => {
                visit(opcode, data, offset)?;
                offset += 9 + data.len() as u64;
            }
        }
    }
    Ok(())
}

pub(super) fn scan(
    path: &Path,
    inventory: &mut FileInventory,
    limits: &SourceLimits,
    mut records: Option<&mut Vec<SourceRecord>>,
    inventory_used: &mut usize,
) -> Result<(), Error> {
    let options = LinearReaderOptions::default()
        .with_check_finishes_after_end_magic(true)
        .with_emit_chunks(true)
        .with_validate_data_section_crc(true)
        .with_validate_summary_section_crc(true)
        .with_record_length_limit(limits.max_record_bytes.max(limits.max_chunk_bytes));
    let mut data_chunks = 0u64;
    let mut first = true;
    let mut in_summary = false;
    let mut indexed_chunks = 0u64;
    let mut message = |header: mcap::records::MessageHeader, locator| {
        if let Some(records) = records.as_deref_mut() {
            push_record(
                records,
                SourceRecord {
                    identity: SourceIdentity {
                        file: 0,
                        channel: header.channel_id,
                        locator,
                    },
                    log_time: header.log_time,
                    publish_time: header.publish_time,
                    sequence: header.sequence,
                },
                limits.index_memory_bytes,
            )?;
        }
        Ok(())
    };
    walk(
        File::open(path).map_err(io_error)?,
        options,
        8,
        |opcode, data, offset| {
            let limit = if opcode == op::CHUNK {
                limits.max_chunk_bytes
            } else {
                limits.max_record_bytes
            };
            if data.len() > limit {
                return Err(error(
                    ErrorKind::ResourceLimit,
                    "record length limit",
                    offset,
                ));
            }
            if first && opcode != op::HEADER {
                return Err(bad("expected MCAP Header", offset));
            }
            first = false;
            match parse(opcode, data, offset)? {
                value @ (Record::Schema { .. } | Record::Channel(_)) => {
                    definition(value, inventory, inventory_used, limits, offset)?;
                }
                Record::Message { header, .. } if !in_summary => {
                    if !inventory.channels.contains_key(&header.channel_id) {
                        return Err(bad("unknown message channel", offset));
                    }
                    message(header, Locator::Unchunked { offset })?;
                }
                Record::Chunk { header, data } if !in_summary => {
                    let decompressed = decompress(&header, &data, limits, offset)?;
                    let inner = LinearReaderOptions::default()
                        .with_skip_start_magic(true)
                        .with_skip_end_magic(true)
                        .with_emit_chunks(true)
                        .with_record_length_limit(limits.max_record_bytes);
                    walk(
                        decompressed.as_slice(),
                        inner,
                        0,
                        |opcode, bytes, message_offset| {
                            match parse(opcode, bytes, offset)? {
                                value @ (Record::Schema { .. } | Record::Channel(_)) => {
                                    definition(value, inventory, inventory_used, limits, offset)?;
                                }
                                Record::Message { header, .. } => {
                                    if !inventory.channels.contains_key(&header.channel_id) {
                                        return Err(bad("unknown chunk message channel", offset));
                                    }
                                    message(
                                        header,
                                        Locator::Chunked {
                                            chunk_offset: offset,
                                            message_offset,
                                        },
                                    )?;
                                }
                                _ => return Err(bad("unsupported chunk record", offset)),
                            }
                            Ok(())
                        },
                    )?;
                    data_chunks += 1;
                }
                Record::DataEnd(_) if !in_summary => in_summary = true,
                Record::ChunkIndex(_) if in_summary => indexed_chunks += 1,
                Record::Unknown { .. } => return Err(bad("unsupported MCAP record", offset)),
                Record::Message { .. }
                | Record::Chunk { .. }
                | Record::ChunkIndex(_)
                | Record::DataEnd(_) => {
                    return Err(bad("misplaced MCAP record", offset));
                }
                _ => {}
            }
            Ok(())
        },
    )?;
    if !in_summary || indexed_chunks != data_chunks {
        return Err(bad("example requires an indexed MCAP file", indexed_chunks));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn limits() -> SourceLimits {
        SourceLimits {
            index_memory_bytes: 4096,
            inventory_memory_bytes: 65536,
            max_record_bytes: 65536,
            max_chunk_bytes: 65536,
            max_decompressed_chunk_bytes: 65536,
            temporary_bytes: 1_048_576,
            max_open_files: 32,
        }
    }

    #[test]
    fn nested_lengths_are_checked_before_the_mcap_parser_allocates() {
        let cases = [
            (op::HEADER, vec![255; 8]),
            (op::SCHEMA, vec![255; 10]),
            (op::CHANNEL, vec![255; 20]),
            (op::MESSAGE_INDEX, vec![255; 6]),
            (op::METADATA, vec![255; 8]),
            (op::ATTACHMENT, vec![255; 24]),
        ];
        for (opcode, bytes) in cases {
            let failure =
                parse(opcode, &bytes, 123).expect_err("unbounded nested lengths rejected");
            assert_eq!(failure.kind, ErrorKind::InvalidSource);
            assert!(failure.detail.expect("offset diagnostic").contains("123"));
        }
    }

    #[test]
    fn actual_decompression_cannot_exceed_declared_size_and_crc_is_checked() {
        let input = vec![42; 16384];
        for compression in ["", "zstd", "lz4"] {
            let compressed = match compression {
                "" => input.clone(),
                "zstd" => zstd::stream::encode_all(Cursor::new(&input), 1).expect("compress"),
                "lz4" => {
                    let mut encoder = lz4::EncoderBuilder::new()
                        .build(Vec::new())
                        .expect("encoder");
                    encoder.write_all(&input).expect("compress");
                    let (bytes, result) = encoder.finish();
                    result.expect("finish");
                    bytes
                }
                _ => unreachable!(),
            };
            let mut header = ChunkHeader {
                message_start_time: 0,
                message_end_time: 0,
                uncompressed_size: input.len() as u64,
                uncompressed_crc: crc32fast::hash(&input),
                compression: compression.to_owned(),
                compressed_size: compressed.len() as u64,
            };
            assert_eq!(
                decompress(&header, &compressed, &limits(), 0).expect("decode"),
                input
            );
            header.uncompressed_crc ^= 1;
            assert!(decompress(&header, &compressed, &limits(), 0).is_err());
            header.uncompressed_crc = 0;
            header.uncompressed_size = 16;
            assert!(decompress(&header, &compressed, &limits(), 0).is_err());
            header.uncompressed_size = 1_000_000;
            assert_eq!(
                decompress(&header, &compressed, &limits(), 0)
                    .expect_err("declared bomb limit")
                    .kind,
                ErrorKind::ResourceLimit
            );
        }
    }
}
