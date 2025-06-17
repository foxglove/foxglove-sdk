FROM ubuntu:24.04

WORKDIR /app

RUN apt-get -y update
RUN apt-get install -y cmake clang protobuf-compiler

ENV CC=clang
ENV CXX=clang++

COPY CMakeLists.txt .
COPY main.cpp .
COPY protos/* protos/

RUN mkdir -p /app/output
VOLUME /app/output

RUN mkdir build \
  && cd build \
  && cmake .. \
  && cmake --build .

CMD ["./build/custom_protobuf"]
