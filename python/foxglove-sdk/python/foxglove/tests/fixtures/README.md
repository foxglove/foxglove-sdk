# Video fixtures

`h264.json` contains eight synthetic 48×32 RGB images encoded as H.264 Annex B access
units, base64-encoded individually. Frames 0 and 4 are IDR keyframes; intervening
frames depend on earlier frames. `h264_b_frames.json` uses the same images with two
B-frames between reference frames to exercise rejection. Neither contains customer data.

Generated with PyAV 16.1.0 and its bundled libx264 encoder:

```python
import av
import base64
import json
from fractions import Fraction

import numpy as np

for name, bframes in [("h264", 0), ("h264_b_frames", 2)]:
    codec = av.CodecContext.create("libx264", "w")
    codec.width, codec.height = 48, 32
    codec.pix_fmt = "yuv420p"
    codec.time_base = Fraction(1, 30)
    codec.options = {
        "crf": "18",
        "preset": "medium",
        "x264-params": (
            f"keyint=4:min-keyint=4:scenecut=0:bframes={bframes}:b-adapt=0"
        ),
    }
    packets = []
    for index in range(8):
        pixels = np.zeros((32, 48, 3), dtype=np.uint8)
        pixels[:] = [40 + index * 15, 100, 180 - index * 10]
        pixels[8:24, index * 4:index * 4 + 8] = [220, 30, 80]
        frame = av.VideoFrame.from_ndarray(pixels, format="rgb24")
        frame.pts = index
        packets.extend(codec.encode(frame))
    packets.extend(codec.encode(None))
    with open(f"{name}.json", "w") as output:
        json.dump({"packets": [
            base64.b64encode(bytes(packet)).decode() for packet in packets
        ]}, output, indent=2)
        output.write("\n")
```

Tests use pixel tolerances rather than exact RGB hashes to allow differences in FFmpeg
color conversion, and compare windowed decoding with full decoding pixel-for-pixel.

# TLS fixtures

`tls_cert.pem` and `tls_key.pem` are a self-signed X.509 certificate and matching PKCS#8
private key used only to exercise the WebSocket server's TLS support in tests. They are
valid for `127.0.0.1` and `localhost` and are not trusted by anything outside these tests.

Generated with:

```sh
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 36500 \
  -subj "/CN=foxglove-sdk-test" \
  -addext "subjectAltName=IP:127.0.0.1,DNS:localhost" \
  -addext "basicConstraints=critical,CA:TRUE" \
  -keyout tls_key.pem -out tls_cert.pem
```
