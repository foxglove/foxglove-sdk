Parameters that identify the data to load.

In the Foxglove app, remote data sources are opened using a URL like:

```text
https://app.foxglove.dev/view?ds=remote-data-loader&ds.dataLoaderUrl=https%3A%2F%2Fremote-data-loader.example.com&ds.flightId=ABC&ds.startTime=2024-01-01T00:00:00Z
```

The `ds.*` parameters (except `ds.dataLoaderUrl`) are forwarded to your upstream server with
the `ds.` prefix stripped:

```text
GET /v1/manifest?flightId=ABC&startTime=2024-01-01T00:00:00Z
GET /v1/data?flightId=ABC&startTime=2024-01-01T00:00:00Z
```

These parameters are deserialized into an instance of
[`QueryParams`](`UpstreamServer::QueryParams`) using [`serde::Deserialize`].
