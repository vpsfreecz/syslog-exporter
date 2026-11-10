use clap::Parser;
use prometheus::Registry;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use syslog_exporter::{collectors::Engine, config::Config, http, input};

#[derive(Parser)]
#[command(
    version,
    about = "Generate Prometheus metrics from rsyslog FIFO messages"
)]
struct Args {
    /// JSON configuration file
    #[arg(
        long,
        env = "SYSLOG_EXPORTER_CONFIG",
        default_value = "config-sample.json"
    )]
    config: PathBuf,
    /// HTTP bind address
    #[arg(long, default_value = "0.0.0.0:9102")]
    listen: SocketAddr,
    /// Validate configuration without opening the FIFO or HTTP listener
    #[arg(long)]
    check_config: bool,
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let config = Config::load(&args.config)?;
    if args.check_config {
        println!("configuration is valid");
        return Ok(());
    }
    let registry = Registry::new();
    let engine = Engine::new(&config, &registry)?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = stop.clone();
    let (finished, mut receive) = tokio::sync::oneshot::channel();
    let reader = std::thread::spawn(move || {
        let result = input::run(
            &config.syslog_pipe,
            config.pipe_size,
            config.max_line_bytes,
            engine,
            reader_stop,
        );
        let _ = finished.send(result);
    });
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let result = tokio::select! {
        outcome = &mut receive => outcome.map_err(std::io::Error::other)?,
        result = http::serve(listener, registry, stop.clone()) => result,
        result = tokio::signal::ctrl_c() => result,
        _ = term.recv() => Ok(()),
    };
    stop.store(true, Ordering::Relaxed);
    reader
        .join()
        .map_err(|_| std::io::Error::other("FIFO reader panicked"))?;
    result?;
    Ok(())
}
