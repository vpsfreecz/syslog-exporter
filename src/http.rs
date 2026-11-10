use axum::{
    Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use prometheus::Registry;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinSet};
use tower_http::compression::CompressionLayer;

#[derive(Clone)]
struct HttpState {
    registry: Registry,
    encoding: Arc<Semaphore>,
    admission: Arc<Semaphore>,
}
impl HttpState {
    fn new(registry: Registry) -> Self {
        Self {
            registry,
            encoding: Arc::new(Semaphore::new(1)),
            admission: Arc::new(Semaphore::new(2)),
        }
    }
}
async fn metrics(State(state): State<HttpState>) -> impl IntoResponse {
    let Ok(admission) = state.admission.try_acquire_owned() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "scrape capacity reached").into_response();
    };
    let Ok(Ok(encoding)) =
        tokio::time::timeout(Duration::from_secs(1), state.encoding.acquire_owned()).await
    else {
        return (StatusCode::SERVICE_UNAVAILABLE, "scrape wait timed out").into_response();
    };
    match tokio::task::spawn_blocking(move || {
        // Client cancellation must not release either limit while this work continues.
        let (_admission, _encoding) = (admission, encoding);
        crate::metrics::encode(&state.registry)
    })
    .await
    {
        Ok(Ok(body)) => {
            ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body).into_response()
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "unable to encode metrics",
        )
            .into_response(),
    }
}
pub fn router(registry: Registry) -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .fallback(|| async { ([(header::CONTENT_TYPE, "text/html")], "OK") })
        .layer(CompressionLayer::new())
        .with_state(HttpState::new(registry))
}
/// Bound connections and their complete lifetime independently of ingestion.
pub async fn serve(
    listener: TcpListener,
    registry: Registry,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    let app = router(registry);
    let connections = Arc::new(Semaphore::new(32));
    let mut tasks = JoinSet::new();
    while !stop.load(Ordering::Relaxed) {
        tokio::select! {
            result = listener.accept() => {
                let (socket, _) = result?;
                let Ok(permit) = connections.clone().try_acquire_owned() else { continue; };
                let app = app.clone();
                tasks.spawn(async move {
                    let _permit = permit;
                    let builder = Builder::new(TokioExecutor::new());
                    let connection = builder.serve_connection_with_upgrades(TokioIo::new(socket), TowerToHyperService::new(app));
                    // Covers incomplete headers, slow body writes and keep-alive idle time.
                    let _ = tokio::time::timeout(Duration::from_secs(30), connection).await;
                });
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {},
            _ = tasks.join_next(), if !tasks.is_empty() => {},
        }
    }
    // Responses own their encoded bytes; shutdown does not wait for a slow client.
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, response::Response};
    use prometheus::{
        IntGauge,
        core::{Collector, Desc},
        proto::MetricFamily,
    };
    use std::sync::{Mutex, atomic::AtomicUsize};
    use tokio::sync::{Notify, oneshot};

    struct BlockOnce {
        gauge: IntGauge,
        started: Arc<Notify>,
        release: Mutex<Option<oneshot::Receiver<()>>>,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }
    impl Collector for BlockOnce {
        fn desc(&self) -> Vec<&Desc> {
            self.gauge.desc()
        }
        fn collect(&self) -> Vec<MetricFamily> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            let release = self.release.lock().unwrap().take();
            if let Some(release) = release {
                self.started.notify_one();
                let _ = release.blocking_recv();
            }
            let families = self.gauge.collect();
            self.active.fetch_sub(1, Ordering::SeqCst);
            families
        }
    }
    struct Gate {
        state: HttpState,
        started: Arc<Notify>,
        release: oneshot::Sender<()>,
        active: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }
    fn blocked_encoder() -> Gate {
        let registry = Registry::new();
        let started = Arc::new(Notify::new());
        let (release, receive) = oneshot::channel();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        registry
            .register(Box::new(BlockOnce {
                gauge: IntGauge::new("test_http_encoder", "Test encode gate").unwrap(),
                started: started.clone(),
                release: Mutex::new(Some(receive)),
                active: active.clone(),
                peak: peak.clone(),
            }))
            .unwrap();
        Gate {
            state: HttpState::new(registry),
            started,
            release,
            active,
            peak,
        }
    }
    async fn request(state: HttpState) -> Response {
        metrics(State(state)).await.into_response()
    }
    async fn admitted(state: &HttpState, available: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while state.admission.available_permits() != available {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    async fn valid(response: Response) {
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/plain; version=0.0.4"
        );
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .contains("test_http_encoder 0")
        );
    }
    #[tokio::test]
    async fn two_overlaps_serialize_and_third_is_rejected() {
        let gate = blocked_encoder();
        let first = tokio::spawn(request(gate.state.clone()));
        tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
            .await
            .unwrap();
        let second = tokio::spawn(request(gate.state.clone()));
        admitted(&gate.state, 0).await;
        assert_eq!(
            request(gate.state.clone()).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(gate.active.load(Ordering::SeqCst), 1);
        gate.release.send(()).unwrap();
        // Leave the first response body unread while the second encode completes.
        let first = first.await.unwrap();
        let second = second.await.unwrap();
        assert_eq!(gate.state.admission.available_permits(), 2);
        assert_eq!(gate.state.encoding.available_permits(), 1);
        assert_eq!(gate.peak.load(Ordering::SeqCst), 1);
        valid(first).await;
        valid(second).await;
    }
    #[tokio::test]
    async fn waiting_encoder_times_out_and_recovers() {
        let gate = blocked_encoder();
        let first = tokio::spawn(request(gate.state.clone()));
        tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
            .await
            .unwrap();
        let started = tokio::time::Instant::now();
        let response =
            tokio::time::timeout(Duration::from_millis(1300), request(gate.state.clone()))
                .await
                .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(started.elapsed() >= Duration::from_secs(1));
        assert_eq!(gate.state.admission.available_permits(), 1);
        gate.release.send(()).unwrap();
        valid(first.await.unwrap()).await;
        valid(request(gate.state.clone()).await).await;
        assert_eq!(gate.state.admission.available_permits(), 2);
        assert_eq!(gate.peak.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn cancelling_a_waiter_returns_its_admission() {
        let gate = blocked_encoder();
        let first = tokio::spawn(request(gate.state.clone()));
        tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
            .await
            .unwrap();
        let waiter = tokio::spawn(request(gate.state.clone()));
        admitted(&gate.state, 0).await;
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        assert_eq!(gate.state.admission.available_permits(), 1);
        gate.release.send(()).unwrap();
        valid(first.await.unwrap()).await;
        valid(request(gate.state.clone()).await).await;
        assert_eq!(gate.state.admission.available_permits(), 2);
    }
    #[tokio::test]
    async fn cancelling_a_running_encode_retains_both_permits() {
        let gate = blocked_encoder();
        let first = tokio::spawn(request(gate.state.clone()));
        tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
            .await
            .unwrap();
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        assert_eq!(gate.state.admission.available_permits(), 1);
        assert_eq!(gate.state.encoding.available_permits(), 0);
        let waiter = tokio::spawn(request(gate.state.clone()));
        admitted(&gate.state, 0).await;
        assert_eq!(
            request(gate.state.clone()).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        gate.release.send(()).unwrap();
        valid(waiter.await.unwrap()).await;
        assert_eq!(gate.state.admission.available_permits(), 2);
        assert_eq!(gate.state.encoding.available_permits(), 1);
        assert_eq!(gate.peak.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn dropping_the_test_gate_unblocks_failure_cleanup() {
        let gate = blocked_encoder();
        let first = tokio::spawn(request(gate.state.clone()));
        tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
            .await
            .unwrap();
        // Assertion unwind drops this same sole sender; no blocked worker is stranded.
        drop(gate.release);
        valid(
            tokio::time::timeout(Duration::from_secs(1), first)
                .await
                .unwrap()
                .unwrap(),
        )
        .await;
        assert_eq!(gate.state.admission.available_permits(), 2);
        assert_eq!(gate.state.encoding.available_permits(), 1);
    }
}
