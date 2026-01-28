Stream MCAP data.

Receives the [`Context`](UpstreamServer::Context) returned by [`initialize`](UpstreamServer::initialize).
Use channels from the context to log messages, then call [`StreamHandle::close`] when done.
