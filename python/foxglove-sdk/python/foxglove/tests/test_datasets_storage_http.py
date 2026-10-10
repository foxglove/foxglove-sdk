import re
import threading
from collections.abc import Callable
from datetime import timedelta
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import IO, Any, cast
from urllib.parse import urlsplit

import pytest
from foxglove.datasets.storage import ObjectLocation, _CloudObjectStore, _iter_messages

from .datasets_helpers import START, mcap_bytes

s3 = pytest.importorskip("pyarrow.fs")


@pytest.mark.parametrize("deny_reads", [False, True])
@pytest.mark.parametrize("builtin", [False, True])
def test_native_s3_range_reads_are_sparse_and_propagate_errors(
    monkeypatch: pytest.MonkeyPatch,
    deny_reads: bool,
    builtin: bool,
    record_property: Callable[[str, object], None],
) -> None:
    import requests

    data = mcap_bytes(
        [
            (topic, index * 1_000_000_000, {"value": index, "padding": "x" * 16384})
            for index in range(100)
            for topic in ("/selected", "/other")
        ]
    )
    location = ObjectLocation("bucket", "prefix/run.mcap", scheme="s3")
    ranges: list[str | None] = []
    fetched_sizes: list[int] = []
    paths: list[str] = []

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, _format: str, *_args: Any) -> None:
            pass

        def headers_for(self, code: int, size: int) -> None:
            self.send_response(code)
            self.send_header("Content-Length", str(size))
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("ETag", '"test-object"')
            self.send_header("Last-Modified", "Thu, 01 Jan 2026 00:00:00 GMT")
            self.send_header("Accept-Ranges", "bytes")

        def do_HEAD(self) -> None:
            paths.append(urlsplit(self.path).path)
            self.headers_for(200, len(data))
            self.end_headers()

        def do_GET(self) -> None:
            paths.append(urlsplit(self.path).path)
            range_header = self.headers.get("Range")
            ranges.append(range_header)
            if deny_reads:
                error = b"<Error><Code>AccessDenied</Code><Message>Forbidden</Message></Error>"
                self.headers_for(403, len(error))
                self.end_headers()
                self.wfile.write(error)
                return
            match = re.fullmatch(r"bytes=(\d+)-(\d*)", range_header or "")
            if match is None:
                self.headers_for(400, 0)
                self.end_headers()
                return
            start = int(match.group(1))
            end = min(
                int(match.group(2)) if match.group(2) else len(data) - 1, len(data) - 1
            )
            payload = data[start : end + 1]
            fetched_sizes.append(len(payload))
            self.headers_for(206, len(payload))
            self.send_header("Content-Range", f"bytes {start}-{end}/{len(data)}")
            self.end_headers()
            self.wfile.write(payload)

    def unexpected_api_request(*_args: Any, **_kwargs: Any) -> None:
        raise AssertionError("S3 object storage reads must not use the Foxglove API")

    monkeypatch.setattr(requests.Session, "request", unexpected_api_request)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        filesystem = s3.S3FileSystem(
            access_key="dummy",
            secret_key="dummy",
            region="us-east-1",
            scheme="http",
            endpoint_override=f"127.0.0.1:{server.server_port}",
            connect_timeout=2,
            request_timeout=2,
            retry_strategy=s3.AwsStandardS3RetryStrategy(max_attempts=1),
        )

        class S3Store:
            def open(self, object_location: ObjectLocation) -> IO[bytes]:
                assert object_location == location
                return cast(
                    IO[bytes],
                    filesystem.open_input_file(
                        f"{object_location.bucket}/{object_location.path}"
                    ),
                )

        monkeypatch.setattr(s3, "resolve_s3_region", lambda bucket: "us-east-1")
        monkeypatch.setattr(s3, "S3FileSystem", lambda **kwargs: filesystem)
        stamp = START + timedelta(seconds=50)
        stream = _iter_messages(
            _CloudObjectStore() if builtin else S3Store(),
            [location],
            ["/selected"],
            stamp,
            stamp,
            None,
        )
        if deny_reads:
            with pytest.raises(
                OSError, match="403|ACCESS_DENIED|AccessDenied|Forbidden"
            ):
                list(stream)
            assert fetched_sizes == []
        else:
            samples = list(stream)
            assert [
                (channel.topic, decoded["value"]) for _, channel, _, decoded in samples
            ] == [("/selected", 50)]
            assert sum(fetched_sizes) < len(data) // 10
            assert len(ranges) <= 6
            record_property("object_bytes", len(data))
            record_property("fetched_bytes", sum(fetched_sizes))
            record_property("range_requests", len(ranges))
        assert ranges and all(ranges)
        assert set(paths) == {"/bucket/prefix/run.mcap"}
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
        assert not thread.is_alive()
