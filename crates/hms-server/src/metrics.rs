//! Minimal Prometheus metrics (README §17 monitoring).
//!
//! Exposes `GET /metrics` in the Prometheus text exposition format:
//! request count, server-error count, a latency histogram and database pool
//! gauges. Deliberately dependency-free (plain atomics) so the hot path stays
//! cheap. Scrape it from Prometheus/Grafana, or restrict it at the reverse
//! proxy — it has no authentication because scrapers normally use a private
//! network.

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use crate::AppState;

/// Upper bounds in milliseconds (exclusive of `+Inf`).
const BUCKET_MS: [u64; 11] = [5, 10, 25, 50, 100, 200, 500, 1_000, 2_000, 5_000, 10_000];

static REQUESTS: AtomicU64 = AtomicU64::new(0);
static SERVER_ERRORS: AtomicU64 = AtomicU64::new(0);
/// Cumulative counts per bucket (index-aligned with [`BUCKET_MS`]).
static LATENCY: [AtomicU64; 11] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
static LATENCY_SUM_MICROS: AtomicU64 = AtomicU64::new(0);
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Middleware: time each request and update the counters.
pub async fn track(req: Request, next: Next) -> Response {
    STARTED.get_or_init(Instant::now);
    let started = Instant::now();
    let response = next.run(req).await;
    let elapsed_micros = started.elapsed().as_micros() as u64;
    let elapsed_ms = elapsed_micros / 1_000;

    REQUESTS.fetch_add(1, Ordering::Relaxed);
    LATENCY_SUM_MICROS.fetch_add(elapsed_micros, Ordering::Relaxed);
    // Cumulative histogram: a sample increments ONLY the first bucket whose
    // bound it fits (le is "less than or equal"), and the exposition sums
    // forward. Incrementing every larger bucket would make le="0.005" claim
    // almost all traffic and render p95 alerts useless.
    if let Some(i) = BUCKET_MS.iter().position(|upper| elapsed_ms <= *upper) {
        LATENCY[i].fetch_add(1, Ordering::Relaxed);
    }
    if response.status().is_server_error() {
        SERVER_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
    response
}

/// `GET /metrics` — Prometheus text exposition.
pub async fn endpoint(State(state): State<AppState>) -> impl IntoResponse {
    let requests = REQUESTS.load(Ordering::Relaxed);
    let errors = SERVER_ERRORS.load(Ordering::Relaxed);
    let sum_seconds = LATENCY_SUM_MICROS.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let uptime = STARTED.get().map(|s| s.elapsed().as_secs()).unwrap_or(0);
    let pool_size = state.db.size();
    let pool_idle = state.db.num_idle();

    let change_seq: i64 = sqlx::query_scalar("SELECT COALESCE(max(change_seq), 0) FROM change_log")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let mut out = String::with_capacity(2_048);
    out.push_str("# HELP hms_up 1 when the process is serving requests\n# TYPE hms_up gauge\nhms_up 1\n");
    out.push_str(&format!(
        "# HELP hms_uptime_seconds Process uptime in seconds\n# TYPE hms_uptime_seconds gauge\nhms_uptime_seconds {uptime}\n"
    ));
    out.push_str(&format!(
        "# HELP hms_http_requests_total HTTP requests served\n# TYPE hms_http_requests_total counter\nhms_http_requests_total {requests}\n"
    ));
    out.push_str(&format!(
        "# HELP hms_http_server_errors_total HTTP responses with a 5xx status\n# TYPE hms_http_server_errors_total counter\nhms_http_server_errors_total {errors}\n"
    ));
    out.push_str(
        "# HELP hms_http_request_duration_seconds Request latency\n# TYPE hms_http_request_duration_seconds histogram\n",
    );
    for (i, upper) in BUCKET_MS.iter().enumerate() {
        // Prometheus cumulative buckets: le="x" counts samples <= x, so this
        // bucket is the sum of every smaller-or-equal bucket (indices 0..=i).
        let count: u64 = LATENCY[..=i].iter().map(|a| a.load(Ordering::Relaxed)).sum();
        out.push_str(&format!(
            "hms_http_request_duration_seconds_bucket{{le=\"{:.3}\"}} {count}\n",
            *upper as f64 / 1_000.0
        ));
    }
    // +Inf always equals the total number of samples, including those slower
    // than the largest finite bound.
    out.push_str(&format!(
        "hms_http_request_duration_seconds_bucket{{le=\"+Inf\"}} {requests}\n\
         hms_http_request_duration_seconds_sum {sum_seconds}\n\
         hms_http_request_duration_seconds_count {requests}\n"
    ));
    out.push_str(&format!(
        "# HELP hms_db_pool_connections Database pool connections\n# TYPE hms_db_pool_connections gauge\nhms_db_pool_connections{{state=\"total\"}} {pool_size}\nhms_db_pool_connections{{state=\"idle\"}} {pool_idle}\n"
    ));
    out.push_str(&format!(
        "# HELP hms_change_log_latest_seq Highest change_log sequence (sync watermark)\n# TYPE hms_change_log_latest_seq gauge\nhms_change_log_latest_seq {change_seq}\n"
    ));

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        out,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_are_strictly_increasing() {
        for pair in BUCKET_MS.windows(2) {
            assert!(pair[0] < pair[1]);
        }
    }

    /// Regression (found while verifying E4 against a live scrape): a slow
    /// request must NOT be counted in the fast buckets. Mirrors `track`'s
    /// accounting and the exposition's cumulative sum.
    #[test]
    fn e4_slow_requests_are_not_counted_in_fast_buckets() {
        // isolated counters: 3 fast samples (2 ms) and 1 slow sample (300 ms)
        let mut raw = [0u64; 11];
        for elapsed_ms in [2u64, 2, 2, 300] {
            if let Some(i) = BUCKET_MS.iter().position(|upper| elapsed_ms <= *upper) {
                raw[i] += 1;
            }
        }
        // cumulative for bucket `i` = every sample that fits in bound i or an
        // earlier (smaller) one: raw[..=i], NOT raw[i..]
        let cumulative = |idx: usize| -> u64 { raw[..=idx].iter().sum() };

        // le=0.005 sees exactly the three 2 ms samples
        assert_eq!(cumulative(0), 3, "fast bucket must not absorb the slow sample");
        // BUCKET_MS[1..=5] are 10/25/50/100/200 ms — all below 300 ms, so the
        // slow sample is still excluded there
        for idx in 1..=5 {
            assert_eq!(
                cumulative(idx),
                3,
                "BUCKET_MS[{idx}] = {} ms must exclude the 300 ms sample",
                BUCKET_MS[idx]
            );
        }
        // BUCKET_MS[6] = 500 ms is the first bound that includes it
        assert_eq!(cumulative(6), 4);
        assert_eq!(cumulative(10), 4, "every finite bound up to 10 s includes it");
        // the sequence is non-decreasing, as Prometheus requires
        assert!((0..11).map(cumulative).collect::<Vec<_>>().windows(2).all(|w| w[0] <= w[1]));
    }
}
