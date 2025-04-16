from enum import Enum
from typing import Any, Optional

class MCAPCompression(Enum):
    """
    Compression options for content in an MCAP file.
    """

    Zstd = 0
    Lz4 = 1

class MCAPWriteOptions:
    """
    Options for the MCAP writer.

    :param compression: Specifies the compression that should be used on chunks. Defaults to Zstd.
        Pass `None` to disable compression.
    :param profile: Specifies the profile that should be written to the MCAP Header record.
    :param chunk_size: Specifies the target uncompressed size of each chunk.
    :param use_chunks: Specifies whether to use chunks for storing messages.
    :param emit_statistics: Specifies whether to write a statistics record in the summary section.
    :param emit_summary_offsets: Specifies whether to write summary offset records.
    :param emit_message_indexes: Specifies whether to write message index records after each chunk.
    :param emit_chunk_indexes: Specifies whether to write chunk index records in the summary
        section.
    :param repeat_channels: Specifies whether to repeat each channel record from the data section
        in the summary section.
    :param repeat_schemas: Specifies whether to repeat each schema record from the data section in
        the summary section.
    :param calculate_chunk_crcs: Specifies whether to calculate and write CRCs for chunk records.
    :param calculate_data_section_crc: Specifies whether to calculate and write a data section CRC
        into the DataEnd record.
    :param calculate_summary_section_crc: Specifies whether to calculate and write a summary section
        CRC into the Footer record.
    """

    def __new__(
        cls,
        *,
        compression: Optional[MCAPCompression] = MCAPCompression.Zstd,
        profile: Optional[str] = None,
        chunk_size: Optional[int] = None,
        use_chunks: bool = False,
        emit_statistics: bool = True,
        emit_summary_offsets: bool = True,
        emit_message_indexes: bool = True,
        emit_chunk_indexes: bool = True,
        repeat_channels: bool = True,
        repeat_schemas: bool = True,
        calculate_chunk_crcs: bool = True,
        calculate_data_section_crc: bool = True,
        calculate_summary_section_crc: bool = True,
    ) -> "MCAPWriteOptions": ...

class MCAPWriter:
    """
    A writer for logging messages to an MCAP file.

    Obtain an instance by calling :py:func:`open_mcap`.

    This class may be used as a context manager, in which case the writer will
    be closed when you exit the context.

    If the writer is not closed by the time it is garbage collected, it will be
    closed automatically, and any errors will be logged.
    """

    def __new__(
        cls,
        *,
        allow_overwrite: Optional[bool] = False,
        writer_options: Optional[MCAPWriteOptions] = None,
    ) -> "MCAPWriter": ...
    def __enter__(self) -> "MCAPWriter": ...
    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any) -> None: ...
    def close(self) -> None:
        """
        Close the writer explicitly.

        You may call this to explicitly close the writer. Note that the writer
        will be automatically closed when it is garbage-collected, or when
        exiting the context manager.
        """
        ...
