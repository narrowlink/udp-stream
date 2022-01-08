
# udp-stream

[![crates.io](https://img.shields.io/crates/v/udp-stream.svg)](https://crates.io/crates/openssl)

Virtual UDP Stream implementation like Tokio TCP Stream based on Tokio.

### Notes
* It provides an easy interface to deal with DTLS and UDP sockets. Please check [Examples](https://github.com/SajjadPourali/udp-stream/tree/master/examples) folder. **Need help for documentation and test. PRs are welcome**
* Since UDP is a connection-less protocol, handling exceptions or sessions on this library is impossible. You should implement end connection roles methods; for example, we used a timeout to handle de-activated sessions in echo examples.

