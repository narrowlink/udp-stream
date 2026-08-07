use std::{net::SocketAddr, pin::Pin, str::FromStr, time::Duration};
use udp_stream::UdpListener;

use openssl::{
    pkey::PKey,
    ssl::{Ssl, SslAcceptor, SslContext, SslMethod},
    x509::X509,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};

const UDP_BUFFER_SIZE: usize = 17480; // 17kb
const UDP_TIMEOUT: u64 = 10 * 1000; // 10sec

static SERVER_CERT: &[u8] = include_bytes!("server-cert.pem");
static SERVER_KEY: &[u8] = include_bytes!("server-key.pem");

fn ssl_acceptor(certificate: &[u8], private_key: &[u8]) -> std::io::Result<SslContext> {
    let mut acceptor_builder = SslAcceptor::mozilla_intermediate(SslMethod::dtls())?;
    let certificate = X509::from_pem(certificate)?;
    let private_key = PKey::private_key_from_pem(private_key)?;
    acceptor_builder.set_certificate(&certificate)?;
    acceptor_builder.set_private_key(&private_key)?;
    acceptor_builder.check_private_key()?;
    let acceptor = acceptor_builder.build();
    Ok(acceptor.into_context())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = UdpListener::bind(SocketAddr::from_str("127.0.0.1:8080")?).await?;
    let acceptor = ssl_acceptor(SERVER_CERT, SERVER_KEY)?;
    loop {
        let (socket, _) = listener.accept().await?;
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            let ssl = Ssl::new(&acceptor).map_err(std::io::Error::other)?;
            let mut stream = tokio_openssl::SslStream::new(ssl, socket).map_err(std::io::Error::other)?;
            Pin::new(&mut stream).accept().await.map_err(std::io::Error::other)?;
            let mut buf = vec![0u8; UDP_BUFFER_SIZE];
            loop {
                let duration = Duration::from_millis(UDP_TIMEOUT);
                let n = match timeout(duration, stream.read(&mut buf)).await {
                    Ok(len) => len?,
                    Err(_) => {
                        stream.shutdown().await?;
                        break;
                    }
                };

                stream.write_all(&buf[0..n]).await?;
            }
            Ok::<(), std::io::Error>(())
        });
    }
}
