Called on every request to declare channels and initialize the [`Context`](UpstreamServer::Context).

Calling [`ChannelRegistry::channel`] adds a channel to the response for this request. The complete set of declared channels **must be uniquely determined** by the received [`QueryParams`](UpstreamServer::QueryParams). Violating this requirement may cause unexpected behavior (e.g. topics missing in the Foxglove app).

The returned `Context` is passed through verbatim to either [`metadata`](UpstreamServer::metadata) or [`stream`](UpstreamServer::stream), depending on the request.

# Notes

The [`Channel`] returned from [`ChannelRegistry::channel`] can be stored in your `Context` type. You will want to do this to log messages in `stream`.
