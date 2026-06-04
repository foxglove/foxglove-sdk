# Spike: RemoteAccessGateway on Ubuntu 22.04 (jammy, glibc 2.35) — the candidate
# base for a ROS 1 Noetic foxglove_bridge image. Noetic's official base (focal)
# has glibc 2.31, which is too old for the SDK's remote access support.
#
# Build from the foxglove-sdk repo root (requires `make build-cpp-dist` first):
#   docker build -f ros/ros1_spike/Dockerfile -t ros1-spike .
#
# Run against the local stack (LiveKit compose + API server):
#   docker run --rm --network host \
#     -e FOXGLOVE_DEVICE_TOKEN=... \
#     -e FOXGLOVE_API_URL=http://localhost:3000/api \
#     ros1-spike

FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# SDK dist produced by `make build-cpp-dist` (jammy-built, remote-access ON)
COPY cpp/dist /sdk/cpp/dist

# Surface any missing runtime deps of the remote-access cdylib early.
RUN ldd /sdk/cpp/dist/lib/libfoxglove.so | tee /tmp/ldd.out \
    && ! grep "not found" /tmp/ldd.out

COPY ros/ros1_spike /spike
WORKDIR /spike

RUN cmake -B build -DCMAKE_BUILD_TYPE=Release -DFOXGLOVE_CPP_SDK_DIR=/sdk/cpp/dist \
    && cmake --build build -j"$(nproc)"

ENV LD_LIBRARY_PATH=/sdk/cpp/dist/lib

CMD ["/spike/build/ros1_spike"]
