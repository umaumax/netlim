use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use log::{debug, error, info};
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
        long = "unshare",
        parse(from_flag = std::ops::Not::not),
        help = "unshare bandwidth limit flag"
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

pub struct NetBandwidthLimitter {
    src_socket: String,
    dst_socket: String,
    listener: Option<TcpListener>,
    outbound_bandwidth_limit: u64,
    inbound_bandwidth_limit: u64,
    next_outbound_time: Arc<Mutex<tokio::time::Instant>>,
    next_inbound_time: Arc<Mutex<tokio::time::Instant>>,
    share_bandwidth_limit: bool,
}

impl NetBandwidthLimitter {
    fn try_new(
        src_socket: impl Into<String>,
        dst_socket: impl Into<String>,
        outbound_bandwidth_limit: u64,
        inbound_bandwidth_limit: u64,
        share_bandwidth_limit: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let src_socket = src_socket.into();
        let dst_socket = dst_socket.into();
        Ok(Self {
            src_socket,
            dst_socket,
            listener: None,
            outbound_bandwidth_limit,
            inbound_bandwidth_limit,
            next_outbound_time: Arc::new(Mutex::new(tokio::time::Instant::now())),
            next_inbound_time: Arc::new(Mutex::new(tokio::time::Instant::now())),
            share_bandwidth_limit,
        })
    }

    async fn bind(&mut self) -> Result<(), std::io::Error> {
        self.listener = Some(TcpListener::bind(&self.src_socket).await?);
        Ok(())
    }

    async fn accept(&self) -> Result<(TcpStream, SocketAddr), std::io::Error> {
        self.listener.as_ref().unwrap().accept().await
    }

    async fn send_outbound(
        server_stream: &mut TcpStream,
        outbound_buf: &mut [u8],
        next_outbound_time: &Arc<Mutex<tokio::time::Instant>>,
        outbound_bandwidth_limit: u64,
        n: Result<usize, tokio::io::Error>,
    ) -> Result<usize, tokio::io::Error> {
        let n = match n {
            Ok(n) if n == 0 => return Ok(0),
            Ok(n) => n,
            Err(e) => {
                return Err(e);
            }
        };
        debug!("[outbound] -recv-> {} bytes", n);

        let _n = server_stream.write_all(&outbound_buf[0..n]).await?;
        debug!("[outbound] -send-> {} bytes", n);

        let sleep_duration =
            tokio::time::Duration::from_secs_f64(n as f64 / outbound_bandwidth_limit as f64);
        {
            let mut next_outbound_time = next_outbound_time.lock().await;
            *next_outbound_time =
                next_outbound_time.max(tokio::time::Instant::now()) + sleep_duration;
        }
        Ok(n)
    }

    async fn recv_inbound(
        client_stream: &mut TcpStream,
        inbound_buf: &mut [u8],
        next_inbound_time: &Arc<Mutex<tokio::time::Instant>>,
        inbound_bandwidth_limit: u64,
        n: Result<usize, tokio::io::Error>,
    ) -> Result<usize, tokio::io::Error> {
        let n = match n {
            Ok(n) if n == 0 => return Ok(0),
            Ok(n) => n,
            Err(e) => {
                return Err(e);
            }
        };
        debug!("[inbound] <-recv- {} bytes", n);

        let _n = client_stream.write_all(&inbound_buf[0..n]).await?;
        debug!("[inbound] <-send- {} bytes", n);

        let sleep_duration =
            tokio::time::Duration::from_secs_f64(n as f64 / inbound_bandwidth_limit as f64);
        {
            let mut next_inbound_time = next_inbound_time.lock().await;
            *next_inbound_time =
                next_inbound_time.max(tokio::time::Instant::now()) + sleep_duration;
        }
        Ok(n)
    }

    async fn spawn(
        &self,
        (mut client_stream, client_addr): (TcpStream, SocketAddr),
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut next_outbound_time = Arc::clone(&self.next_outbound_time);
        let mut next_inbound_time = Arc::clone(&self.next_inbound_time);

        let interval_secs = 0.1;
        let outbound_buffer_size = (self.outbound_bandwidth_limit as f64 * interval_secs) as usize;
        let inbound_buffer_size = (self.inbound_bandwidth_limit as f64 * interval_secs) as usize;
        let dst_socket = self.dst_socket.clone();
        let share_bandwidth_limit = self.share_bandwidth_limit;
        let outbound_bandwidth_limit = self.outbound_bandwidth_limit;
        let inbound_bandwidth_limit = self.inbound_bandwidth_limit;
        tokio::spawn(async move {
            info!("connected from {:?}", client_addr);
            let mut server_stream = match TcpStream::connect(&dst_socket).await {
                Ok(s) => s,
                Err(e) => {
                    error!("failed to connect to dst server; err = {:?}", e);
                    return;
                }
            };
            info!("connected to dst server at {}", &dst_socket);

            let mut outbound_buf = vec![0; outbound_buffer_size];
            let mut inbound_buf = vec![0; inbound_buffer_size];

            if !share_bandwidth_limit {
                next_outbound_time = Arc::new(Mutex::new(tokio::time::Instant::now()));
                next_inbound_time = Arc::new(Mutex::new(tokio::time::Instant::now()));
            }
            let ret: Result<(), std::io::Error> = loop {
                let now = tokio::time::Instant::now();
                tokio::select! {
                    // send(outbound)
                    _ = tokio::time::sleep_until(*next_outbound_time.lock().await), if now < *next_outbound_time.lock().await => {
                    }
                    n = client_stream.read(&mut outbound_buf), if now >= *next_outbound_time.lock().await => {
                        match NetBandwidthLimitter::send_outbound(&mut server_stream, &mut outbound_buf,&next_outbound_time,outbound_bandwidth_limit,n).await {
                            Ok(0) => break Ok(()),
                            Ok(_) => {},
                            Err(e) => {
                                break Err(e);
                            }
                        };
                    }
                    // receive(inbound)
                    _ = tokio::time::sleep_until(*next_inbound_time.lock().await), if now < *next_inbound_time.lock().await => {
                    }
                    n = server_stream.read(&mut inbound_buf), if now >= *next_inbound_time.lock().await => {
                        match NetBandwidthLimitter::recv_inbound(&mut client_stream, &mut inbound_buf, &next_inbound_time, inbound_bandwidth_limit, n).await {
                            Ok(0) => break Ok(()),
                            Ok(_) => {},
                            Err(e) => {
                                break Err(e);
                            }
                        };
                    }
                };
            };
            match ret {
                Ok(_) => {}
                Err(e) => {
                    error!("failed to read or write from stream; err = {:?}", e);
                }
            };
            info!("disconnected from dst server at {}", &dst_socket);
            info!("disconnect from {:?}", client_addr);
            // automatically called on drop, but explicitly calls the close process
            client_stream.shutdown().await.unwrap();
            server_stream.shutdown().await.unwrap();
        });
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::from_args();

    env::set_var("RUST_LOG", "info");
    if args.verbose {
        env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();

    let mut net_bandwidth_limitter = NetBandwidthLimitter::try_new(
        &args.src_socket,
        &args.dst_socket,
        args.outbound_bandwidth_limit,
        args.inbound_bandwidth_limit,
        args.share_bandwidth_limit,
    )?;

    net_bandwidth_limitter.bind().await?;
    info!("started server at {:?}", &args.src_socket);

    loop {
        let (client_stream, client_addr) = net_bandwidth_limitter.accept().await?;
        net_bandwidth_limitter
            .spawn((client_stream, client_addr))
            .await?;
    }
    // NOTE: unreachable here
}
