//! The headers every response carries, whatever it answers.
//!
//! These are cheap and easy to lose: a layer added in the wrong order, a route that builds its own response and skips
//! the middleware, an error path that returns before the headers are attached. The property worth locking in is that an
//! *error* carries them too — a 401 with `nosniff` is the one a browser might otherwise sniff and act on.

use api::{AppState, router};

async fn boot() -> Option<String> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        &format!("{}-{}", "haven-api-headers-test", uuid::Uuid::now_v7()),
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });
    Some(format!("http://{addr}"))
}

#[tokio::test]
async fn every_response_says_what_it_is_and_what_may_be_done_with_it() {
    let Some(base) = boot().await else {
        eprintln!("skip: database or redis unavailable");
        return;
    };
    let client = reqwest::Client::new();

    // A success, an error, and the two probes — different paths through the stack, same contract.
    for path in ["/livez", "/v1/theme", "/v1/homes"] {
        let response = client
            .get(format!("{base}{path}"))
            .send()
            .await
            .expect("request");

        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };

        assert_eq!(
            header("x-content-type-options").as_deref(),
            Some("nosniff"),
            "{path}"
        );
        assert_eq!(header("x-frame-options").as_deref(), Some("DENY"), "{path}");
        assert_eq!(
            header("referrer-policy").as_deref(),
            Some("no-referrer"),
            "{path}"
        );
        assert_eq!(
            header("content-security-policy").as_deref(),
            Some("default-src 'none'; frame-ancestors 'none'"),
            "{path}: this API returns data, never a document — the policy says so"
        );
        // The API does not terminate TLS, so a promise about it here would be about somebody else's connection.
        assert!(header("strict-transport-security").is_none(), "{path}");
    }

    // The third path above is a 401 with a JSON body; prove it stayed JSON rather than becoming an HTML error page.
    let unauthorized = client
        .get(format!("{base}/v1/homes"))
        .send()
        .await
        .expect("request");
    assert_eq!(unauthorized.status(), 401);
    assert_eq!(
        unauthorized
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("application/json")
    );
}
