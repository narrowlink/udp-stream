#![doc = include_str!("../README.md")]

use bytes::{Buf, Bytes, BytesMut};
use std::{
    collections::HashMap,
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::UdpSocket,
    sync::{mpsc, Mutex},
};

const UDP_BUFFER_SIZE: usize = 17480; // 17kb
                                      // const UDP_TIMEOUT: u64 = 10 * 1000; // 10sec
const CHANNEL_LEN: usize = 100;

/// An I/O object representing a UDP socket listening for incoming connections.
///
/// This object can be converted into a stream of incoming connections for
/// various forms of processing.
///
/// # Examples
///
/// ```no_run
/// use udp_stream::UdpListener;
///
/// use std::{io, net::SocketAddr, error::Error, str::FromStr};
/// # async fn process_socket<T>(_socket: T) {}
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn Error>> {
///     let mut listener = UdpListener::bind(SocketAddr::from_str("127.0.0.1:8080")?).await?;
///
///     loop {
///         let socket = listener.accept().await?;
///         process_socket(socket).await;
///     }
/// }
/// ```
pub struct UdpListener {
    handler: tokio::task::JoinHandle<()>,
    receiver: Arc<Mutex<mpsc::Receiver<UdpStream>>>,
    local_addr: SocketAddr,
}

impl Drop for UdpListener {
    fn drop(&mut self) {
        self.handler.abort();
    }
}

impl UdpListener {
    /// Binds the `UdpListener` to the given local address.
    pub async fn bind(local_addr: SocketAddr) -> std::io::Result<Self> {
        let udp_socket = UdpSocket::bind(local_addr).await?;
        Self::from_tokio(udp_socket).await
    }

    /// Creates a `UdpListener` from an existing `tokio::net::UdpSocket`.
    pub async fn from_tokio(udp_socket: UdpSocket) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel(CHANNEL_LEN);
        let local_addr = udp_socket.local_addr()?;

        let handler = tokio::spawn(async move {
            let mut streams: HashMap<SocketAddr, mpsc::Sender<Bytes>> = HashMap::new();
            let socket = Arc::new(udp_socket);
            let (drop_tx, mut drop_rx) = mpsc::channel(1);

            let mut buf = BytesMut::with_capacity(UDP_BUFFER_SIZE * 3);
            loop {
                if buf.capacity() < UDP_BUFFER_SIZE {
                    buf.reserve(UDP_BUFFER_SIZE * 3);
                }
                tokio::select! {
                    Some(peer_addr) = drop_rx.recv() => {
                        streams.remove(&peer_addr);
                    }
                    Ok((len, peer_addr)) = socket.recv_buf_from(&mut buf) => {
                        match streams.get_mut(&peer_addr) {
                            Some(child_tx) => {
                                if let Err(err) = child_tx.send(buf.copy_to_bytes(len)).await {
                                    log::error!("child_tx.send {:?}", err);
                                    child_tx.closed().await;
                                    streams.remove(&peer_addr);
                                    continue;
                                }
                            }
                            None => {
                                let (child_tx, child_rx) = mpsc::channel(CHANNEL_LEN);
                                if let Err(err) = child_tx.send(buf.copy_to_bytes(len)).await {
                                    log::error!("child_tx.send {:?}", err);
                                    continue;
                                }
                                let udp_stream = UdpStream {
                                    local_addr,
                                    peer_addr,
                                    receiver: Arc::new(Mutex::new(child_rx)),
                                    socket: socket.clone(),
                                    handler: None,
                                    drop: Some(drop_tx.clone()),
                                    remaining: None,
                                };
                                if let Err(err) = tx.send(udp_stream).await {
                                    log::error!("tx.send {:?}", err);
                                    continue;
                                }
                                streams.insert(peer_addr, child_tx.clone());
                            }
                        }
                    }
                }
            }
        });
        Ok(Self {
            handler,
            receiver: Arc::new(Mutex::new(rx)),
            local_addr,
        })
    }

    ///Returns the local address that this socket is bound to.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    /// Accepts a new incoming UDP connection.
    ///
    /// This function waits for a new UDP connection and returns a `UdpStream`
    /// representing the connection to the remote endpoint. The `UdpStream` is
    /// wrapped in a `std::io::Result` to handle potential errors.
    pub async fn accept(&self) -> std::io::Result<UdpStream> {
        self.receiver
            .lock()
            .await
            .recv()
            .await
            .ok_or(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
    }
}

/// An I/O object representing a UDP stream connected to a remote endpoint.
///
/// A UDP stream can either be created by connecting to an endpoint, via the
/// [`connect`] method, or by [accepting] a connection from a [listener].
///
/// [`connect`]: struct.UdpStream.html#method.connect
/// [accepting]: struct.UdpListener.html#method.accept
/// [listener]: struct.UdpListener.html
#[derive(Debug)]
pub struct UdpStream {
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
    receiver: Arc<Mutex<mpsc::Receiver<Bytes>>>,
    socket: Arc<tokio::net::UdpSocket>,
    handler: Option<tokio::task::JoinHandle<()>>,
    drop: Option<mpsc::Sender<SocketAddr>>,
    remaining: Option<Bytes>,
}

impl Drop for UdpStream {
    fn drop(&mut self) {
        if let Some(handler) = &self.handler {
            handler.abort()
        }

        if let Some(drop) = &self.drop {
            let _ = drop.try_send(self.peer_addr);
        };
    }
}

impl UdpStream {
    /// Create a new UDP stream connected to the specified address.
    ///
    /// This function will create a new UDP socket and attempt to connect it to
    /// the `addr` provided. The returned future will be resolved once the
    /// stream has successfully connected, or it will return an error if one
    /// occurs.
    pub async fn connect(addr: SocketAddr) -> Result<Self, tokio::io::Error> {
        let local_addr: SocketAddr = if addr.is_ipv4() {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
        } else {
            SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)
        };
        let socket = UdpSocket::bind(local_addr).await?;
        Self::from_tokio(socket, addr).await
    }
    /// Creates a new UdpStream from a tokio::net::UdpSocket.
    /// This function is intended to be used to wrap a UDP socket from the tokio library.
    /// Note: The UdpSocket must have the UdpSocket::connect method called before invoking this function.
    pub async fn from_tokio(socket: UdpSocket, peer_addr: SocketAddr) -> Result<Self, tokio::io::Error> {
        let socket = Arc::new(socket);

        let local_addr = socket.local_addr()?;

        let (child_tx, child_rx) = mpsc::channel(CHANNEL_LEN);

        let socket_inner = socket.clone();

        let handler = tokio::spawn(async move {
            let mut buf = BytesMut::with_capacity(UDP_BUFFER_SIZE);
            while let Ok((len, received_addr)) = socket_inner.clone().recv_buf_from(&mut buf).await {
                if received_addr != peer_addr {
                    continue;
                }
                if child_tx.send(buf.copy_to_bytes(len)).await.is_err() {
                    child_tx.closed().await;
                    break;
                }

                if buf.capacity() < UDP_BUFFER_SIZE {
                    buf.reserve(UDP_BUFFER_SIZE * 3);
                }
            }
        });

        Ok(UdpStream {
            local_addr,
            peer_addr,
            receiver: Arc::new(Mutex::new(child_rx)),
            socket: socket.clone(),
            handler: Some(handler),
            drop: None,
            remaining: None,
        })
    }

    pub fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        Ok(self.peer_addr)
    }
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        Ok(self.local_addr)
    }
}

impl AsyncRead for UdpStream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<std::io::Result<()>> {
        if let Some(remaining) = self.remaining.as_mut() {
            if buf.remaining() < remaining.len() {
                buf.put_slice(&remaining.split_to(buf.remaining())[..]);
            } else {
                buf.put_slice(&remaining[..]);
                self.remaining = None;
            }
            return Poll::Ready(Ok(()));
        }

        let receiver = self.receiver.clone();
        let mut socket = match Pin::new(&mut Box::pin(receiver.lock())).poll(cx) {
            Poll::Ready(socket) => socket,
            Poll::Pending => return Poll::Pending,
        };

        match socket.poll_recv(cx) {
            Poll::Ready(Some(mut inner_buf)) => {
                if buf.remaining() < inner_buf.len() {
                    self.remaining = Some(inner_buf.split_off(buf.remaining()));
                };
                buf.put_slice(&inner_buf[..]);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for UdpStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        match self.socket.poll_send_to(cx, buf, self.peer_addr) {
            Poll::Ready(Ok(r)) => Poll::Ready(Ok(r)),
            Poll::Ready(Err(e)) => {
                if let Some(drop) = &self.drop {
                    let _ = drop.try_send(self.peer_addr);
                };
                Poll::Ready(Err(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<std::io::Result<()>> {
        let Some(drop) = &self.drop else {
            return Poll::Ready(Ok(()));
        };
        match Box::pin(drop.send(self.peer_addr)).as_mut().poll(cx) {
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}
