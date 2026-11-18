//! Sorting the fleet, which the table could not do at all.
//!
//! The order is a closed set of columns and the cursor stays a bare id: the keyset recomputes the ordering value of the
//! cursor row, so a page break and a sort cannot disagree about what "next" means. These tests drive it on a socket -
//! the point is not that SQL runs, but that page two continues where page one stopped for every column, and that an
//! unknown column is refused rather than passed through to an ORDER BY.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-fleet-sort-test",
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Some((format!("http://{addr}"), state))
}

/// The demo vendor, whose seeded fleet is large enough for two pages.
const VENDOR: &str = "11111111-1111-7111-8111-111111111111";

async fn token(base: &str) -> Option<String> {
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/vendor/auth/login"))
        .json(&serde_json::json!({
            "vendor": VENDOR,
            "email": "admin@acme.test",
            "password": "acme-demo-password",
        }))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    Some(body["access"].as_str()?.to_string())
}

async fn page(base: &str, token: &str, query: &str) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{base}/v1/vendor/devices?{query}"))
        .bearer_auth(token)
        .send()
        .await
        .expect("page")
        .json()
        .await
        .expect("json")
}

fn serials(body: &serde_json::Value) -> Vec<String> {
    body["devices"]
        .as_array()
        .expect("devices")
        .iter()
        .map(|d| d["serial"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn the_fleet_can_be_ordered_and_the_next_page_continues_the_same_order() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let Some(token) = token(&base).await else {
        eprintln!("skipping: the demo vendor is not seeded");
        return;
    };

    for (sort, first_page_should_be_ascending) in
        [("serial", true), ("name", true), ("created", false)]
    {
        let first = page(&base, &token, &format!("limit=5&sort={sort}")).await;
        let second = page(
            &base,
            &token,
            &format!(
                "limit=5&sort={sort}&cursor={}",
                first["next_cursor"].as_str().unwrap_or_default()
            ),
        )
        .await;

        let a = serials(&first);
        let b = serials(&second);
        assert_eq!(a.len(), 5, "{sort}: a page asks for five");
        assert!(
            !b.is_empty(),
            "{sort}: there is a second page to continue into"
        );
        assert!(
            a.iter().collect::<Vec<_>>() != b.iter().collect::<Vec<_>>(),
            "{sort}: page two repeated page one"
        );

        // The two pages must not overlap: the keyset is doing its job.
        for serial in &b {
            assert!(
                !a.contains(serial),
                "{sort}: {serial} appeared on both pages"
            );
        }

        if first_page_should_be_ascending {
            let mut sorted = a.clone();
            sorted.sort();
            assert_eq!(
                a, sorted,
                "{sort}: the default direction is the one it is read in"
            );
        }
    }
}

#[tokio::test]
async fn an_unknown_sort_or_direction_is_refused_with_the_value_that_is_wrong() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let Some(token) = token(&base).await else {
        eprintln!("skipping: the demo vendor is not seeded");
        return;
    };
    let client = reqwest::Client::new();

    for (query, expected) in [
        (
            "sort=d.created_at%3B%20DROP%20TABLE%20devices",
            "unknown sort",
        ),
        ("sort=serial&dir=sideways", "asc or desc"),
    ] {
        let response = client
            .get(format!("{base}/v1/vendor/devices?{query}"))
            .bearer_auth(&token)
            .send()
            .await
            .expect("request");
        assert_eq!(response.status().as_u16(), 422, "{query}");
        let body: serde_json::Value = response.json().await.expect("json");
        assert!(
            body["error"]["message"]
                .as_str()
                .is_some_and(|m| m.contains(expected)),
            "{query}: {body}"
        );
    }
}
