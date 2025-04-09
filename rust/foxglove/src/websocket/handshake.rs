use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::handshake::server;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{tungstenite, WebSocketStream};

pub(crate) static SUBPROTOCOL: &str = "foxglove.sdk.v1";

/// Add the subprotocol header to the response if the client requested it. If the client requests
/// subprotocols which don't contain ours, or does not include the expected header, return a 400.
pub(crate) async fn do_handshake(
    stream: TcpStream,
) -> Result<WebSocketStream<TcpStream>, tungstenite::Error> {
    tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &server::Request, mut res: server::Response| {
            let protocol_headers = req.headers().get_all("sec-websocket-protocol");
            for header in &protocol_headers {
                if header
                    .to_str()
                    .unwrap_or_default()
                    .split(',')
                    .any(|v| v.trim() == SUBPROTOCOL)
                {
                    res.headers_mut().insert(
                        "sec-websocket-protocol",
                        HeaderValue::from_static(SUBPROTOCOL),
                    );
                    return Ok(res);
                }
            }

            let resp = server::Response::builder()
                .status(400)
                .body(Some(
                    "Missing expected sec-websocket-protocol header".into(),
                ))
                .unwrap();

            Err(resp)
        },
    )
    .await
}
