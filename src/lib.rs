use std::{
    collections::HashMap,
    io::{self},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use std::future::Future;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::UdpSocket,
    sync::mpsc,
    sync::Mutex as AsyncMutex,
};

macro_rules! pin_mut {
    ($($x:ident),*) => { $(
        let mut $x = $x;
        #[allow(unused_mut)]
        let mut $x = unsafe {
            Pin::new_unchecked(&mut $x)
        };
    )* }
}

const UDP_BUFFER_SIZE: usize = 17480; // 17kb
                                      // const UDP_TIMEOUT: u64 = 10 * 1000; // 10sec
const CHANNEL_LEN: usize = 100;

pub struct UdpListener {
    handler: tokio::task::JoinHandle<()>,
    receiver: Arc<AsyncMutex<mpsc::Receiver<(UdpStream, SocketAddr)>>>,
    local_addr: SocketAddr,
}

impl Drop for UdpListener {
    fn drop(&mut self) {
        self.handler.abort();
    }
}

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
/// use std::{io, net::SocketAddr};
/// # async fn process_socket<T>(_socket: T) {}
///
/// #[tokio::main]
/// async fn main() -> io::Result<()> {
///     let mut listener = UdpListener::bind(SocketAddr::from_str("127.0.0.1:8080").unwrap()).await?;
///
///     loop {
///         let (socket, _) = listener.accept().await?;
///         process_socket(socket).await;
///     }
/// }
/// ```
impl UdpListener {
    pub async fn bind(local_addr: SocketAddr) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel(CHANNEL_LEN);
        let udp_socket = UdpSocket::bind(local_addr).await?;
        let local_addr = udp_socket.local_addr()?;

        let handler = tokio::spawn(async move {
            let mut streams: HashMap<
                SocketAddr,
                (mpsc::Sender<(Vec<u8>, usize)>, Arc<std::sync::Mutex<bool>>),
            > = HashMap::new();
            let socket = Arc::new(udp_socket);
            loop {
                let mut buf = vec![0u8; UDP_BUFFER_SIZE];
                if let Ok((len, addr)) = socket.recv_from(&mut buf).await {
                    for (k, (_, drop)) in streams.clone().into_iter() {
                        if let Ok(drop) = drop.try_lock() {
                            if *drop == true {
                                streams.remove(&k.clone());
                                continue;
                            }
                        }
                    }
                    match streams.get_mut(&addr) {
                        Some((child_tx, _)) => {
                            if let Err(_) = child_tx.send((buf, len)).await {
                                child_tx.closed().await;
                                streams.remove(&addr);
                                continue;
                            }
                        }
                        None => {
                            let (child_tx, child_rx): (
                                mpsc::Sender<(Vec<u8>, usize)>,
                                mpsc::Receiver<(Vec<u8>, usize)>,
                            ) = mpsc::channel(CHANNEL_LEN);
                            let drop = Arc::new(Mutex::new(false));
                            if child_tx.send((buf, len)).await.is_err()
                                || tx
                                    .send((
                                        UdpStream {
                                            local_addr: local_addr,
                                            peer_addr: addr,
                                            receiver: Arc::new(Mutex::new(child_rx)),
                                            socket: socket.clone(),
                                            handler: None,
                                            drop: drop.clone(),
                                        },
                                        addr,
                                    ))
                                    .await
                                    .is_err()
                            {
                                continue;
                            };
                            streams.insert(addr, (child_tx.clone(), drop));
                        }
                    }
                }
                //error?
            }
        });
        Ok(Self {
            handler,
            receiver: Arc::new(AsyncMutex::new(rx)),
            local_addr,
        })
    }
    ///Returns the local address that this socket is bound to.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr.clone())
    }
    pub async fn accept(&self) -> io::Result<(UdpStream, SocketAddr)> {
        (&self.receiver)
            .lock()
            .await
            .recv()
            .await
            .ok_or(io::Error::new(io::ErrorKind::BrokenPipe, "Broken Pipe"))
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
    receiver: Arc<Mutex<mpsc::Receiver<(Vec<u8>, usize)>>>,
    socket: Arc<tokio::net::UdpSocket>,
    handler: Option<tokio::task::JoinHandle<()>>,
    drop: Arc<Mutex<bool>>,
}

impl Drop for UdpStream {
    fn drop(&mut self) {
        if let Some(handler) = &self.handler {
            handler.abort()
        }
        if let Ok(mut drop) = (&self.drop).lock() {
            *drop = true;
        }
    }
}

impl UdpStream {
    /// Create a new UDP stream connected to the specified address.
    ///
    /// This function will create a new UDP socket and attempt to connect it to
    /// the `addr` provided. The returned future will be resolved once the
    /// stream has successfully connected, or it will return an error if one
    /// occurs.
    #[allow(unused)]
    pub async fn connect(addr: SocketAddr) -> Result<Self, tokio::io::Error> {
        let local_addr: SocketAddr = if addr.is_ipv4() {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0)
        } else {
            SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 0)
        };

        let socket = Arc::new(UdpSocket::bind(local_addr).await?);
        let local_addr = socket.local_addr()?;
        socket.connect(&addr);
        let (child_tx, child_rx): (
            mpsc::Sender<(Vec<u8>, usize)>,
            mpsc::Receiver<(Vec<u8>, usize)>,
        ) = mpsc::channel(CHANNEL_LEN);

        let drop = Arc::new(Mutex::new(false));
        let socket_inner = socket.clone();
        let handler = tokio::spawn(async move {
            let mut buf = vec![0u8; UDP_BUFFER_SIZE];
            while let Ok((len, addr)) = socket_inner.clone().recv_from(&mut buf).await {
                if child_tx.send((buf.clone(), len)).await.is_err() {
                    child_tx.closed();
                    break;
                }
            }
        });
        Ok(UdpStream {
            local_addr: local_addr,
            peer_addr: addr,
            receiver: Arc::new(Mutex::new(child_rx)),
            socket: socket.clone(),
            handler: Some(handler),
            drop: drop,
        })
    }
    #[allow(unused)]
    pub fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        Ok(self.peer_addr)
    }
    #[allow(unused)]
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        Ok(self.local_addr)
    }
    #[allow(unused)]
    pub fn shutdown(&self) {
        if let Ok(mut drop) = (&self.drop).lock() {
            *drop = true;
        }
    }
}

impl AsyncRead for UdpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        let mut socket = match (&self.receiver).lock() {
            Ok(socket) => socket,
            Err(_) => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "Broken Pipe",
                )))
            }
        };
        // let mut socket = (&self.receiver).lock().unwrap();
        let future = socket.recv();
        pin_mut!(future);
        future.poll(cx).map(|res| match res {
            Some((inner_buf, len)) => {
                buf.put_slice(&inner_buf[..len]);
                Ok(())
            }
            None => Err(io::Error::new(io::ErrorKind::BrokenPipe, "Broken Pipe")),
        })
    }
}

impl AsyncWrite for UdpStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        let future = self.socket.send_to(&buf, self.peer_addr);
        pin_mut!(future);
        future.poll(cx).map(|r| match r {
            Ok(r) => Ok(r),
            Err(_e) => {
                if let Ok(mut drop) = (&self.drop).lock() {
                    *drop = true;
                }
                Err(_e)
            }
        })
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
