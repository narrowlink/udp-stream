use std::{error::Error, net::SocketAddr, str::FromStr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use udp_stream::UdpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let level = format!("{}={}", module_path!(), "trace");
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level)).init();

    let mut stream = UdpStream::connect(SocketAddr::from_str("127.0.0.1:8080")?).await?;
    log::info!("Ready to Connected to {}", &stream.peer_addr()?);
    let mut buffer = String::new();
    loop {
        std::io::stdin().read_line(&mut buffer)?;
        stream.write_all(buffer.as_bytes()).await?;
        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await?;
        log::trace!("-> {}", String::from_utf8_lossy(&buf[..n]));
        buffer.clear();
    }
}
