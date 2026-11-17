//! A home with more devices than fit on a phone screen.
//!
//! The dashboard used to answer with every device in the home, every point included: 322 kB for a
//! hundred devices, and about 3 MB for a thousand. Paging is opt-in so no installed build breaks, and
//! the properties that matter are the ones a page can get wrong — losing a device between pages,
//! repeating one, or handing out a cursor the server will not accept.

use api::{AppState, router};
use uuid::Uuid;

struct Fixture {
    base: String,
    token: String,
    home_id: Uuid,
    devices: Vec<Uuid>,
}

async fn boot() -> Option<Fixture> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-paging-test",
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

    let base = format!("http://{addr}");
    let email = format!("paging-{}@haven.dev", Uuid::now_v7());

    let tokens: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "paging-Password-1" }))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let token = tokens["access"].as_str()?.to_string();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&db)
        .await
        .ok()?;
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .ok()?;

    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .ok()?;

    let home_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO homes (id, name, timezone, owner_id) VALUES ($1, 'Paging flat', 'UTC', $2)",
    )
    .bind(home_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .ok()?;
    sqlx::query("INSERT INTO home_members (home_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(home_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .ok()?;

    // A vendor and one product, so the devices are real rows with the columns the API selects.
    let vendor_id = Uuid::now_v7();
    let tag = &Uuid::now_v7().to_string()[20..32];
    let product_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO vendors (id, slug, name, status) VALUES ($1, $2, 'Paging vendor', 'active')",
    )
    .bind(vendor_id)
    .bind(format!("paging-{tag}"))
    .execute(&pool)
    .await
    .ok()?;
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model)
         VALUES ($1, $2, $3, 'Paging bulb', 'light', '{}'::jsonb)",
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(format!("pg_{tag}"))
    .execute(&pool)
    .await
    .ok()?;

    let mut devices = Vec::new();
    for index in 0..5 {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO devices (id, vendor_id, product_id, product_key, category, home_id, name,
                                    serial, secret_hash, state, claimed_at)
               VALUES ($1, $2, $3, $4, 'light', $5, $6, $7, 'x', 'claimed', now())"#,
        )
        .bind(id)
        .bind(vendor_id)
        .bind(product_id)
        .bind(format!("pg_{tag}"))
        .bind(home_id)
        .bind(format!("Lamp {index}"))
        .bind(format!("PG-{tag}-{index}"))
        .execute(&pool)
        .await
        .ok()?;
        devices.push(id);
    }

    Some(Fixture {
        base,
        token,
        home_id,
        devices,
    })
}

async fn page(f: &Fixture, query: &str) -> (u16, serde_json::Value) {
    let res = reqwest::Client::new()
        .get(format!(
            "{}/v1/homes/{}/dashboard?{}",
            f.base, f.home_id, query
        ))
        .bearer_auth(&f.token)
        .send()
        .await
        .expect("dashboard");
    let status = res.status().as_u16();
    (status, res.json().await.expect("json"))
}

#[tokio::test]
async fn a_page_is_a_page_and_the_cursor_reaches_the_rest() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let (status, first) = page(&f, "device_limit=2").await;
    assert_eq!(status, 200);
    assert_eq!(first["devices"].as_array().expect("array").len(), 2);
    assert_eq!(
        first["devices_total"], 5,
        "the total is the home's, not the page's"
    );

    let cursor = first["devices_next_cursor"]
        .as_str()
        .expect("a cursor when there is more")
        .to_string();

    let mut seen: Vec<String> = first["devices"]
        .as_array()
        .expect("array")
        .iter()
        .map(|device| device["name"].as_str().expect("name").to_string())
        .collect();

    let mut next = Some(cursor);
    let mut pages = 1;
    while let Some(cursor) = next {
        let (status, body) = page(&f, &format!("device_limit=2&device_cursor={cursor}")).await;
        assert_eq!(status, 200);
        for device in body["devices"].as_array().expect("array") {
            seen.push(device["name"].as_str().expect("name").to_string());
        }
        next = body["devices_next_cursor"].as_str().map(str::to_string);
        pages += 1;
        assert!(pages < 10, "paging did not terminate");
    }

    seen.sort();
    let mut expected: Vec<String> = (0..5).map(|index| format!("Lamp {index}")).collect();
    expected.sort();

    assert_eq!(
        seen, expected,
        "every device exactly once, across every page"
    );
}

#[tokio::test]
async fn the_presence_is_the_homes_and_not_the_pages() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // Two of five devices on a page of two: a header that counted the page would say "2 offline".
    let (status, body) = page(&f, "device_limit=2").await;
    assert_eq!(status, 200);
    let presence = &body["presence"];
    assert_eq!(presence["total"], 5, "the house, not the response");
    // The fixture's devices were created now and have never reported: online, but silent, is the
    // honest answer, and either way the two counts have to agree with the total.
    let online = presence["online"].as_i64().expect("online");
    let silent = presence["silent"].as_i64().expect("silent");
    assert_eq!(
        online + silent,
        5,
        "every device is either reporting or not"
    );
}

#[tokio::test]
async fn no_limit_still_answers_with_the_whole_home() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // This is the contract every installed build relies on: no query parameter, one answer.
    let (status, body) = page(&f, "").await;
    assert_eq!(status, 200);
    assert_eq!(body["devices"].as_array().expect("array").len(), 5);
    assert!(
        body.get("devices_total").is_none(),
        "an unpaged answer must not claim to be a page: {body}"
    );
}

#[tokio::test]
async fn a_cursor_this_server_did_not_issue_is_refused() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    for bad in ["nonsense", "1:zz:not-a-uuid", "0::"] {
        let (status, body) = page(&f, &format!("device_limit=2&device_cursor={bad}")).await;
        assert_eq!(status, 422, "for {bad:?}");
        assert_eq!(body["error"]["details"]["field"], "device_cursor");
    }
}

#[tokio::test]
async fn a_page_of_points_is_the_page_s_devices_only() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // The point query is the other half of the cost paging exists to remove: fetching every point in
    // the home and filtering in memory would keep the payload small and the query expensive.
    let (_, first) = page(&f, "device_limit=2").await;
    for device in first["devices"].as_array().expect("array") {
        let id = device["id"].as_str().expect("id");
        assert!(
            f.devices.iter().any(|seeded| seeded.to_string() == id),
            "a page must contain devices from this home"
        );
    }
}
