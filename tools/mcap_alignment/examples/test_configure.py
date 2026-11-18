import json
import sys
from pathlib import Path

import configure
import pytest


def test_first_tick_waits_for_telemetry(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    channels = [
        dict(
            topic="/camera",
            schema="foxglove.CompressedVideo",
            channel=1,
            messages=4,
            first_log_time=100,
            last_log_time=400,
        ),
        dict(
            topic="/action",
            schema="example.Action",
            channel=2,
            messages=2,
            first_log_time=150,
            last_log_time=300,
        ),
    ]
    monkeypatch.setattr(
        configure, "inspect", lambda _: {"files": [{"channels": channels}]}
    )
    job = tmp_path / "job.json"
    monkeypatch.setattr(
        sys,
        "argv",
        ["configure", "in.mcap", "out.mcap", str(job), "--telemetry", "/action"],
    )
    configure.main()
    config = json.loads(job.read_text())
    assert config["timeline"]["origin"] == 150
    assert config["episode"] == {"start": 100, "end": 301}
    assert config["streams"][1]["independence"] == "declared"
