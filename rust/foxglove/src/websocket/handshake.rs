use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::tungstenite::handshake::server;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{WebSocketStream, tungstenite};

pub(crate) const SUBPROTOCOL_V1: &str = "foxglove.sdk.v1";
pub(crate) const SUBPROTOCOL_V2: &str = "foxglove.sdk.v2";

/// The WebSocket subprotocol negotiated with a client.
///
/// The subprotocol is fixed for the lifetime of a connection, and determines the shape of the
/// server `advertise` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Subprotocol {
    /// `foxglove.sdk.v1`: channel advertisements carry inline schemas.
    V1,
    /// `foxglove.sdk.v2`: channel advertisements reference a connection-scoped schema table.
    V2,
}

impl Subprotocol {
    /// Returns the subprotocol identifier, as used in the `sec-websocket-protocol` header.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Subprotocol::V1 => SUBPROTOCOL_V1,
            Subprotocol::V2 => SUBPROTOCOL_V2,
        }
    }

    /// Returns the subprotocol identified by `name`, if it is one the server supports.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            SUBPROTOCOL_V1 => Some(Subprotocol::V1),
            SUBPROTOCOL_V2 => Some(Subprotocol::V2),
            _ => None,
        }
    }
}

/// Selects the subprotocol to use for a connection, given the subprotocols offered by the client.
///
/// The server prefers the newest subprotocol it supports, regardless of the order in which the
/// client listed them. Returns `None` if the client did not offer a supported subprotocol.
fn select_subprotocol<'a>(offered: impl IntoIterator<Item = &'a str>) -> Option<Subprotocol> {
    offered
        .into_iter()
        .filter_map(|name| Subprotocol::from_name(name.trim()))
        .max_by_key(|subprotocol| match subprotocol {
            Subprotocol::V1 => 1,
            Subprotocol::V2 => 2,
        })
}

/// Negotiates a subprotocol with the client and completes the WebSocket handshake.
///
/// If the client requests subprotocols which don't include one of ours, or does not include the
/// expected header, respond with a 400.
pub(crate) async fn do_handshake<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
) -> Result<(WebSocketStream<S>, Subprotocol), tungstenite::Error> {
    let mut negotiated = None;
    let stream = tokio_tungstenite::accept_hdr_async(
        stream,
        #[allow(clippy::result_large_err)]
        |req: &server::Request, mut res: server::Response| {
            let offered = req
                .headers()
                .get_all("sec-websocket-protocol")
                .iter()
                .flat_map(|header| header.to_str().unwrap_or_default().split(','));
            let Some(subprotocol) = select_subprotocol(offered) else {
                return Err(server::Response::builder()
                    .status(400)
                    .body(Some(
                        "Missing expected sec-websocket-protocol header".into(),
                    ))
                    .unwrap());
            };
            res.headers_mut().insert(
                "sec-websocket-protocol",
                HeaderValue::from_static(subprotocol.as_str()),
            );
            negotiated = Some(subprotocol);
            Ok(res)
        },
    )
    .await?;
    let subprotocol = negotiated.expect("handshake callback sets the subprotocol on success");
    Ok((stream, subprotocol))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_subprotocol_prefers_v2() {
        assert_eq!(
            select_subprotocol(["foxglove.sdk.v2", "foxglove.sdk.v1"]),
            Some(Subprotocol::V2)
        );
        assert_eq!(
            select_subprotocol(["foxglove.sdk.v1", " foxglove.sdk.v2"]),
            Some(Subprotocol::V2)
        );
    }

    #[test]
    fn test_select_subprotocol_falls_back_to_v1() {
        assert_eq!(
            select_subprotocol(["unknown", "foxglove.sdk.v1", "another"]),
            Some(Subprotocol::V1)
        );
    }

    #[test]
    fn test_select_subprotocol_rejects_unknown() {
        assert_eq!(select_subprotocol(["unknown", "foxglove.sdk.v3"]), None);
        assert_eq!(select_subprotocol([]), None);
    }
}
