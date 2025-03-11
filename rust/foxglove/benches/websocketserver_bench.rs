use std::net::SocketAddr;
use std::time::Duration;

use foxglove::convert::SaturatingInto;
use foxglove::schemas::log::Level;
use foxglove::schemas::{
    Color, CubePrimitive, Log, Pose, Quaternion, SceneEntity, SceneUpdate, Timestamp, Vector3,
};
use foxglove::WebSocketServer;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};

fn main() {
    // Run registered benchmarks.
    divan::main();
}

const PRINTABLE: &str = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

#[divan::bench(args = [1, 2, 4, 8])]
fn roundtrip_json_message(bencher: divan::Bencher, num_clients: usize) {
    #[derive(Debug, serde::Serialize, schemars::JsonSchema)]
    struct CustomMessage {
        msg: &'static str,
        count: u32,
    }

    foxglove::static_typed_channel!(pub MSG_CHANNEL, "/json_msg", CustomMessage);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap();
    let server = runtime.block_on(async move {
        WebSocketServer::new()
            .bind("127.0.0.1", 0)
            .start()
            .await
            .unwrap()
    });
    let port = server.port();

    // Round trip 100 messages to the client
    bencher
        .with_inputs(|| {
            runtime.block_on(async move {
                let mut ws_clients = Vec::with_capacity(num_clients);
                for _ in 0..num_clients {
                    // Create a client and subscribe to the channel
                    let mut ws_client =
                        connect_client(format!("127.0.0.1:{}", port).parse().unwrap()).await;
                    let subscribe = json!({
                        "op": "subscribe",
                        "subscriptions": [
                            {
                                "id": 1,
                                "channelId": MSG_CHANNEL.id(),
                            }
                        ]
                    });
                    ws_client
                        .send(Message::text(subscribe.to_string()))
                        .await
                        .expect("Failed to send");

                    _ = ws_client.next().await.expect("No serverInfo sent");

                    // FG-10395 replace this with something more precise
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    ws_clients.push(ws_client);
                }
                ws_clients
            })
        })
        .bench_values(|mut ws_clients| {
            for _ in 0..50 {
                let message = CustomMessage {
                    msg: PRINTABLE,
                    count: (num_clients as u32) << 12,
                };
                MSG_CHANNEL.log(&message);
            }

            runtime.block_on(async move {
                for _ in 0..50 {
                    for ws_client in &mut ws_clients {
                        _ = ws_client.next().await.expect("missing message");
                    }
                }
            });
        });
}

#[divan::bench(args = [1, 2, 4, 8])]
fn roundtrip_scene_update(bencher: divan::Bencher, num_clients: usize) {
    foxglove::static_typed_channel!(pub SCENE_CHANNEL, "/boxes", SceneUpdate);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap();
    let server = runtime.block_on(async move {
        WebSocketServer::new()
            .bind("127.0.0.1", 0)
            .start()
            .await
            .unwrap()
    });
    let port = server.port();

    // Round trip 100 messages to the client
    bencher
        .with_inputs(|| {
            runtime.block_on(async move {
                let mut ws_clients = Vec::with_capacity(num_clients);
                for _ in 0..num_clients {
                    // Create a client and subscribe to the channel
                    let mut ws_client =
                        connect_client(format!("127.0.0.1:{}", port).parse().unwrap()).await;
                    let subscribe = json!({
                        "op": "subscribe",
                        "subscriptions": [
                            {
                                "id": 1,
                                "channelId": SCENE_CHANNEL.id(),
                            }
                        ]
                    });
                    ws_client
                        .send(Message::text(subscribe.to_string()))
                        .await
                        .expect("Failed to send");

                    _ = ws_client.next().await.expect("No serverInfo sent");

                    // FG-10395 replace this with something more precise
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    ws_clients.push(ws_client);
                }
                ws_clients
            })
        })
        .bench_values(|mut ws_clients| {
            let mut entities = Vec::with_capacity(num_clients);
            for i in 0..num_clients {
                entities.push(SceneEntity {
                    frame_id: "box".to_string(),
                    id: "box_1".to_string(),
                    lifetime: Some(Duration::from_millis(10_100).saturating_into()),
                    cubes: vec![CubePrimitive {
                        pose: Some(Pose {
                            position: Some(Vector3 {
                                x: 0.0,
                                y: 0.0,
                                z: 3.0,
                            }),
                            orientation: Some(Quaternion {
                                x: i as f64 * -0.1,
                                y: i as f64 * 0.1,
                                z: 0.0,
                                w: 1.0,
                            }),
                        }),
                        size: Some(Vector3 {
                            x: 1.0,
                            y: 1.0,
                            z: 1.0,
                        }),
                        color: Some(Color {
                            r: 1.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                    }],
                    ..Default::default()
                });
            }

            for _ in 0..50 {
                SCENE_CHANNEL.log(&SceneUpdate {
                    deletions: vec![],
                    entities: entities.clone(),
                });
            }

            runtime.block_on(async move {
                for _ in 0..50 {
                    for ws_client in &mut ws_clients {
                        _ = ws_client.next().await.expect("missing message");
                    }
                }
            });
        });
}

#[divan::bench(args = [1, 2, 4, 8, 16, 32])]
fn roundtrip_mutlithreaded(bencher: divan::Bencher, num_threads: usize) {
    foxglove::static_typed_channel!(pub LOG_CHANNEL, "/logs", Log);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap();
    let server = runtime.block_on(async move {
        WebSocketServer::new()
            .bind("127.0.0.1", 0)
            .start()
            .await
            .unwrap()
    });
    let port = server.port();

    const NUM_CLIENTS: usize = 4;

    bencher
        .with_inputs(|| {
            runtime.block_on(async move {
                let mut ws_clients = Vec::with_capacity(NUM_CLIENTS);
                for _ in 0..NUM_CLIENTS {
                    // Create a client and subscribe to the channel
                    let mut ws_client =
                        connect_client(format!("127.0.0.1:{}", port).parse().unwrap()).await;
                    let subscribe = json!({
                        "op": "subscribe",
                        "subscriptions": [
                            {
                                "id": 1,
                                "channelId": LOG_CHANNEL.id(),
                            }
                        ]
                    });
                    ws_client
                        .send(Message::text(subscribe.to_string()))
                        .await
                        .expect("Failed to send");

                    _ = ws_client.next().await.expect("No serverInfo sent");

                    // FG-10395 replace this with something more precise
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    ws_clients.push(ws_client);
                }
                ws_clients
            })
        })
        .bench_values(|mut ws_clients| {
            let num_messages = 2048 / num_threads;
            let mut threads = Vec::with_capacity(num_threads);
            for _ in 0..num_threads {
                let handle = std::thread::spawn(move || {
                    for _ in 0..num_messages {
                        LOG_CHANNEL.log(&Log {
                            timestamp: Some(Timestamp::new(1234567890, 123456789)),
                            level: Level::Info as i32,
                            message: PRINTABLE[..100].to_string(),
                            name: "node name".to_string(),
                            file: "file_name.rs".to_string(),
                            line: 1111,
                        });
                    }
                });
                threads.push(handle);
            }

            runtime.block_on(async move {
                for _ in 0..num_messages {
                    for ws_client in &mut ws_clients {
                        _ = ws_client.next().await.expect("missing message");
                    }
                }
            });

            for thread in threads {
                thread.join().unwrap();
            }
        });
}

/// Connect to a server, ensuring the protocol header is set, and return the client WS stream
async fn connect_client(
    addr: SocketAddr,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    const SUBPROTOCOL: &str = "foxglove.sdk.v1";

    let mut request = format!("ws://{}/", addr)
        .into_client_request()
        .expect("Failed to build request");

    request.headers_mut().insert(
        "sec-websocket-protocol",
        HeaderValue::from_static(SUBPROTOCOL),
    );

    let (ws_stream, response) = tokio_tungstenite::connect_async(request)
        .await
        .expect("Failed to connect");

    assert_eq!(
        response.headers().get("sec-websocket-protocol"),
        Some(&HeaderValue::from_static(SUBPROTOCOL))
    );

    ws_stream
}
