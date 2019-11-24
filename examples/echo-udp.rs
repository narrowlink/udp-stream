use std::{io, net::SocketAddr, str::FromStr, time::Duration};
#[allow(unused)]
use tokio::{
    future::{Future, FutureExt},
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UdpSocket,
    sync::mpsc,
};

use udp_stream::UdpListener;

const UDP_BUFFER_SIZE: usize = 17480; // 17kb
const UDP_TIMEOUT: u64 = 10 * 1000; // 10sec

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut listener = UdpListener::bind(SocketAddr::from_str("127.0.0.1:8080").unwrap()).await?;
    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = vec![0u8; UDP_BUFFER_SIZE];
            loop {
                // let n = stream.read(&mut buf).await.unwrap();

                let n = match stream
                    .read(&mut buf)
                    .timeout(Duration::from_millis(UDP_TIMEOUT))
                    .await
                    .unwrap()
                {
                    Ok(len) => len,
                    Err(_) => {
                        stream.shutdown(std::net::Shutdown::Both);
                        return;
                    }
                };

                stream.write_all(&buf[0..n]).await.unwrap();
            }
        });
    }
}
