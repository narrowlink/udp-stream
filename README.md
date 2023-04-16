
# udp-stream

[![crates.io](https://img.shields.io/crates/v/udp-stream.svg)](https://crates.io/crates/udp-stream)


`udp-stream` is a Rust library that provides a simple API for handling streaming data over the User Datagram Protocol (UDP), similar to TcpStream. It abstracts the complexities of working with UDP, such as handling packet fragmentation, reassembly, and flow control, making it easy for developers to send and receive continuous streams of data over UDP sockets especially when using DTLS protocol.

## Features

-   **Stream-based**: `udp-stream` provides an abstraction layer for handling UDP packets as a continuous stream of data, using a similar function signature as `TcpStream` in the `tokio` library. This allows developers familiar with `tokio` to leverage their existing knowledge to work with UDP in a similar manner.
    
-   **Lightweight**: `udp-stream` has a small footprint and only depends on the `tokio` library, making it lightweight and easy to integrate into your existing projects.
    
## Usage

To use `udp-stream` in your Rust project, simply add it as a dependency in your `Cargo.toml` file:

toml

`[dependencies]
udp-stream = "0.0.7"` 

Then, you can import and use the library in your Rust code:

rust

```
use std::{net::SocketAddr, str::FromStr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use udp_stream::UdpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut stream = UdpStream::connect(SocketAddr::from_str("127.0.0.1:8080")?).await?;
    println!("Ready to Connected to {}", &stream.peer_addr()?);
    let mut buffer = String::new();
    loop {
        std::io::stdin().read_line(&mut buffer)?;
        stream.write_all(buffer.as_bytes()).await?;
        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await?;
        print!("-> {}", String::from_utf8_lossy(&buf[..n]));
    }
}
```

For more details on how to use `udp-stream`, including configuration options, using for DTLS, and advanced usage, please refer to the [examples](https://github.com/SajjadPourali/udp-stream/tree/master/examples).

## Contributing

Contributions to `udp-stream` are welcome! If you would like to contribute to the library, please follow the standard Rust community guidelines for contributing, including opening issues, submitting pull requests, and providing feedback.

## License

`udp-stream` is licensed under the [MIT License](https://github.com/SajjadPourali/udp-stream/blob/master/LICENSE), which allows for free use, modification, and distribution, subject to the terms and conditions outlined in the license.

We hope that `udp-stream` is useful for your projects! If you have any questions or need further assistance, please don't hesitate to contact us or open an issue in the repository.
