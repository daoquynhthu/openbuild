//! Redirect mock server for P9-002 redirect policy tests.
//!
//! Starts an axum server that returns `n` sequential 302 redirects (self-referencing)
//! followed by a 200 OK with JSON body.  `n=0` returns 200 immediately.
//!
//! Also provides [`SlowServer`] for P9-003 bounded concurrency tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A mock server that counts redirects and returns a JSON response on the final hit.
#[derive(Debug)]
pub struct RedirectMockServer {
    addr: String,
    count: Arc<AtomicUsize>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl RedirectMockServer {
    /// Start a redirect loop: returns 302 `n` times, then 200.
    pub async fn start(n: usize) -> Self {
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = Arc::clone(&count);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        use axum::response::IntoResponse;

        let app = axum::Router::new().route(
            "/v1/models",
            axum::routing::get(move || {
                let cnt = Arc::clone(&count_clone);
                async move {
                    let hits = cnt.fetch_add(1, Ordering::SeqCst);
                    if hits < n {
                        // Return 302 redirect to self
                        axum::response::Response::builder()
                            .status(302)
                            .header("Location", "/v1/models")
                            .body(axum::body::Body::empty())
                            .unwrap()
                    } else {
                        // Return 200 with model list
                        axum::Json(serde_json::json!({"data": [{"id": "test-model"}]}))
                            .into_response()
                    }
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                })
                .await;
        });

        Self {
            addr: format!("http://{addr}"),
            count,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn url(&self) -> String {
        format!("{}/v1/models", self.addr)
    }

    pub fn request_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

impl Drop for RedirectMockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// A server that adds a fixed delay per request, used to observe concurrency limits.
/// Records the peak number of simultaneous in-flight requests and total request count.
#[derive(Debug)]
pub struct SlowServer {
    addr: String,
    in_flight: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    total: Arc<AtomicUsize>,
    delay: std::time::Duration,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl SlowServer {
    /// Start a slow server.  Each request takes `delay` to respond.
    pub async fn start(delay: std::time::Duration) -> Self {
        use axum::response::IntoResponse;

        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(0));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let app = {
            let in_flight = Arc::clone(&in_flight);
            let peak = Arc::clone(&peak);
            let total = Arc::clone(&total);
            let delay_clone = delay;

            axum::Router::new().route(
                "/v1/models",
                axum::routing::get(move || {
                    let in_flight = Arc::clone(&in_flight);
                    let peak = Arc::clone(&peak);
                    let total = Arc::clone(&total);
                    let d = delay_clone;
                    async move {
                        total.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let cur = in_flight.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                        peak.fetch_max(cur, std::sync::atomic::Ordering::SeqCst);
                        tokio::time::sleep(d).await;
                        in_flight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                        axum::Json(serde_json::json!({"data": [{"id": "m1"}]})).into_response()
                    }
                }),
            )
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                })
                .await;
        });

        Self {
            addr: format!("http://{addr}"),
            in_flight,
            peak,
            total,
            delay,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn url(&self) -> String {
        format!("{}/v1/models", self.addr)
    }

    /// Peak concurrent requests observed by the server.
    pub fn in_flight_peak(&self) -> usize {
        self.peak.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Total requests received.
    pub fn request_count(&self) -> usize {
        self.total.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Drop for SlowServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}
