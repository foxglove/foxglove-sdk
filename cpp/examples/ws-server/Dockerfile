FROM ubuntu:22.04

WORKDIR /app

RUN apt-get -y update
RUN apt-get install -y cmake clang

ENV CC=clang
ENV CXX=clang++

COPY CMakeLists.txt .
COPY src/main.cpp .

RUN mkdir -p /app/output
VOLUME /app/output

RUN mkdir build \
  && cd build \
  && cmake .. \
  && cmake --build .

CMD ["./build/ws-server"]
