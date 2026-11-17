//! Platform appearance over the wire.
//!
//! The properties that matter are the ones a request can get wrong and a unit test cannot see: the
//! sign-in screen has to be branded before there is a session, a bad colour has to be refused with a
//! message the console can put beside the input, and a second operator saving the same thing must not
//! look like a change.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-theme-test",
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state.clone());
    tokio::spawn(async move {
        // Connect info matters here: the operator door accepts loopback without a token, and
        // without the peer address the server cannot tell loopback from anywhere else.
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });

    Some((format!("http://{addr}"), state))
}

/// One theme row for the whole platform, so tests that write it have to take turns: run in parallel
/// they read each other's saves and disagree about what changed.
static THEME_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn body(accent: &str, mode: &str) -> serde_json::Value {
    serde_json::json!({
        "product_name": "Acme Home",
        "logo_text": "AH",
        "accent": accent,
        "accent_secondary": "#22D3EE",
        "mode": mode,
    })
}

/// Loopback is the operator door, so these tests need no token — the same rule a sidecar uses.
async fn put(base: &str, payload: serde_json::Value) -> reqwest::Response {
    reqwest::Client::new()
        .put(format!("{base}/v1/ops/theme"))
        .json(&payload)
        .send()
        .await
        .expect("put theme")
}

#[tokio::test]
async fn the_sign_in_screen_can_be_branded_without_a_session() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // No Authorization header at all: the app asks for this before anyone has signed in.
    let res = reqwest::Client::new()
        .get(format!("{base}/v1/theme"))
        .send()
        .await
        .expect("get theme");

    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("public, max-age=60"),
        "a theme that is not cached is a round trip on every cold start"
    );

    let theme: serde_json::Value = res.json().await.expect("json");
    assert!(theme["product_name"].is_string());
    assert!(
        theme["accent"]
            .as_str()
            .unwrap_or_default()
            .starts_with('#')
    );
    assert!(
        theme["accent_on"].as_str().is_some(),
        "the server decides which text colour can be read on the accent"
    );
}

#[tokio::test]
async fn a_phone_that_already_has_the_brand_is_told_nothing_changed() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // The whole test reads and writes the one global row, so it takes the turn for its duration:
    // two tests sharing the theme row is how a validator test fails for a reason that is not the code.
    let _turn = THEME_LOCK.lock().await;

    let client = reqwest::Client::new();
    let first = client
        .get(format!("{base}/v1/theme"))
        .send()
        .await
        .expect("first");
    assert_eq!(first.status(), 200);

    let etag = first
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .expect("an etag: without it every launch re-downloads the palette")
        .to_string();

    let second = client
        .get(format!("{base}/v1/theme"))
        .header("if-none-match", &etag)
        .send()
        .await
        .expect("second");
    assert_eq!(second.status(), 304);
    assert_eq!(second.bytes().await.expect("body").len(), 0);

    // A different theme is a different validator: a 304 here would pin every phone to the old brand.
    assert_eq!(put(&base, body("#10B981", "dark")).await.status(), 200);
    let changed = client
        .get(format!("{base}/v1/theme"))
        .header("if-none-match", &etag)
        .send()
        .await
        .expect("changed");
    assert_eq!(changed.status(), 200);
}

#[tokio::test]
async fn what_the_operator_saves_is_what_the_app_reads() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let _turn = THEME_LOCK.lock().await;
    let saved = put(&base, body("#ff3b30", "light")).await;
    assert_eq!(saved.status(), 200);
    let saved: serde_json::Value = saved.json().await.expect("saved json");
    assert_eq!(
        saved["accent"], "#FF3B30",
        "colours are normalised on the way in"
    );
    assert_eq!(saved["mode"], "light");
    assert_eq!(saved["accent_on"], "#000000", "white on red is 2.9:1");

    let refreshed: serde_json::Value = reqwest::Client::new()
        .get(format!("{base}/v1/theme"))
        .send()
        .await
        .expect("get")
        .json()
        .await
        .expect("json");
    assert_eq!(refreshed["accent"], "#FF3B30");
    assert_eq!(refreshed["product_name"], "Acme Home");
    assert!(refreshed["updated_by"].as_str().is_some());
}

#[tokio::test]
async fn a_colour_that_is_not_one_is_refused_beside_its_field() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let res = put(&base, body("rebeccapurple", "dark")).await;
    // The platform's validation failures are 422, not 400 — the request was well-formed and the
    // value was not, and clients already branch on that distinction elsewhere.
    assert_eq!(res.status(), 422);

    let err: serde_json::Value = res.json().await.expect("error json");
    assert_eq!(err["error"]["code"], "validation_failed");
    assert!(
        err["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("#RRGGBB"),
        "the refusal has to say what a colour looks like: {err}"
    );
    assert_eq!(
        err["error"]["details"]["field"], "accent",
        "the console puts the message beside the input, so the field has to be named"
    );
}

#[tokio::test]
async fn an_unknown_appearance_is_refused_rather_than_ignored() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let res = put(&base, body("#0A84FF", "dusk")).await;
    assert_eq!(res.status(), 422);
    let err: serde_json::Value = res.json().await.expect("error json");
    assert_eq!(err["error"]["details"]["field"], "mode");
}

#[tokio::test]
async fn a_second_identical_save_is_not_recorded_as_a_change() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let _turn = THEME_LOCK.lock().await;

    // Start from a known theme, then save exactly the same thing twice.
    assert_eq!(put(&base, body("#8b5cf6", "dark")).await.status(), 200);
    // The newest row is the receipt: comparing counts would compare a capped page of 20 and stop
    // noticing anything once the log is longer than that.
    let before = state.theme.history(1).await.expect("history");
    assert_eq!(put(&base, body("#8B5CF6", "dark")).await.status(), 200);
    let after = state.theme.history(1).await.expect("history");

    assert_eq!(
        before[0].changed_at, after[0].changed_at,
        "a save that changes nothing must not write a history row: a log of no-ops is a log nobody reads"
    );

    // A real change does write one, and it names the field. The colour is chosen against what is
    // saved now, because a fixed colour is only a change the first time this test ever runs.
    let current = state.theme.load().await.expect("load").accent;
    let next = if current == "#10B981" {
        "#F43F5E"
    } else {
        "#10B981"
    };
    assert_eq!(put(&base, body(next, "dark")).await.status(), 200);
    let changes = state.theme.history(1).await.expect("history");
    assert!(
        changes[0].changed_at > before[0].changed_at,
        "a real change has to leave a newer receipt"
    );
    assert!(changes[0].fields.contains(&"accent".to_string()));
    assert_eq!(
        changes[0].changed_by, "ops",
        "no actor header means 'ops', not nobody"
    );
}

#[tokio::test]
async fn a_second_read_is_answered_without_asking_the_database() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let _turn = THEME_LOCK.lock().await;

    // Prime: the first read after a boot goes to the database.
    let _ = page(&base).await;
    let before = state.theme.cache_hits();

    let again: serde_json::Value = reqwest::Client::new()
        .get(format!("{base}/v1/theme"))
        .send()
        .await
        .expect("read")
        .json()
        .await
        .expect("json");
    let after = state.theme.cache_hits();

    assert!(
        after > before,
        "the second read must come from the process cache, not the database"
    );
    assert!(again["accent"].is_string());
}

#[tokio::test]
async fn a_save_is_visible_before_the_cache_expires() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let _turn = THEME_LOCK.lock().await;

    // Read, save a different accent, read again: the TTL must not hand back the old brand to the
    // operator who just changed it.
    let _ = page(&base).await;
    assert_eq!(put(&base, body("#22C55E", "dark")).await.status(), 200);

    let fresh = state.theme.load().await.expect("load").accent;
    assert_eq!(
        fresh, "#22C55E",
        "a write primes the cache with what it wrote"
    );
}

async fn page(base: &str) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{base}/v1/theme"))
        .send()
        .await
        .expect("theme")
        .json()
        .await
        .expect("json")
}

#[tokio::test]
async fn the_actor_can_be_named_when_the_console_knows_who_is_signed_in() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let _turn = THEME_LOCK.lock().await;

    let res = reqwest::Client::new()
        .put(format!("{base}/v1/ops/theme"))
        .header("x-ops-actor", "priya@operator.test")
        .json(&body("#0EA5E9", "dark"))
        .send()
        .await
        .expect("put with actor");
    assert_eq!(res.status(), 200);

    // The saved row carries the name; asserting on `history()[0]` would be a race with any other
    // test that writes the same single row.
    let saved = state.theme.load().await.expect("load");
    assert_eq!(saved.updated_by.as_deref(), Some("priya@operator.test"));
    let changes = state.theme.history(1).await.expect("history");
    assert_eq!(changes[0].changed_by, "priya@operator.test");
}
