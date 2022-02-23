use std::cmp;
use std::ops::Sub;

use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// TODO: add run flag to determine client port and server address
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:11111").await?;

    let interval_secs = 0.1;
    let interval_micros = (interval_secs * 1000.0 * 1000.0) as u64;
    let outbound_bandwidth_limit = 10 * 1024;
    let inbound_bandwidth_limit = 10 * 1024;
    let outbound_buffer_size = (outbound_bandwidth_limit as f64 * interval_secs) as usize;
    let inbound_buffer_size = (inbound_bandwidth_limit as f64 * interval_secs) as usize;
    // TODO: update logs
    eprintln!("start server {:?}", listener);

    loop {
        let (mut client_stream, _) = listener.accept().await?;

        tokio::spawn(async move {
            eprintln!("connect from xxx");
            let server_addr = "localhost:22222";
            let mut server_stream = TcpStream::connect(server_addr.clone()).await.unwrap();
            eprintln!("connect to {}", server_addr);

            let mut outbound_buf = vec![0; outbound_buffer_size];
            let mut inbound_buf = vec![0; inbound_buffer_size];
            let mut outbound_capacity = outbound_buffer_size;
            let mut inbound_capacity = inbound_buffer_size;
            let mut pre_timestamp = Instant::now();
            // TODO: add close step
            loop {
                let timeout_sleep =
                    tokio::time::sleep(tokio::time::Duration::from_micros(interval_micros));
                tokio::pin!(timeout_sleep);

                tokio::select! {
                    // send(outbound)
                    n = client_stream.read(&mut outbound_buf[0..outbound_capacity]), if outbound_capacity > 0 => {
                        let n = match n {
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

                        outbound_capacity -= n;
                        outbound_capacity = cmp::max(outbound_capacity, 0);

                        eprintln!("-send-> {} bytes", n);
                    }
                    // receive(inbound)
                    n = server_stream.read(&mut inbound_buf[0..inbound_capacity]), if inbound_capacity > 0 => {
                        let n = match n {
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
                        inbound_capacity -= n;
                        inbound_capacity = cmp::max(inbound_capacity, 0);

                        eprintln!("<-send- {} bytes", n);
                    }
                    _ = &mut timeout_sleep => {
                        eprintln!("timeout");
                    }
                };
                let duration = pre_timestamp.elapsed().as_micros() as u64;
                if duration >= interval_micros {
                    // reset capacity limit
                    eprintln!("reset capacity limit");
                    let now = Instant::now();
                    pre_timestamp = now.sub(Duration::from_micros(duration % interval_micros));
                    outbound_capacity = outbound_buffer_size;
                    inbound_capacity = inbound_buffer_size;
                }
            }
        });
    }
}
