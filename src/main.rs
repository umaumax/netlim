use std::cmp;
use std::ops::Sub;

use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use structopt::StructOpt;

#[derive(StructOpt)]
struct Cli {
    #[structopt(long = "src", default_value("0.0.0.0:11111"), help = "src socket")]
    src_socket: String,
    #[structopt(long = "dst", default_value("localhost:22222"), help = "dst socket")]
    dst_socket: String,

    #[structopt(
        long = "in",
        default_value("1024"),
        help = "inbound bandwidth limit [B]"
    )]
    inbound_bandwidth_limit: f64,
    #[structopt(
        long = "out",
        default_value("1024"),
        help = "outbound bandwidth limit [B]"
    )]
    outbound_bandwidth_limit: f64,
}

// TODO: add run flag to determine client port and server address
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::from_args();

    let listener = TcpListener::bind(args.src_socket).await?;

    let interval_secs = 0.1;
    let interval_micros = (interval_secs * 1000.0 * 1000.0) as u64;
    let outbound_buffer_size = (args.outbound_bandwidth_limit as f64 * interval_secs) as usize;
    let inbound_buffer_size = (args.inbound_bandwidth_limit as f64 * interval_secs) as usize;
    // TODO: update logs
    eprintln!("start server {:?}", listener);

    loop {
        let (mut client_stream, _) = listener.accept().await?;

        let dst_socket = args.dst_socket.clone();
        tokio::spawn(async move {
            eprintln!("connect from xxx");
            let mut server_stream = TcpStream::connect(dst_socket.clone()).await.unwrap();
            eprintln!("connect to {}", dst_socket.clone());

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
