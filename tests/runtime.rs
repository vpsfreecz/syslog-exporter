#![cfg(target_os = "linux")]
use nix::{sys::stat::Mode, unistd::mkfifo};
use prometheus::Registry;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use syslog_exporter::{collectors::Engine, config::Config, http, input};
#[test]
fn fifo_no_writer_split_first_record_eof_reopen_and_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("syslog.pipe");
    mkfifo(&path, Mode::S_IRUSR | Mode::S_IWUSR).unwrap();
    let mut config: Config = serde_json::from_str(include_str!("fixtures/config.json")).unwrap();
    config.syslog_pipe = path.clone();
    let registry = Registry::new();
    let engine = Engine::new(&config, &registry).unwrap();
    let d = engine.diagnostics.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = stop.clone();
    let reader = thread::spawn(move || input::run(&path, -1, 128, engine, reader_stop));
    thread::sleep(Duration::from_millis(150)); // no writer is a supported startup state
    {
        let mut writer = OpenOptions::new()
            .write(true)
            .open(&config.syslog_pipe)
            .unwrap();
        writer.write_all(b"2026-11-10T12:00:00Z host-vps ").unwrap();
        writer
            .write_all(b"kernel[123] apache invoked oom-killer: flags\n")
            .unwrap();
        writer.write_all(b"invalid date body\n").unwrap();
        writer.write_all(&[b'x'; 200]).unwrap();
        writer.write_all(b"\n").unwrap();
        writer
            .write_all(b"2026-11-10T12:00:00Z host-vps nodectld Daemon crashed \xff\n")
            .unwrap();
        writer
            .write_all(b"2026-11-10T12:00:00Z host-vps x good\npartial")
            .unwrap();
    }
    let until = Instant::now() + Duration::from_secs(3);
    while d.rejected.with_label_values(&["incomplete"]).get() == 0 {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(10));
    }
    {
        let mut writer = OpenOptions::new()
            .write(true)
            .open(&config.syslog_pipe)
            .unwrap();
        writer
            .write_all(b"2026-11-10T12:00:00Z host-vps x reopened\n")
            .unwrap();
    }
    while d.records.get() < 6 {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(10));
    }
    stop.store(true, Ordering::Relaxed);
    reader.join().unwrap().unwrap();
    assert_eq!(d.records.get(), 6);
    assert_eq!(d.body_decoded.get(), 1);
    assert_eq!(d.rejected.with_label_values(&["utf8"]).get(), 0);
    assert_eq!(d.rejected.with_label_values(&["oversized"]).get(), 1);
    assert_eq!(d.rejected.with_label_values(&["timestamp"]).get(), 1);
    assert!(d.capacity.get() > 0);
    assert!(
        syslog_exporter::metrics::encode(&registry)
            .unwrap()
            .lines()
            .any(|l| l.starts_with("syslog_nodectld_crash{") && l.ends_with(" 1"))
    );
    assert!(
        syslog_exporter::metrics::encode(&registry)
            .unwrap()
            .lines()
            .any(|l| l.starts_with("syslog_kernel_oom_count{") && l.ends_with(" 1"))
    );
}
#[test]
fn non_fifo_is_fatal() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let config: Config = serde_json::from_str(include_str!("fixtures/config.json")).unwrap();
    let engine = Engine::new(&config, &Registry::new()).unwrap();
    assert!(
        input::run(
            file.path(),
            1_048_576,
            128,
            engine,
            Arc::new(AtomicBool::new(false))
        )
        .is_err()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_contract_slow_clients_do_not_hold_ingestion_and_stop_is_bounded() {
    let config: Config = serde_json::from_str(include_str!("fixtures/config.json")).unwrap();
    let registry = Registry::new();
    let mut engine = Engine::new(&config, &registry).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server = tokio::spawn(http::serve(listener, registry.clone(), stop.clone()));
    // Header-incomplete sockets occupy bounded HTTP connections, never the ingestion owner.
    let clients = (0..8)
        .map(|_| {
            let mut s = TcpStream::connect(address).unwrap();
            s.write_all(b"GET /metrics HTTP/1.1\r\nHost:").unwrap();
            s
        })
        .collect::<Vec<_>>();
    for _ in 0..1000 {
        engine.process(
            &syslog_exporter::parser::Message {
                host: "host-vps",
                program: "kernel",
                body: "apache invoked oom-killer: flags".into(),
            },
            Duration::ZERO,
        );
    }
    let response = tokio::task::spawn_blocking(move || {
        let mut s = TcpStream::connect(address).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        s.write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut b = String::new();
        s.read_to_string(&mut b).unwrap();
        b
    })
    .await
    .unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("text/plain; version=0.0.4"));
    assert!(response.contains("syslog_kernel_oom_count{"));
    assert!(response.contains(" 1000\n"));
    stop.store(true, Ordering::Relaxed);
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop(clients);
}
#[test]
fn cli_validation_has_no_fifo_or_http_side_effects() {
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_syslog-exporter"))
        .args([
            "--config",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/config.json"),
            "--check-config",
        ])
        .status()
        .unwrap();
    assert!(status.success());
}

#[tokio::test]
async fn captured_http_fallback_and_gzip_contract() {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, header},
    };
    use tower::ServiceExt;
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/http.json")).unwrap();
    let config: Config = serde_json::from_str(include_str!("fixtures/config.json")).unwrap();
    let registry = Registry::new();
    let _engine = Engine::new(&config, &registry).unwrap();
    let app = http::router(registry.clone());
    let fallback = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/other")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        u64::from(fallback.status().as_u16()),
        expected["fallback"]["status"].as_u64().unwrap()
    );
    assert_eq!(
        fallback.headers()[header::CONTENT_TYPE],
        expected["fallback"]["headers"]["content-type"]
            .as_str()
            .unwrap()
    );
    assert_eq!(
        to_bytes(fallback.into_body(), 1024).await.unwrap().as_ref(),
        expected["fallback"]["body"].as_str().unwrap().as_bytes()
    );
    let plain = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plain.status(), 200);
    assert_eq!(
        plain.headers()[header::CONTENT_TYPE],
        expected["metrics"]["content-type"].as_str().unwrap()
    );
    let plain = to_bytes(plain.into_body(), 1024 * 1024).await.unwrap();
    let gzip = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(gzip.status(), 200);
    for name in ["content-type", "content-encoding"] {
        assert_eq!(
            gzip.headers()[name],
            expected["gzip"][name].as_str().unwrap()
        );
    }
    assert!(
        gzip.headers()[header::VARY]
            .to_str()
            .unwrap()
            .eq_ignore_ascii_case(expected["gzip"]["vary"].as_str().unwrap())
    );
    let compressed = to_bytes(gzip.into_body(), 1024 * 1024).await.unwrap();
    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(compressed.as_ref())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, plain.as_ref());
    assert_eq!(
        decoded,
        syslog_exporter::metrics::encode(&registry)
            .unwrap()
            .as_bytes()
    );
}

#[test]
fn benchmark_low_rate_deadline_reaps_candidate() {
    use std::os::unix::process::CommandExt;
    let dir = tempfile::tempdir().unwrap();
    let receipt = dir.path().join("result.json");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let mut command = std::process::Command::new("python3");
    command
        .args([
            concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/benchmark.py"),
            "--config",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/config.json"),
            "--records",
            "2",
            "--rate",
            "0.001",
            "--timeout",
            "1",
            "--scrape-interval",
            "0",
            "--port",
        ])
        .arg(address.port().to_string())
        .arg("--output")
        .arg(&receipt)
        .args([
            "--",
            env!("CARGO_BIN_EXE_syslog-exporter"),
            "--config",
            "{config}",
            "--listen",
            "{listen}",
        ])
        .stdout(std::process::Stdio::null())
        .process_group(0);
    let started = Instant::now();
    let mut child = command.spawn().unwrap();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(5) {
            // A regression must not leave the driver or its owned candidate alive.
            nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(child.id() as i32),
                nix::sys::signal::Signal::SIGKILL,
            )
            .unwrap();
            child.wait().unwrap();
            panic!("benchmark pacing exceeded the finite deadline");
        }
        thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(1));
    let result: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
    assert_eq!(result["outcome"], "failed");
    assert!(result["error"].as_str().unwrap().contains("deadline"));
    assert_eq!(result["submitted_unique_sequences"], 1);
    assert_eq!(result["exit_status"], 0);
    assert_eq!(result["shutdown_forced"], false);
    assert!(TcpStream::connect(address).is_err());
}
