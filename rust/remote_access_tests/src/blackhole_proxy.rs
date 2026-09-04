//! A TCP proxy that can stop forwarding bytes without closing either socket.
//!
//! Simulates a link that goes dead without the OS noticing: no FIN, no RST, no
//! ICMP. Both peers keep an established socket that never delivers anything
//! again. This is the network condition that distinguishes a "clean" disconnect
//! (the peer closes, reads return EOF) from a wedged one (reads block forever),
//! and it is what the LiveKit signal client's teardown path has to survive.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tracing::info;

/// Polling interval used to notice the blackhole flag flipping mid-copy.
const FLAG_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// A TCP proxy that forwards to `target` until [`BlackholeProxy::blackhole`] is
/// called, after which it silently drops all traffic in both directions while
/// keeping every connection open.
pub struct BlackholeProxy {
    addr: SocketAddr,
    blackholed: Arc<AtomicBool>,
    accept_task: tokio::task::JoinHandle<()>,
}

impl Drop for BlackholeProxy {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

impl BlackholeProxy {
    /// Binds an ephemeral local port forwarding to `target` (`host:port`).
    pub async fn start(target: impl Into<String>) -> Result<Self> {
        let target = target.into();
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let blackholed = Arc::new(AtomicBool::new(false));

        let accept_task = tokio::spawn({
            let blackholed = blackholed.clone();
            async move {
                while let Ok((inbound, peer)) = listener.accept().await {
                    let target = target.clone();
                    let blackholed = blackholed.clone();
                    tokio::spawn(async move {
                        let outbound = match TcpStream::connect(&target).await {
                            Ok(s) => s,
                            Err(e) => {
                                info!("blackhole proxy: upstream connect failed: {e}");
                                return;
                            }
                        };
                        info!("blackhole proxy: {peer} -> {target}");
                        let (client_r, client_w) = inbound.into_split();
                        let (server_r, server_w) = outbound.into_split();
                        let up = tokio::spawn(pump(client_r, server_w, blackholed.clone()));
                        let down = tokio::spawn(pump(server_r, client_w, blackholed));
                        let _ = up.await;
                        let _ = down.await;
                    });
                }
            }
        });

        Ok(Self {
            addr,
            blackholed,
            accept_task,
        })
    }

    /// The address clients should connect to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops forwarding in both directions. Existing sockets stay open, so
    /// neither side observes a close.
    pub fn blackhole(&self) {
        info!("blackhole proxy: dropping all traffic");
        self.blackholed.store(true, Ordering::SeqCst);
    }
}

/// Copies `reader` into `writer` until either end closes or the blackhole flag
/// is set, at which point the task parks forever, holding both halves open.
async fn pump(mut reader: OwnedReadHalf, mut writer: OwnedWriteHalf, blackholed: Arc<AtomicBool>) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        if blackholed.load(Ordering::SeqCst) {
            // Park without dropping the halves: the sockets stay established.
            std::future::pending::<()>().await;
        }
        tokio::select! {
            result = reader.read(&mut buf) => match result {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if writer.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            },
            () = wait_for_blackhole(&blackholed) => {}
        }
    }
}

/// Resolves once the blackhole flag is set.
async fn wait_for_blackhole(blackholed: &AtomicBool) {
    while !blackholed.load(Ordering::SeqCst) {
        tokio::time::sleep(FLAG_POLL_INTERVAL).await;
    }
}
