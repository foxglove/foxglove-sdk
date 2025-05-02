from pathlib import Path
from typing import Callable, Generator, Optional

import pytest
from foxglove import Context, open_mcap
from foxglove.channel import Channel
from foxglove.mcap import MCAPWriteOptions

chan = Channel("test", schema={"type": "object"})


@pytest.fixture
def make_tmp_mcap(
    tmp_path_factory: pytest.TempPathFactory,
) -> Generator[Callable[[], Path], None, None]:
    mcap: Optional[Path] = None
    dir: Optional[Path] = None

    def _make_tmp_mcap() -> Path:
        nonlocal dir, mcap
        dir = tmp_path_factory.mktemp("test", numbered=True)
        mcap = dir / "test.mcap"
        return mcap

    yield _make_tmp_mcap

    if mcap is not None and dir is not None:
        try:
            mcap.unlink()
            dir.rmdir()
        except FileNotFoundError:
            pass


@pytest.fixture
def tmp_mcap(make_tmp_mcap: Callable[[], Path]) -> Generator[Path, None, None]:
    yield make_tmp_mcap()


def test_open_with_str(tmp_mcap: Path) -> None:
    open_mcap(str(tmp_mcap))


def test_overwrite(tmp_mcap: Path) -> None:
    tmp_mcap.touch()
    with pytest.raises(FileExistsError):
        open_mcap(tmp_mcap)
    open_mcap(tmp_mcap, allow_overwrite=True)


def test_explicit_close(tmp_mcap: Path) -> None:
    mcap = open_mcap(tmp_mcap)
    for ii in range(20):
        chan.log({"foo": ii})
    size_before_close = tmp_mcap.stat().st_size
    mcap.close()
    assert tmp_mcap.stat().st_size > size_before_close


def test_context_manager(tmp_mcap: Path) -> None:
    with open_mcap(tmp_mcap):
        for ii in range(20):
            chan.log({"foo": ii})
        size_before_close = tmp_mcap.stat().st_size
    assert tmp_mcap.stat().st_size > size_before_close


def test_writer_compression(make_tmp_mcap: Callable[[], Path]) -> None:
    tmp_1 = make_tmp_mcap()
    tmp_2 = make_tmp_mcap()

    # Compression is enabled by default
    mcap_1 = open_mcap(tmp_1)
    mcap_2 = open_mcap(tmp_2, writer_options=MCAPWriteOptions(compression=None))

    for _ in range(20):
        chan.log({"foo": "bar"})

    mcap_1.close()
    mcap_2.close()

    assert tmp_1.stat().st_size < tmp_2.stat().st_size


def test_writer_custom_profile(tmp_mcap: Path) -> None:
    options = MCAPWriteOptions(profile="--custom-profile-1--")
    with open_mcap(tmp_mcap, writer_options=options):
        chan.log({"foo": "bar"})

    contents = tmp_mcap.read_bytes()
    assert contents.find(b"--custom-profile-1--") > -1


def test_write_to_different_contexts(make_tmp_mcap: Callable[[], Path]) -> None:
    tmp_1 = make_tmp_mcap()
    tmp_2 = make_tmp_mcap()

    ctx1 = Context()
    ctx2 = Context()

    options = MCAPWriteOptions(compression=None)
    mcap1 = open_mcap(tmp_1, writer_options=options, context=ctx1)
    mcap2 = open_mcap(tmp_2, writer_options=options, context=ctx2)

    ch1 = Channel("ctx1", context=ctx1)
    ch1.log({"a": "b"})

    ch2 = Channel("ctx2", context=ctx2)
    ch2.log({"has-more-data": "true"})

    mcap1.close()
    mcap2.close()

    contents1 = tmp_1.read_bytes()
    contents2 = tmp_2.read_bytes()

    assert len(contents1) < len(contents2)
