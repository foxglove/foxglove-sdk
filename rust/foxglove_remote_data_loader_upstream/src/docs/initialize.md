Initialize the [`Context`](UpstreamServer::Context) for the data source.

Declare channels using the [`ChannelRegistry`] and store them in the `Context` for later use.

The returned `Context` is passed directly to either [`metadata`](UpstreamServer::metadata) or [`stream`](UpstreamServer::stream), depending on the request.
