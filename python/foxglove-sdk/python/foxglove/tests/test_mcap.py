import os
from pathlib import Path
from typing import Generator

import pytest
from foxglove import open_mcap
from foxglove.channel import Channel

chan = Channel("test", schema={"type": "object"})


@pytest.fixture
def tmp_mcap(tmpdir: os.PathLike[str]) -> Generator[Path, None, None]:
    dir = Path(tmpdir)
    mcap = dir / "test.mcap"
    yield mcap
    mcap.unlink()
    dir.rmdir()


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
