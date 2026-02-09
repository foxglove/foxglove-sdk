Stream time-ordered MCAP data.

Implementations must use the [`Channel`]s created in
[`initialize`](UpstreamServer::initialize) to log messages.

Receives the [`Context`](UpstreamServer::Context) returned by
`initialize`.

Buffer management is automatic: the MCAP buffer is flushed whenever it
exceeds a configurable threshold (set via the `FOXGLOVE_FLUSH_THRESHOLD`
environment variable, default 1 MiB). The stream is finalized when this
method returns.

# Ordering requirements

The output **must be time-ordered.** Messages are sent in the order
they are logged, so they must be logged in monotonically increasing timestamp
order, including across different channels.

Violating this requirement will cause playback errors in the Foxglove app.
