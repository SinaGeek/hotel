//! Standalone load runner for the README §13 performance targets:
//!   - room board / availability search / reservation save: p95 < 200 ms
//!   - full-year report: p95 < 500 ms
//!
//! No external tooling: plain HTTP against a running server. The k6 scripts
//! under `benchmarks/k6/` cover the same scenarios for CI users with k6.
//!
//! Usage:
//!   cargo run -p hms-tests -r --example loadtest -- \
//!       --base http://localhost:8080 --users 16 --n 200
//!   credentials come from the environment (LOADTEST_PASSWORD);
//!   defaults target the load-test hotel seeded by `seed/loadtest.sql`.
//!
//! Login goes through the real rate limiter (10 per 60 s), so this performs a
//! single login and reuses the token across all virtual users.
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use serde_json::json;

#[derive(Parser, Debug)]
#[command(name = "loadtest")]
struct Args {
    /// base URL of the running HMS server
    #[arg(long, default_value = "http://localhost:8080")]
    base: String,
    /// concurrent virtual users (real semaphore limit)
    #[arg(long, default_value = "16")]
    users: usize,
    /// requests per scenario (total, across all users)
    #[arg(long, default_value = "200")]
    n: usize,
    /// hotel to log into (default: the load-test hotel from seed/loadtest.sql)
    #[arg(long, default_value = "4443")]
    hotel: i64,
    #[arg(long, default_value = "loadmgr")]
    username: String,
    #[arg(long, env = "LOADTEST_PASSWORD", default_value = "")]
    password: String,
}

const TARGET_BOARD_MS: u64 = 200;
const TARGET_SAVE_MS: u64 = 200;
const TARGET_REPORT_MS: u64 = 500;

/// response-time tracker (microseconds)
struct Times(Vec<u128>);

impl Times {
    fn new() -> Self { Times(Vec::new()) }
    fn push(&mut self, t: u128) { self.0.push(t) }
    fn pct(&self, p: f64) -> f64 {
        let mut v = self.0.clone();
        v.sort_unstable();
        if v.is_empty() { return 0.0 }
        let idx = ((v.len() as f64 * p) as usize).clamp(0, v.len() - 1);
        v[idx] as f64 / 1000.0
    }
    fn count(&self) -> usize { self.0.len() }
    fn mean_ms(&self) -> f64 {
        if self.0.is_empty() { 0.0 } else { self.0.iter().sum::<u128>() as f64 / 1000.0 / self.0.len() as f64 }
    }
}

type OneReq = (u128, Result<(), String>);
type Fut = Pin<Box<dyn Future<Output = OneReq> + Send>>;

async fn bench(
    name: &str,
    target: u64,
    n: usize,
    users: usize,
    make: &(dyn Fn(usize) -> Fut + Send + Sync),
) -> (String, u64, Times, Duration, Vec<String>) {
    let sem = Arc::new(tokio::sync::Semaphore::new(users));
    let fails = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
    let mut times = Times::new();
    let t0 = Instant::now();
    let mut set = tokio::task::JoinSet::new();
    for i in 0..n {
        let permit = sem.clone().acquire_owned().await.unwrap();
        let fut = make(i);
        let fails = fails.clone();
        set.spawn(async move {
            let start = Instant::now();
            let (us, res) = fut.await;
            if let Err(e) = res {
                fails.lock().await.push(e);
            }
            drop(permit);
            let _ = start;
            us
        });
    }
    while let Some(h) = set.join_next().await {
        times.push(h.unwrap());
    }
    let wall = t0.elapsed();
    (name.to_string(), target, times, wall, Arc::try_unwrap(fails).unwrap().into_inner())
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if args.password.is_empty() {
        eprintln!("LOADTEST_PASSWORD env var not set - exiting");
        std::process::exit(2);
    }
    let base = args.base.trim_end_matches('/').to_string();
    run(&base, args.n, args.users, args.hotel, &args.username, &args.password).await;
}

async fn run(base: &str, n: usize, users: usize, hotel: i64, username: &str, password: &str) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    // single login (the login endpoint is rate-limited 10/60s)
    let login_url = format!("{base}/api/v1/auth/login");
    let res = client
        .post(&login_url)
        .json(&json!({ "hotel_id": hotel, "username": username, "password": password }))
        .send()
        .await
        .expect("login request failed");
    assert!(res.status().is_success(), "login failed: {}", res.status());
    let body: serde_json::Value = res.json().await.expect("login body");
    let token = body
        .get("data")
        .and_then(|d| d.get("token"))
        .and_then(|t| t.as_str())
        .ok_or("no token in login response")
        .expect("token")
        .to_string();

    let today = chrono::Utc::now().date_naive();
    let day_str = |days: i64| (today + chrono::Duration::days(days)).to_string();

    println!("HMS load test | base={base} users={users} requests/scenario={n}\n");

    let mut table: Vec<(String, u64, Times, Duration, Vec<String>)> = Vec::new();

    // ---- 1. board (21-day window over 60 rooms) -----------------------------
    {
        let url = format!("{base}/api/v1/board?from={}&to={}", day_str(0), day_str(21));
        let client = client.clone();
        let token = token.clone();
        let make = move |_i: usize| {
            let client = client.clone();
            let token = token.clone();
            let url = url.clone();
            Box::pin(async move {
                let start = Instant::now();
                match client.get(&url).bearer_auth(&token).send().await {
                    Ok(res) if res.status().is_success() => {
                        if res.bytes().await.is_err() {
                            return (start.elapsed().as_micros(), Err("board: body read failed".into()));
                        }
                        (start.elapsed().as_micros(), Ok(()))
                    }
                    Ok(res) => {
                        let st = res.status();
                        (start.elapsed().as_micros(), Err(format!("board: HTTP {st}")))
                    }
                    Err(e) => (start.elapsed().as_micros(), Err(format!("board: {e}"))),
                }
            }) as Fut
        };
        table.push(bench("board (21d, 60 rooms)", TARGET_BOARD_MS, n, users, &make).await);
    }

    // ---- 2. availability search (7-day window, sliding offsets) -------------
    {
        let client = client.clone();
        let token = token.clone();
        let make = move |i: usize| {
            let client = client.clone();
            let token = token.clone();
            let url = format!("{base}/api/v1/board?from={}&to={}", day_str(i as i64), day_str(i as i64 + 7));
            Box::pin(async move {
                let start = Instant::now();
                match client.get(&url).bearer_auth(&token).send().await {
                    Ok(res) if res.status().is_success() => {
                        if res.bytes().await.is_err() {
                            return (start.elapsed().as_micros(), Err("availability: body read failed".into()));
                        }
                        (start.elapsed().as_micros(), Ok(()))
                    }
                    Ok(res) => {
                        let st = res.status();
                        (start.elapsed().as_micros(), Err(format!("availability: HTTP {st}")))
                    }
                    Err(e) => (start.elapsed().as_micros(), Err(format!("availability: {e}"))),
                }
            }) as Fut
        };
        table.push(bench("availability (7d sweep)", TARGET_BOARD_MS, n, users, &make).await);
    }

    // ---- 3. reservation save (unique room+dates per request) ----------------
    {
        let rooms_body: serde_json::Value = client
            .get(format!("{base}/api/v1/rooms"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let rooms: Vec<(String, String)> = rooms_body
            .get("data")
            .and_then(|d| d.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| {
                let id = r.get("id")?.as_str()?.to_string();
                Some((id, r.get("room_number")?.as_str()?.to_string()))
            })
            .collect();
        assert!(rooms.len() >= 8, "expected a multi-room hotel for the save test (got {})", rooms.len());
        let channels_body: serde_json::Value = client
            .get(format!("{base}/api/v1/channels"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let channel_id = channels_body
            .get("data")
            .and_then(|d| d.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .find(|c| c["kind"] == "direct")
            .or_else(|| {
                channels_body["data"].as_array().and_then(|a| a.first())
            })
            .and_then(|c| c["id"].as_str())
            .expect("no channel")
            .to_string();

        // far-future walk-away base, unique per run-minute so re-runs do not
        // collide with previously created stays (full reset = re-run seed SQL)
        let epoch_minutes = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() / 60) % 700;
        let base_day = 400 + (epoch_minutes * 10) as i64;

        let client = client.clone();
        let token = token.clone();
        let make = move |i: usize| {
            let client = client.clone();
            let token = token.clone();
            let room_id = rooms[i % rooms.len()].0.clone();
            let channel_id = channel_id.clone();
            // every room gets a distinct 3-day slot per full sweep of the list
            let ci = base_day + ((i / rooms.len()) as i64) * 3;
            let body = json!({
                "room_id": room_id,
                "channel_id": channel_id,
                "guest_id": null,
                "checkin_date": (today + chrono::Duration::days(ci)).to_string(),
                "checkout_date": (today + chrono::Duration::days(ci + 1)).to_string(),
                "adults": 1,
                "children": 0,
            });
            let url = format!("{base}/api/v1/reservations");
            Box::pin(async move {
                let start = Instant::now();
                let r = client.post(&url).bearer_auth(&token).json(&body).send().await;
                match r {
                    Ok(res) if res.status().is_success() => {
                        if res.bytes().await.is_err() {
                            (start.elapsed().as_micros(), Err("save: body read failed".into()))
                        } else {
                            (start.elapsed().as_micros(), Ok(()))
                        }
                    }
                    Ok(res) => {
                        let st = res.status();
                        let txt = res.text().await.unwrap_or_default();
                        (start.elapsed().as_micros(), Err(format!("save: HTTP {st}: {txt}")))
                    }
                    Err(e) => (start.elapsed().as_micros(), Err(format!("save: {e}"))),
                }
            }) as Fut
        };
        table.push(bench("reservation save", TARGET_SAVE_MS, n, users, &make).await);
    }

    // ---- 4. full-year report ------------------------------------------------
    {
        let url = format!("{base}/api/v1/reports/by-day?from={}&to={}", day_str(0), day_str(365));
        let client = client.clone();
        let token = token.clone();
        let make = move |_i: usize| {
            let client = client.clone();
            let token = token.clone();
            let url = url.clone();
            Box::pin(async move {
                let start = Instant::now();
                match client.get(&url).bearer_auth(&token).send().await {
                    Ok(res) if res.status().is_success() => {
                        if res.bytes().await.is_err() {
                            return (start.elapsed().as_micros(), Err("report: body read failed".into()));
                        }
                        (start.elapsed().as_micros(), Ok(()))
                    }
                    Ok(res) => {
                        let st = res.status();
                        (start.elapsed().as_micros(), Err(format!("report: HTTP {st}")))
                    }
                    Err(e) => (start.elapsed().as_micros(), Err(format!("report: {e}"))),
                }
            }) as Fut
        };
        table.push(bench("report full-year (by-day)", TARGET_REPORT_MS, n, users, &make).await);
    }

    // ------------------------------------------------------------------ table
    println!(
        "{:<32} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}  {:<9}",
        "scenario", "p50ms", "meanms", "p95ms", "p99ms", "max", "req/s", "TGT p95"
    );
    let mut all_ok = true;
    for (name, target, t, wall, fails) in &table {
        // requests that failed carry a +1e6 us marker AND an entry in `fails`;
        // latency percentiles use successful samples only for fairness
        let n_fail = fails.len();
        let ok = t.pct(0.95) <= *target as f64 && n_fail == 0;
        all_ok &= ok;
        let rps = (t.count() as f64) / wall.as_secs_f64();
        println!(
            "{:<32} {:>7.1} {:>7.1} {:>7.1} {:>7.1} {:>7.1} {:>7.0}  p95<{:<4} {}",
            name,
            t.pct(0.50),
            t.mean_ms(),
            t.pct(0.95),
            t.pct(0.99),
            t.0.iter().max().map(|m| *m as f64 / 1000.0).unwrap_or(0.0),
            rps,
            target,
            if ok { "OK" } else { "FAIL" },
        );
        if !fails.is_empty() {
            for f in fails.iter().take(3) {
                println!("    error: {f}");
            }
            if fails.len() > 3 {
                println!("    ... and {} more", fails.len() - 3);
            }
            if fails.iter().any(|f| f.contains("no_double_booking")) {
                println!(
                    "\n    hint: the save scenario books far-future one-night slots that previous\n    runs may still hold - reset the load-test hotel first:\n      psql -U postgres -d hms -h localhost -f seed/loadtest.sql"
                );
            }
        }
    }
    println!();
    if all_ok {
        println!("ALL TARGETS MET");
    } else {
        println!("SOME TARGETS MISSED");
        std::process::exit(1);
    }
}
