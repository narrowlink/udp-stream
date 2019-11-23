use openssl::{
    pkey::PKey,
    ssl::{SslAcceptor, SslMethod},
    x509::X509,
};

use std::{
    net::SocketAddr,
    str::FromStr,
    time::Duration,
};
#[allow(unused)]
use tokio::{
    future::{Future, FutureExt},
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UdpSocket,
    sync::mpsc,
};

use udpstream::UdpListener;

const UDP_BUFFER_SIZE: usize = 17480; // 17kb
const UDP_TIMEOUT: u64 = 10 * 1000; // 10sec

static SERVER_CERT: &'static [u8] = include_bytes!("server-cert.pem");
static SERVER_KEY: &'static [u8] = include_bytes!("server-key.pem");

fn ssl_acceptor(certificate: &[u8], private_key: &[u8]) -> std::io::Result<SslAcceptor> {
    let mut acceptor_builder = SslAcceptor::mozilla_intermediate(SslMethod::dtls())?;
    acceptor_builder.set_certificate(&&X509::from_pem(certificate)?)?;
    acceptor_builder.set_private_key(&&PKey::private_key_from_pem(private_key)?)?;
    acceptor_builder.check_private_key()?;
    let acceptor = acceptor_builder.build();
    Ok(acceptor)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut listener = UdpListener::bind(SocketAddr::from_str("127.0.0.1:8080").unwrap()).await?;
    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            let acceptor = ssl_acceptor(SERVER_CERT, SERVER_KEY).unwrap();
            let stream = tokio_openssl::accept(&acceptor, socket).await;
            match stream {
                Ok(mut socket) => {
                    let mut buf = vec![0u8; UDP_BUFFER_SIZE];
                    loop {
                        // let n = socket.read(&mut buf).await.unwrap();

                        let n = match socket
                            .read(&mut buf)
                            .timeout(Duration::from_millis(UDP_TIMEOUT))
                            .await
                            .unwrap()
                        {
                            Ok(len) => len,
                            Err(_) => {
                                socket.get_ref().shutdown(std::net::Shutdown::Both);
                                return;
                            }
                        };

                        socket.write_all(&buf[0..n]).await.unwrap();
                    }
                }
                Err(_) => unimplemented!(),
            }
        });
    }
}
