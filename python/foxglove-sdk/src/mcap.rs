use crate::errors::PyFoxgloveError;
use foxglove::{McapCompression, McapWriteOptions, McapWriterHandle};
use pyo3::prelude::*;
use std::fs::File;
use std::io::BufWriter;

/// Compression algorithm to use for MCAP writing.
#[pyclass(eq, eq_int, name = "MCAPCompression", module = "foxglove.mcap")]
#[derive(PartialEq, Clone)]
pub enum PyMcapCompression {
    Zstd = 0,
    Lz4 = 1,
}

impl From<PyMcapCompression> for McapCompression {
    fn from(value: PyMcapCompression) -> Self {
        match value {
            PyMcapCompression::Zstd => McapCompression::Zstd,
            PyMcapCompression::Lz4 => McapCompression::Lz4,
        }
    }
}

/// Options for the MCAP writer.
///
/// All parameters are optional.
///
/// :param compression: Specifies the compression that should be used on chunks. Defaults to Zstd.
///     Pass `None` to disable compression.
/// :type compression: MCAPCompression
/// :param profile: Specifies the profile that should be written to the MCAP Header record.
/// :type profile: str
/// :param chunk_size: Specifies the target uncompressed size of each chunk.
/// :type chunk_size: int
/// :param use_chunks: Specifies whether to use chunks for storing messages.
/// :type use_chunks: bool
/// :param emit_statistics: Specifies whether to write a statistics record in the summary section.
/// :type emit_statistics: bool
/// :param emit_summary_offsets: Specifies whether to write summary offset records.
/// :type emit_summary_offsets: bool
/// :param emit_message_indexes: Specifies whether to write message index records after each chunk.
/// :type emit_message_indexes: bool
/// :param emit_chunk_indexes: Specifies whether to write chunk index records in the summary section.
/// :type emit_chunk_indexes: bool
/// :param repeat_channels: Specifies whether to repeat each channel record from the data section in the summary section.
/// :type repeat_channels: bool
/// :param repeat_schemas: Specifies whether to repeat each schema record from the data section in the summary section.
/// :type repeat_schemas: bool
/// :param calculate_chunk_crcs: Specifies whether to calculate and write CRCs for chunk records.
/// :type calculate_chunk_crcs: bool
/// :param calculate_data_section_crc: Specifies whether to calculate and write a data section CRC into the DataEnd record.
/// :type calculate_data_section_crc: bool
/// :param calculate_summary_section_crc: Specifies whether to calculate and write a summary section CRC into the Footer record.
/// :type calculate_summary_section_crc: bool
#[derive(Default, Clone)]
#[pyclass(name = "MCAPWriteOptions", module = "foxglove.mcap")]
pub(crate) struct PyMcapWriteOptions(McapWriteOptions);

#[pymethods]
impl PyMcapWriteOptions {
    #[new]
    #[pyo3(signature = (
        *,
        compression = None,
        profile = None,
        chunk_size = None,
        use_chunks = false,
        emit_statistics = true,
        emit_summary_offsets = true,
        emit_message_indexes = true,
        emit_chunk_indexes = true,
        repeat_channels = true,
        repeat_schemas = true,
        calculate_chunk_crcs = true,
        calculate_data_section_crc = true,
        calculate_summary_section_crc = true,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        compression: Option<PyMcapCompression>,
        profile: Option<String>,
        chunk_size: Option<u64>,
        use_chunks: Option<bool>,
        emit_statistics: Option<bool>,
        emit_summary_offsets: Option<bool>,
        emit_message_indexes: Option<bool>,
        emit_chunk_indexes: Option<bool>,
        repeat_channels: Option<bool>,
        repeat_schemas: Option<bool>,
        calculate_chunk_crcs: Option<bool>,
        calculate_data_section_crc: Option<bool>,
        calculate_summary_section_crc: Option<bool>,
    ) -> Self {
        let compression = compression.or(Some(PyMcapCompression::Zstd));
        let opts = McapWriteOptions::default()
            .compression(compression.map(Into::into))
            .chunk_size(chunk_size)
            .use_chunks(use_chunks.unwrap_or(false))
            .emit_statistics(emit_statistics.unwrap_or(true))
            .emit_summary_offsets(emit_summary_offsets.unwrap_or(true))
            .emit_message_indexes(emit_message_indexes.unwrap_or(true))
            .emit_chunk_indexes(emit_chunk_indexes.unwrap_or(true))
            .repeat_channels(repeat_channels.unwrap_or(true))
            .repeat_schemas(repeat_schemas.unwrap_or(true))
            .calculate_chunk_crcs(calculate_chunk_crcs.unwrap_or(true))
            .calculate_data_section_crc(calculate_data_section_crc.unwrap_or(true))
            .calculate_summary_section_crc(calculate_summary_section_crc.unwrap_or(true));

        let opts = if let Some(profile) = profile {
            opts.profile(profile)
        } else {
            opts
        };

        Self(opts)
    }
}

impl From<PyMcapWriteOptions> for McapWriteOptions {
    fn from(value: PyMcapWriteOptions) -> Self {
        value.0
    }
}

/// A writer for logging messages to an MCAP file.
///
/// Obtain an instance by calling :py:func:`foxglove.open_mcap`.
///
/// This class may be used as a context manager, in which case the writer will
/// be closed when you exit the context.
///
/// If the writer is not closed by the time it is garbage collected, it will be
/// closed automatically, and any errors will be logged.
#[pyclass(name = "MCAPWriter", module = "foxglove.mcap")]
pub(crate) struct PyMcapWriter(pub(crate) Option<McapWriterHandle<BufWriter<File>>>);

impl Drop for PyMcapWriter {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            log::error!("Failed to close MCAP writer: {e}");
        }
    }
}

#[pymethods]
impl PyMcapWriter {
    fn __enter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __exit__(
        &mut self,
        _exc_type: Py<PyAny>,
        _exc_value: Py<PyAny>,
        _traceback: Py<PyAny>,
    ) -> PyResult<()> {
        self.close()
    }

    /// Close the MCAP writer.
    ///
    /// You may call this to explicitly close the writer. Note that the writer will be automatically
    /// closed for you when it is garbage collected, or when exiting the context manager.
    fn close(&mut self) -> PyResult<()> {
        if let Some(writer) = self.0.take() {
            writer.close().map_err(PyFoxgloveError::from)?;
        }
        Ok(())
    }
}

pub fn register_submodule(parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let module = PyModule::new(parent_module.py(), "mcap")?;

    module.add_class::<PyMcapCompression>()?;
    module.add_class::<PyMcapWriter>()?;
    module.add_class::<PyMcapWriteOptions>()?;

    // Define as a package
    // https://github.com/PyO3/pyo3/issues/759
    let py = parent_module.py();
    py.import("sys")?
        .getattr("modules")?
        .set_item("foxglove._foxglove_py.mcap", &module)?;

    parent_module.add_submodule(&module)
}
