use std::{
    collections::HashMap,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::UdpSocket,
    sync::mpsc,
};

const UDP_BUFFER_SIZE: usize = 17480; // 17kb
                                      //const UDP_TIMEOUT: u64 = 10 * 1000; // 10sec
const CHANNEL_LEN: usize = 100;

macro_rules! pin_mut {
    ($($x:ident),*) => { $(
        let mut $x = $x;
        #[allow(unused_mut)]
        let mut $x = unsafe {
            Pin::new_unchecked(&mut $x)
        };
    )* }
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
pub struct UdpListener {
    #[allow(unused)]
    local_addr: SocketAddr,
    #[allow(unused)]
    receiver: mpsc::Receiver<(mpsc::Receiver<(Vec<u8>, usize)>, SocketAddr)>,
    #[allow(unused)]
    revoke: mpsc::Sender<SocketAddr>,
    #[allow(unused)]
    writer: mpsc::Sender<(Vec<u8>, SocketAddr)>,
}

impl UdpListener {
    #[allow(unused)]
    pub async fn bind(addr: SocketAddr) -> std::io::Result<Self> {
        match UdpSocket::bind(addr).await {
            Ok(socket) => {
                let (mut receiver, mut sender) = socket.split();

                let (mut receiver_tx, receiver_rx): (
                    mpsc::Sender<(mpsc::Receiver<(Vec<u8>, usize)>, SocketAddr)>,
                    mpsc::Receiver<(mpsc::Receiver<(Vec<u8>, usize)>, SocketAddr)>,
                ) = mpsc::channel(CHANNEL_LEN);

                let (revoke_tx, mut _revoke_rx): (
                    mpsc::Sender<SocketAddr>,
                    mpsc::Receiver<SocketAddr>,
                ) = mpsc::channel(CHANNEL_LEN);

                let (writer_tx, mut writer_rx): (
                    mpsc::Sender<(Vec<u8>, SocketAddr)>,
                    mpsc::Receiver<(Vec<u8>, SocketAddr)>,
                ) = mpsc::channel(CHANNEL_LEN);

                tokio::spawn(async move {
                    let mut streams: HashMap<SocketAddr, mpsc::Sender<(Vec<u8>, usize)>> =
                        HashMap::new();
                    let mut buf = vec![0u8; UDP_BUFFER_SIZE];
                    loop {
                        let (len, addr) = receiver.recv_from(&mut buf).await.unwrap();
                        match streams.get_mut(&addr) {
                            Some(child_tx) => {
                                child_tx.send((buf.clone(), len)).await.unwrap();
                            }
                            None => {
                                let (mut child_tx, child_rx): (
                                    mpsc::Sender<(Vec<u8>, usize)>,
                                    mpsc::Receiver<(Vec<u8>, usize)>,
                                ) = mpsc::channel(CHANNEL_LEN);

                                streams.insert(addr, child_tx.clone());
                                receiver_tx.send((child_rx, addr)).await.unwrap();
                                child_tx.send((buf.clone(), len)).await.unwrap();
                            }
                        };
                    }
                });

                tokio::spawn(async move {
                    while let Some(data) = writer_rx.recv().await {
                        sender.send_to(&data.0, &data.1).await.unwrap();
                    }
                });

                Ok(Self {
                    local_addr: addr,
                    receiver: receiver_rx,
                    revoke: revoke_tx,
                    writer: writer_tx,
                })
            }
            Err(_e) => unimplemented!(),
        }
    }
    #[allow(unused)]
    pub async fn accept(&mut self) -> std::io::Result<(UdpStream, SocketAddr)> {
        let (receiver, addr) = self.receiver.recv().await.unwrap();
        Ok((
            UdpStream {
                local_addr: self.local_addr,
                peer_addr: addr,
                receiver: Arc::new(Mutex::new(receiver)),
                writer: self.writer.clone(),
            },
            addr,
        ))
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
pub struct UdpStream {
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
    receiver: Arc<Mutex<mpsc::Receiver<(Vec<u8>, usize)>>>,
    writer: mpsc::Sender<(Vec<u8>, SocketAddr)>,
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
            "0.0.0.0:0"
        } else {
            "[::]:0"
        }
        .parse()
        .unwrap();

        let socket = UdpSocket::bind(local_addr).await?;
        let local_addr = socket.local_addr().unwrap();
        socket.connect(&addr);
        let (mut receiver, mut sender) = socket.split();

        let (mut receiver_tx, receiver_rx): (
            mpsc::Sender<(Vec<u8>, usize)>,
            mpsc::Receiver<(Vec<u8>, usize)>,
        ) = mpsc::channel(CHANNEL_LEN);

        let (_revoke_tx, mut _revoke_rx): (mpsc::Sender<SocketAddr>, mpsc::Receiver<SocketAddr>) =
            mpsc::channel(CHANNEL_LEN);

        let (writer_tx, mut writer_rx): (
            mpsc::Sender<(Vec<u8>, SocketAddr)>,
            mpsc::Receiver<(Vec<u8>, SocketAddr)>,
        ) = mpsc::channel(CHANNEL_LEN);

        tokio::spawn(async move {
            let mut buf = vec![0u8; UDP_BUFFER_SIZE];
            loop {
                let (len, addr) = receiver.recv_from(&mut buf).await.unwrap();
                receiver_tx.send((buf.clone(), len)).await.unwrap();
            }
        });

        tokio::spawn(async move {
            while let Some(data) = writer_rx.recv().await {
                sender.send_to(&data.0, &data.1).await.unwrap();
            }
        });

        Ok(Self {
            local_addr: local_addr,
            peer_addr: addr,
            receiver: Arc::new(Mutex::new(receiver_rx)),
            writer: writer_tx,
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
    pub fn shutdown(&self, _how: std::net::Shutdown) {} // should be implemented
}

impl AsyncRead for UdpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut socket = (&self.receiver).lock().unwrap();
        let future = socket.recv();
        pin_mut!(future);
        future.poll(cx).map(|res| {
            let (inner_buf, len) = res.unwrap();
            buf.clone_from_slice(&inner_buf[..buf.len()]);
            Ok(len) // fake len !
        })
    }
}

impl AsyncWrite for UdpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut writer = self.writer.clone();
        let future = writer.send((buf.to_vec(), self.peer_addr));
        pin_mut!(future);
        future.poll(cx).map(|_| Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
