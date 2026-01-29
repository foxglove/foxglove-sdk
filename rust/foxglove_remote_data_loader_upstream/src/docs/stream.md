Stream time-ordered MCAP data.

Implementations must use the [`Channel`]s created in
[`initialize`](UpstreamServer::initialize) to log messages.

Receives the [`Context`](UpstreamServer::Context) returned by
`initialize`.

# Ordering requirements

The output **must be time-ordered.** Messages are sent in the order
they are logged, so they must be logged in monotonically increasing timestamp
order, including across different channels.

Violating this requirement will cause playback errors in the Foxglove app.
