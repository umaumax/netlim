use std::env;
use std::sync::Arc;

use log::{debug, error, info, warn};
use structopt::StructOpt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[derive(StructOpt)]
struct Cli {
    #[structopt(long = "src", default_value("0.0.0.0:11111"), help = "src socket")]
    src_socket: String,
    #[structopt(long = "dst", default_value("localhost:22222"), help = "dst socket")]
    dst_socket: String,

    #[structopt(
        long = "share",
        default_value("true"),
        parse(try_from_str),
        help = "share bandwidth limit flag"
    )]
    share_bandwidth_limit: bool,

    #[structopt(
        long = "in",
        default_value("1MB"),
        parse(try_from_str = parse_size::parse_size),
        help = "inbound bandwidth limit [Byte]"
    )]
    inbound_bandwidth_limit: u64,
    #[structopt(
        long = "out",
        default_value("1MB"),
        parse(try_from_str = parse_size::parse_size),
        help = "outbound bandwidth limit [Byte]"
    )]
    outbound_bandwidth_limit: u64,

    #[structopt(long = "verbose", help = "verbose flag")]
    verbose: bool,
}

// TODO: add run flag to determine client port and server address
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::from_args();

    env::set_var("RUST_LOG", "info");
    if args.verbose {
        env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();

    let listener = TcpListener::bind(args.src_socket).await?;

    let interval_secs = 0.1;
    let outbound_buffer_size = (args.outbound_bandwidth_limit as f64 * interval_secs) as usize;
    let inbound_buffer_size = (args.inbound_bandwidth_limit as f64 * interval_secs) as usize;
    // TODO: update logs
    info!("start server {:?}", listener);

    let next_outbound_time = Arc::new(Mutex::new(tokio::time::Instant::now()));
    let next_inbound_time = Arc::new(Mutex::new(tokio::time::Instant::now()));

    loop {
        let (mut client_stream, _) = listener.accept().await?;

        let dst_socket = args.dst_socket.clone();

        let mut next_outbound_time = Arc::clone(&next_outbound_time);
        let mut next_inbound_time = Arc::clone(&next_inbound_time);
        tokio::spawn(async move {
            debug!("connect from xxx");
            let mut server_stream = TcpStream::connect(dst_socket.clone()).await.unwrap();
            debug!("connect to {}", dst_socket.clone());

            let mut outbound_buf = vec![0; outbound_buffer_size];
            let mut inbound_buf = vec![0; inbound_buffer_size];

            // TODO: add close step

            if !args.share_bandwidth_limit {
                next_outbound_time = Arc::new(Mutex::new(tokio::time::Instant::now()));
                next_inbound_time = Arc::new(Mutex::new(tokio::time::Instant::now()));
            }
            loop {
                let now = tokio::time::Instant::now();
                tokio::select! {
                    // send(outbound)
                    _ = tokio::time::sleep_until(*next_outbound_time.lock().await), if now < *next_outbound_time.lock().await => {
                    }
                    n = client_stream.read(&mut outbound_buf), if now >= *next_outbound_time.lock().await => {
                        let n = match n {
                            Ok(n) if n == 0 => return,
                            Ok(n) => n,
                            Err(e) => {
                                error!("failed to read from stream; err = {:?}", e);
                                return;
                            }
                        };
                        debug!("-receive-> {} bytes", n);

                        match server_stream.write_all(&outbound_buf[0..n]).await {
                            Ok(_n) => {}
                            Err(e) => {
                                error!("failed to write to stream; err = {:?}", e);
                                return;
                            }
                        }

                        debug!("-send-> {} bytes", n);

                        {
                            let mut next_outbound_time = next_outbound_time.lock().await;
                            *next_outbound_time = next_outbound_time.max(tokio::time::Instant::now()) + tokio::time::Duration::from_secs_f64(n as f64 / args.outbound_bandwidth_limit as f64);
                        }
                    }
                    // receive(inbound)
                    _ = tokio::time::sleep_until(*next_inbound_time.lock().await), if now < *next_inbound_time.lock().await => {
                    }
                    n = server_stream.read(&mut inbound_buf), if now >= *next_inbound_time.lock().await => {
                        let n = match n {
                            Ok(n) if n == 0 => return,
                            Ok(n) => n,
                            Err(e) => {
                                error!("failed to read from stream; err = {:?}", e);
                                return;
                            }
                        };
                        debug!("<-receive- {} bytes", n);

                        match client_stream.write_all(&inbound_buf[0..n]).await {
                            Ok(_n) => {}
                            Err(e) => {
                                error!("failed to write to stream; err = {:?}", e);
                                return;
                            }
                        }
                        debug!("<-send- {} bytes", n);

                        {
                            let mut next_inbound_time = next_inbound_time.lock().await;
                            *next_inbound_time = next_inbound_time.max(tokio::time::Instant::now()) + tokio::time::Duration::from_secs_f64(n as f64 / args.inbound_bandwidth_limit as f64);
                        }
                    }
                };
            }
        });
    }
}
