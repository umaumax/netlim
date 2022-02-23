use tokio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const BUFFER_SIZE: usize = 32 * 1024;

// TODO: add run flag to determine client port and server address
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:11111").await?;

    // TODO: update logs
    eprintln!("start server {:?}", listener);
    loop {
        let (mut client_stream, _) = listener.accept().await?;

        tokio::spawn(async move {
            eprintln!("connect from xxx");
            let server_addr = "localhost:22222";
            let mut server_stream = TcpStream::connect(server_addr.clone()).await.unwrap();
            eprintln!("connect to {}", server_addr);

            let mut outbound_buf = [0; BUFFER_SIZE];
            let mut inbound_buf = [0; BUFFER_SIZE];
            // TODO: add close step
            loop {
                tokio::select! {
                    // send(outbound)
                    n = client_stream.read(&mut outbound_buf) =>{
                        let n = match n{
                            Ok(n) if n == 0 => return,
                            Ok(n) => n,
                            Err(e) => {
                                eprintln!("failed to read from stream; err = {:?}", e);
                                return;
                            }
                        };
                        eprintln!("-receive-> {} bytes", n);

                        match server_stream.write_all(&outbound_buf[0..n]).await {
                            Ok(_n) => {}
                            Err(e) => {
                                eprintln!("failed to write to stream; err = {:?}", e);
                                return;
                            }
                        }
                        eprintln!("-send-> {} bytes", n);
                    }
                    // receive(inbound)
                    n = server_stream.read(&mut inbound_buf) =>{
                        let n=match n {
                            Ok(n) if n == 0 => return,
                            Ok(n) => n,
                            Err(e) => {
                                eprintln!("failed to read from stream; err = {:?}", e);
                                return;
                            }
                        };
                        eprintln!("<-receive- {} bytes", n);

                        match client_stream.write_all(&inbound_buf[0..n]).await {
                            Ok(_n) => {}
                            Err(e) => {
                                eprintln!("failed to write to stream; err = {:?}", e);
                                return;
                            }
                        }
                        eprintln!("<-send- {} bytes", n);
                    }
                };
            }
        });
    }
}
