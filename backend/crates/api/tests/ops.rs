//! Rate limits, and the metrics endpoint they feed.
//!
//! Both are operational behaviour, so both are tested the way an operator meets them:
//! over HTTP, against a real cache, counting what comes back.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-ops-test",
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });

    Some((format!("http://{addr}"), state))
}

async fn login_attempt(base: &str, email: &str, password: &str) -> u16 {
    reqwest::Client::new()
        .post(format!("{base}/v1/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login")
        .status()
        .as_u16()
}

#[tokio::test]
async fn a_password_spray_runs_out_of_attempts_but_the_neighbour_does_not() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // A fresh address per run: the limiter's window is a minute, and a fixed email would
    // make the second run of this test start already-exhausted.
    let victim = format!("spray-{}@haven.dev", uuid::Uuid::now_v7());
    let neighbour = format!("neighbour-{}@haven.dev", uuid::Uuid::now_v7());

    let mut codes = Vec::new();
    for attempt in 0..11 {
        codes.push(login_attempt(&base, &victim, &format!("wrong-{attempt}")).await);
    }

    // Ten wrong passwords answer "wrong password"; the eleventh answers "stop".
    assert!(
        codes[..10].iter().all(|c| *c == 401),
        "the first ten should be plain failures: {codes:?}"
    );
    assert_eq!(
        *codes.last().expect("codes"),
        429,
        "the eleventh is limited"
    );

    // The limit is per account: somebody else behind the same address is not locked out.
    assert_eq!(
        login_attempt(&base, &neighbour, "wrong").await,
        401,
        "a different account must not inherit the limit"
    );
}

#[tokio::test]
async fn metrics_render_on_the_loopback_and_count_what_happened() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // Generate one request to count, then read the exposition.
    let _ = reqwest::Client::new()
        .get(format!("{base}/healthz"))
        .send()
        .await
        .expect("health");

    let response = reqwest::Client::new()
        .get(format!("{base}/metrics"))
        .send()
        .await
        .expect("metrics");

    assert_eq!(response.status().as_u16(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.starts_with("text/plain"),
        "Prometheus expects text/plain, got {content_type}"
    );

    let body = response.text().await.expect("body");
    // The status is whatever health reported in this process — the broker may not have
    // answered yet, which is a 503 — so the assertion is on the *route label*, which is
    // the thing the middleware exists to produce.
    assert!(
        body.contains("haven_http_requests_total{route=\"/healthz\""),
        "the request we just made is not in the exposition; first 400 chars: {}",
        body.chars().take(400).collect::<String>()
    );
    // Labelled by matched route, not raw path: a metric per device id is how a metrics
    // endpoint becomes the outage.
    assert!(
        !body.contains(&format!("route=\"{base}")),
        "raw URLs leaked into labels"
    );
}
