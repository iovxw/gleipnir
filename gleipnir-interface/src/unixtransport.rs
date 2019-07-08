use std::io;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::{compat::*, prelude::*, ready};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use pin_utils::unsafe_pinned;
use serde::{Deserialize, Serialize};
use tokio::net::{UnixListener, UnixStream};

pub struct UnixTransport<Item, SinkItem> {
    pid: u32,
    inner: bincode_transport::Transport<UnixStream, Item, SinkItem>,
}

impl<Item, SinkItem> UnixTransport<Item, SinkItem> {
    unsafe_pinned!(inner: bincode_transport::Transport<UnixStream, Item, SinkItem>);

    pub fn new(io: UnixStream) -> UnixTransport<Item, SinkItem>
    where
        Item: for<'de> Deserialize<'de>,
        SinkItem: Serialize,
    {
        let pid = getsockopt(io.as_raw_fd(), PeerCredentials).unwrap().pid() as u32;
        UnixTransport {
            inner: io.into(),
            pid,
        }
    }
}

impl<Item, SinkItem> Stream for UnixTransport<Item, SinkItem>
where
    Item: for<'a> Deserialize<'a>,
{
    type Item = io::Result<Item>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<io::Result<Item>>> {
        self.inner().poll_next(cx)
    }
}

impl<Item, SinkItem> Sink<SinkItem> for UnixTransport<Item, SinkItem>
where
    SinkItem: Serialize,
{
    type Error = io::Error;

    fn start_send(self: Pin<&mut Self>, item: SinkItem) -> io::Result<()> {
        self.inner().start_send(item)
    }

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.inner().poll_ready(cx)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.inner().poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.inner().poll_close(cx)
    }
}

impl<Item, SinkItem> rpc::Transport for UnixTransport<Item, SinkItem>
where
    Item: for<'de> Deserialize<'de>,
    SinkItem: Serialize,
{
    type Item = Item;
    type SinkItem = SinkItem;

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        // HACK! return the pid of peer
        Ok((std::net::Ipv4Addr::from(self.pid), 0).into())
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        panic!("UnixTransport doesn't have a net::SocketAddr")
    }
}

/// Returns a new bincode transport that reads from and writes to `io`.

/// Connects to `addr`, wrapping the connection in a bincode transport.
pub async fn connect<Item, SinkItem>(addr: &str) -> io::Result<UnixTransport<Item, SinkItem>>
where
    Item: for<'de> Deserialize<'de>,
    SinkItem: Serialize,
{
    Ok(UnixTransport::new(
        UnixStream::connect(addr).compat().await?,
    ))
}

/// Listens on `addr`, wrapping accepted connections in bincode transports.
pub fn listen<P, Item, SinkItem>(addr: P) -> io::Result<Incoming<Item, SinkItem>>
where
    P: AsRef<Path>,
    Item: for<'de> Deserialize<'de>,
    SinkItem: Serialize,
{
    let listener = UnixListener::bind(addr)?;
    let incoming = listener.incoming().compat();
    Ok(Incoming {
        incoming,
        ghost: PhantomData,
    })
}

/// A [`TcpListener`] that wraps connections in bincode transports.
#[derive(Debug)]
pub struct Incoming<Item, SinkItem> {
    incoming: Compat01As03<tokio::net::unix::Incoming>,
    ghost: PhantomData<(Item, SinkItem)>,
}

impl<Item, SinkItem> Incoming<Item, SinkItem> {
    unsafe_pinned!(incoming: Compat01As03<tokio::net::unix::Incoming>);
}

impl<Item, SinkItem> Stream for Incoming<Item, SinkItem>
where
    Item: for<'a> Deserialize<'a>,
    SinkItem: Serialize,
{
    type Item = io::Result<UnixTransport<Item, SinkItem>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let next = ready!(self.incoming().poll_next(cx)?);
        Poll::Ready(next.map(|conn| Ok(UnixTransport::new(conn))))
    }
}
