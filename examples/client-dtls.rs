use std::{io, net::SocketAddr, str::FromStr};

use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use udp_stream::UdpStream;

const SERVER_DOMAIN: &'static str = "www.securation.com";

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = UdpStream::connect(SocketAddr::from_str("127.0.0.1:8080").unwrap()).await?;

    let mut connector_builder = SslConnector::builder(SslMethod::dtls())?;
    connector_builder.set_verify(SslVerifyMode::NONE);
    let connector = connector_builder.build().configure().unwrap();

    match tokio_openssl::connect(connector, SERVER_DOMAIN, stream).await {
        Ok(mut dtls_stream) => loop {
            let mut buffer = String::new();
            io::stdin().read_line(&mut buffer)?;
            dtls_stream.write_all(buffer.as_bytes()).await?;
            let mut buf = vec![0u8; 1024];
            let n = dtls_stream.read(&mut buf).await?;
            print!("-> {}", String::from_utf8_lossy(&buf[..n]));
        },

        Err(_) => unimplemented!(),
    };
}
