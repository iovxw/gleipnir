use std::io;
use std::marker::PhantomData;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::{prelude::*, ready};
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use tarpc::serde_transport::Transport;
use tokio::net::{UnixListener, UnixStream};
use tokio_serde::{Deserializer, Serializer};

/// Returns a new JSON transport that reads from and writes to `io`.
pub fn new<Item, SinkItem, Codec>(
    io: UnixStream,
    codec: Codec,
) -> (u32, Transport<UnixStream, Item, SinkItem, Codec>)
where
    Item: for<'de> Deserialize<'de>,
    SinkItem: Serialize,
    Codec: Serializer<SinkItem> + Deserializer<Item>,
{
    let peer_pid = getsockopt(io.as_raw_fd(), PeerCredentials).unwrap().pid() as u32;
    (peer_pid, Transport::from((io, codec)))
}

/// Connects to `addr`, wrapping the connection in a JSON transport.
pub async fn connect<A, Item, SinkItem, Codec>(
    addr: A,
    codec: Codec,
) -> io::Result<(u32, Transport<UnixStream, Item, SinkItem, Codec>)>
where
    A: AsRef<Path>,
    Item: for<'de> Deserialize<'de>,
    SinkItem: Serialize,
    Codec: Serializer<SinkItem> + Deserializer<Item>,
{
    Ok(new(UnixStream::connect(addr).await?, codec))
}

/// Listens on `addr`, wrapping accepted connections in JSON transports.
pub async fn listen<A, Item, SinkItem, Codec, CodecFn>(
    addr: A,
    codec_fn: CodecFn,
) -> io::Result<Incoming<Item, SinkItem, Codec, CodecFn>>
where
    A: AsRef<Path>,
    Item: for<'de> Deserialize<'de>,
    Codec: Serializer<SinkItem> + Deserializer<Item>,
    CodecFn: Fn() -> Codec,
{
    let listener = UnixListener::bind(addr)?;
    let local_addr = listener.local_addr()?;
    Ok(Incoming {
        listener,
        codec_fn,
        local_addr,
        ghost: PhantomData,
    })
}

/// A [`TcpListener`] that wraps connections in JSON transports.
#[pin_project]
#[derive(Debug)]
pub struct Incoming<Item, SinkItem, Codec, CodecFn> {
    listener: UnixListener,
    local_addr: SocketAddr,
    codec_fn: CodecFn,
    ghost: PhantomData<(Item, SinkItem, Codec)>,
}

impl<Item, SinkItem, Codec, CodecFn> Incoming<Item, SinkItem, Codec, CodecFn> {
    /// Returns the address being listened on.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr.clone()
    }
}

impl<Item, SinkItem, Codec, CodecFn> Stream for Incoming<Item, SinkItem, Codec, CodecFn>
where
    Item: for<'de> Deserialize<'de>,
    SinkItem: Serialize,
    Codec: Serializer<SinkItem> + Deserializer<Item>,
    CodecFn: Fn() -> Codec,
{
    type Item = io::Result<(u32, Transport<UnixStream, Item, SinkItem, Codec>)>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let next =
            ready!(Pin::new(&mut self.as_mut().project().listener.incoming()).poll_next(cx)?);
        Poll::Ready(next.map(|conn| Ok(new(conn, (self.codec_fn)()))))
    }
}
