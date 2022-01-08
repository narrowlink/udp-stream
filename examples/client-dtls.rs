use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use std::pin::Pin;
use std::{io, net::SocketAddr, str::FromStr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use udp_stream::UdpStream;

const SERVER_DOMAIN: &'static str = "pourali.com";

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = UdpStream::connect(SocketAddr::from_str("127.0.0.1:8080").unwrap()).await?;

    let mut connector_builder = SslConnector::builder(SslMethod::dtls())?;
    connector_builder.set_verify(SslVerifyMode::NONE);
    let connector = connector_builder.build().configure().unwrap();
    let ssl = connector.into_ssl(SERVER_DOMAIN).unwrap();
    let mut stream = tokio_openssl::SslStream::new(ssl, stream).unwrap();
    Pin::new(&mut stream).connect().await.unwrap();
    let mut buffer = String::new();
    loop {
        io::stdin().read_line(&mut buffer)?;
        stream.write_all(buffer.as_bytes()).await?;
        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await?;
        print!("-> {}", String::from_utf8_lossy(&buf[..n]));
    }
}
