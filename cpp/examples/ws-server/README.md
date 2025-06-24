# WS-Server Example

## Running in Docker

You can run this example in a Docker container, targeting linux amd64 (ubuntu-22).

First, build the image, which will run CMake to build the example:

```sh
docker build --platform linux/amd64 -t ws-server .
```

Next, run the example, providing a place to save the generated MCAP. Here, we'll save the file to
the ./output directory next to this file. By default, the SDK doesn't overwrite an MCAP file; you
can manually delete it if you want to run the example again.

```sh
docker run --platform linux/amd64 --rm -v './output:/app/output' ws-server
```
