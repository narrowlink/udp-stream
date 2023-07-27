use std::{error::Error, net::SocketAddr, str::FromStr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};

use udp_stream::UdpListener;

const UDP_BUFFER_SIZE: usize = 17480; // 17kb
const UDP_TIMEOUT: u64 = 10 * 1000; // 10sec

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = UdpListener::bind(SocketAddr::from_str("127.0.0.1:8080")?).await?;
    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = vec![0u8; UDP_BUFFER_SIZE];
            loop {
                let n = match timeout(Duration::from_millis(UDP_TIMEOUT), stream.read(&mut buf))
                    .await
                    .unwrap()
                {
                    Ok(len) => len,
                    Err(_) => {
                        stream.shutdown();
                        return;
                    }
                };

                stream.write_all(&buf[0..n]).await.unwrap();
            }
        });
    }
}
