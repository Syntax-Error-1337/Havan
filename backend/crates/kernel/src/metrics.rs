//! Metrics, without a dependency.
//!
//! Three things every service needs before it can be operated: how many requests, how
//! slow, and how many of them failed. This module holds exactly those, as atomics and
//! fixed buckets, and renders them in the Prometheus text format. A metrics library
//! would give more — labels on everything, quantiles, a registry — and would also add a
//! dependency for a service that currently needs five numbers.
//!
//! Cardinality is the reason the route label is a *matched* path (`/v1/devices/{id}`),
//! not the raw URL: labelling by raw path turns one metric into one per device id, which
//! is how a metrics endpoint becomes the thing that takes production down.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Latency buckets in seconds. Chosen around what this service actually does: a device
/// list is ~15 ms, a 30-day history read ~60 ms, and anything over a second is a fault.
const BUCKETS: [f64; 8] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0];

#[derive(Default)]
struct RouteStats {
    count: AtomicU64,
    /// One counter per bucket, plus the running sum and count for `_sum`/`_count`.
    buckets: [AtomicU64; BUCKETS.len()],
    sum_micros: AtomicU64,
}

#[derive(Default)]
struct Registry {
    routes: Mutex<BTreeMap<String, RouteStats>>,
    statuses: Mutex<BTreeMap<(String, u16), u64>>,
    commands_dispatched: AtomicU64,
    commands_undelivered: AtomicU64,
    ws_connections: AtomicU64,
    rate_limited: AtomicU64,
    started_unix: AtomicU64,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let r = Registry::default();
        r.started_unix.store(kernel_now_unix(), Ordering::Relaxed);
        r
    })
}

fn kernel_now_unix() -> u64 {
    u64::try_from(crate::now().unix_timestamp()).unwrap_or(0)
}

/// Record one finished request. `route` is the matched pattern, never a raw path.
pub fn observe_http(route: &str, status: u16, seconds: f64) {
    let reg = registry();

    {
        let mut routes = reg.routes.lock().expect("metrics routes");
        let stats = routes.entry(route.to_string()).or_default();
        stats.count.fetch_add(1, Ordering::Relaxed);
        stats
            .sum_micros
            .fetch_add((seconds * 1_000_000.0) as u64, Ordering::Relaxed);
        for (i, bound) in BUCKETS.iter().enumerate() {
            if seconds <= *bound {
                stats.buckets[i].fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    let mut statuses = reg.statuses.lock().expect("metrics statuses");
    *statuses.entry((route.to_string(), status)).or_insert(0) += 1;
}

pub fn command_dispatched() {
    registry()
        .commands_dispatched
        .fetch_add(1, Ordering::Relaxed);
}

/// Commands the platform refused to queue because there was nowhere to send them. Kept
/// apart from `dispatched`: the difference between the two is the outage.
pub fn command_undelivered() {
    registry()
        .commands_undelivered
        .fetch_add(1, Ordering::Relaxed);
}

pub fn ws_opened() {
    registry().ws_connections.fetch_add(1, Ordering::Relaxed);
}

pub fn ws_closed() {
    // Saturating: a close without an open (a test, a race) must not wrap to 18 quintillion.
    let reg = registry();
    let current = reg.ws_connections.load(Ordering::Relaxed);
    reg.ws_connections
        .store(current.saturating_sub(1), Ordering::Relaxed);
}

pub fn rate_limited() {
    registry().rate_limited.fetch_add(1, Ordering::Relaxed);
}

/// The Prometheus text exposition. Stable ordering, so a diff of two scrapes is readable
/// and a test can assert on it.
pub fn render() -> String {
    let reg = registry();
    let mut out = String::with_capacity(4096);

    out.push_str(
        "# HELP haven_http_requests_total Requests served, by matched route and status.\n",
    );
    out.push_str("# TYPE haven_http_requests_total counter\n");
    {
        let statuses = reg.statuses.lock().expect("metrics statuses");
        for ((route, status), count) in statuses.iter() {
            out.push_str(&format!(
                "haven_http_requests_total{{route=\"{route}\",status=\"{status}\"}} {count}\n"
            ));
        }
    }

    out.push_str("# HELP haven_http_request_duration_seconds Request latency, by matched route.\n");
    out.push_str("# TYPE haven_http_request_duration_seconds histogram\n");
    {
        let routes = reg.routes.lock().expect("metrics routes");
        for (route, stats) in routes.iter() {
            let total = stats.count.load(Ordering::Relaxed);
            for (i, bound) in BUCKETS.iter().enumerate() {
                let count = stats.buckets[i].load(Ordering::Relaxed);
                out.push_str(&format!(
                    "haven_http_request_duration_seconds_bucket{{route=\"{route}\",le=\"{bound}\"}} {count}\n"
                ));
            }
            out.push_str(&format!(
                "haven_http_request_duration_seconds_bucket{{route=\"{route}\",le=\"+Inf\"}} {total}\n"
            ));
            let seconds = stats.sum_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            out.push_str(&format!(
                "haven_http_request_duration_seconds_sum{{route=\"{route}\"}} {seconds:.6}\n"
            ));
            out.push_str(&format!(
                "haven_http_request_duration_seconds_count{{route=\"{route}\"}} {total}\n"
            ));
        }
    }

    let commands = reg.commands_dispatched.load(Ordering::Relaxed);
    out.push_str("# HELP haven_commands_dispatched_total Commands accepted from clients.\n");
    out.push_str("# TYPE haven_commands_dispatched_total counter\n");
    out.push_str(&format!("haven_commands_dispatched_total {commands}\n"));

    let undelivered = reg.commands_undelivered.load(Ordering::Relaxed);
    out.push_str(
        "# HELP haven_commands_undelivered_total Commands refused because the broker was down.\n",
    );
    out.push_str("# TYPE haven_commands_undelivered_total counter\n");
    out.push_str(&format!("haven_commands_undelivered_total {undelivered}\n"));

    out.push_str("# HELP haven_ws_connections WebSocket streams open right now.\n");
    out.push_str("# TYPE haven_ws_connections gauge\n");
    out.push_str(&format!(
        "haven_ws_connections {}\n",
        reg.ws_connections.load(Ordering::Relaxed)
    ));

    out.push_str("# HELP haven_rate_limited_total Requests refused by a rate limit.\n");
    out.push_str("# TYPE haven_rate_limited_total counter\n");
    out.push_str(&format!(
        "haven_rate_limited_total {}\n",
        reg.rate_limited.load(Ordering::Relaxed)
    ));

    let up = kernel_now_unix().saturating_sub(reg.started_unix.load(Ordering::Relaxed));
    out.push_str("# HELP haven_uptime_seconds Seconds since this process started.\n");
    out.push_str("# TYPE haven_uptime_seconds gauge\n");
    out.push_str(&format!("haven_uptime_seconds {up}\n"));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_request_shows_up_once() {
        observe_http("/test/route", 200, 0.012);
        let text = render();
        assert!(text.contains("haven_http_requests_total{route=\"/test/route\",status=\"200\"} 1"));
        assert!(
            text.contains("haven_http_request_duration_seconds_count{route=\"/test/route\"} 1")
        );
    }

    #[test]
    fn buckets_are_cumulative_so_a_scrape_can_do_quantiles() {
        // Two requests: one fast, one slow. The 5 ms bucket must hold only the first,
        // and +Inf must hold both — that is what "cumulative" means, and a histogram
        // without it is a chart that lies.
        observe_http("/test/buckets", 200, 0.001);
        observe_http("/test/buckets", 200, 0.400);
        let text = render();
        assert!(text.contains(
            "haven_http_request_duration_seconds_bucket{route=\"/test/buckets\",le=\"0.005\"} 1"
        ));
        assert!(text.contains(
            "haven_http_request_duration_seconds_bucket{route=\"/test/buckets\",le=\"+Inf\"} 2"
        ));
    }

    #[test]
    fn a_close_without_an_open_does_not_wrap_the_gauge() {
        ws_opened();
        ws_closed();
        ws_closed();
        let text = render();
        assert!(
            text.contains("haven_ws_connections 0"),
            "gauge underflowed: {text}"
        );
    }

    #[test]
    fn the_output_is_valid_prometheus_shape() {
        observe_http("/test/shape", 500, 0.2);
        for line in render().lines().filter(|l| !l.starts_with('#')) {
            assert!(
                line.rsplit_once(' ')
                    .is_some_and(|(_, v)| v.parse::<f64>().is_ok() || v == "NaN"),
                "not a metric line: {line}"
            );
        }
    }
}
