# Logging custom protobufs

This example demonstrates logging messages with custom protobuf schemas to an MCAP file.

## Running the example

This example requires additional dependencies, including the protobuf compiler.

The easiest way to run the example is in a Docker container, targeting linux amd64.

First, build the image, which will run CMake to build the example:

```sh
docker build --platform linux/amd64 -t foxglove-protobuf .
```

Next, run the example, providing a place to save the generated MCAP. Here, we'll save the file to
the ./output directory next to this file. By default, the SDK doesn't overwrite an MCAP file; you
can manually delete it if you want to run the example again.

```sh
docker run --platform linux/amd64 --rm -v './output:/app/output' -e MCAP_OUTPUT_PATH='/app/output/example.mcap' foxglove-protobuf
```

Now you can open the saved file in the Foxglove app, or inspect it with the [CLI](https://mcap.dev/guides/cli):

```sh
mcap info output/example.mcap
mcap list schemas output/example.mcap
```
