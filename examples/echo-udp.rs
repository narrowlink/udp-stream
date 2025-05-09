use std::{error::Error, net::SocketAddr, str::FromStr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};

const UDP_BUFFER_SIZE: usize = 17480; // 17kb
const UDP_TIMEOUT: u64 = 10 * 1000; // 10sec

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let level = format!("{}={}", module_path!(), "trace");
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level)).init();

    let listener = udp_stream::UdpListener::bind(SocketAddr::from_str("127.0.0.1:8080")?).await?;
    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let id = std::thread::current().id();
            let block = async move {
                let mut buf = vec![0u8; UDP_BUFFER_SIZE];
                let duration = Duration::from_millis(UDP_TIMEOUT);
                loop {
                    let n = timeout(duration, stream.read(&mut buf)).await??;
                    stream.write_all(&buf[0..n]).await?;
                    log::trace!("{:?} echoed {:?} for {} bytes", id, stream.peer_addr(), n);
                }
                #[allow(unreachable_code)]
                Ok::<(), std::io::Error>(())
            };
            if let Err(e) = block.await {
                log::error!("error: {:?}", e);
            }
        });
    }
}
