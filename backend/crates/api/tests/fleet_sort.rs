//! Sorting the fleet, which the table could not do at all.
//!
//! The order is a closed set of columns and the cursor stays a bare id: the keyset recomputes the ordering value of the
//! cursor row, so a page break and a sort cannot disagree about what "next" means. These tests drive it on a socket —
//! the point is not that SQL runs, but that page two continues where page one stopped for every column, that an unknown
//! column is refused rather than passed through to an ORDER BY, and that a cursor cannot carry a vendor across its own
//! boundary.
//!
//! **The fixture is built, not borrowed.** These tests used to log in as the seeded demo vendor, which exists in the dev
//! database and not in `haven_test` — so every one of them took the early `return` and passed without touching the API.
//! A suite that always passes is worse than a missing one: it reports coverage it does not have.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        &format!("{}-{}", "haven-api-fleet-sort-test", uuid::Uuid::now_v7()),
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

/// 120 devices is more than one page at 50 and small enough to be quick: the property under test is a page break, not a
/// fleet size.
const FLEET: usize = 120;

struct Fixture {
    /// The vendor whose fleet the tests page through. Every serial it owns starts with `SORT-`.
    token: String,
    /// A device belonging to somebody else — a valid id, with the wrong owner.
    foreign_device: uuid::Uuid,
}

/// Fill a vendor: account, product, and a fleet with the two properties a keyset has to answer for.
async fn build_vendor(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tag: &str,
    serial_prefix: &str,
    count: usize,
) -> Option<(uuid::Uuid, String, String, Vec<uuid::Uuid>)> {
    use domain::identity::hash_password;

    let vendor = uuid::Uuid::now_v7();
    let product = uuid::Uuid::now_v7();
    let simple = vendor.simple().to_string();
    let email = format!("{tag}-{simple}@vendor.test");
    let password = "fleet-sort-Password-1".to_string();
    let hash = hash_password(&password).expect("hash");
    // The tail, not the head: a v7 uuid starts with a timestamp, so two vendors created in the same
    // millisecond share their first characters and the slug collides.
    let tail = &simple[simple.len() - 8..];
    let product_key = format!("hv_{tag}_{tail}");

    sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
        .bind(vendor.to_string())
        .execute(&mut **tx)
        .await
        .expect("scope");
    sqlx::query("INSERT INTO vendors (id, name, slug, created_at) VALUES ($1, $2, $3, now())")
        .bind(vendor)
        .bind(format!("Fleet {tag}"))
        .bind(format!("{tag}-{tail}"))
        .execute(&mut **tx)
        .await
        .expect("vendor");
    sqlx::query(
        r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash, created_at)
           VALUES ($1, $2, $3, $4, 'owner', $5, now())"#,
    )
    .bind(uuid::Uuid::now_v7())
    .bind(vendor)
    .bind(&email)
    .bind(format!("Fleet {tag}"))
    .bind(hash)
    .execute(&mut **tx)
    .await
    .expect("vendor user");
    sqlx::query(
        r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status, created_at)
           VALUES ($1, $2, $3, 'Sortable', 'sensor', '{}'::jsonb, 'published', now())"#,
    )
    .bind(product)
    .bind(vendor)
    .bind(&product_key)
    .execute(&mut **tx)
    .await
    .expect("product");

    let mut ids = Vec::new();
    for index in 0..count {
        let id = uuid::Uuid::now_v7();
        ids.push(id);
        // Serials deliberately out of insertion order, and every tenth device has never reported: the two cases the
        // keyset has to answer for, and a ramp of identical rows would hide both.
        sqlx::query(
            r#"INSERT INTO devices (id, vendor_id, product_id, name, serial, secret_hash, product_key,
                                   category, state, last_seen_at)
               VALUES ($1, $2, $3, $4, $5, 'x', $6, 'sensor', 'unclaimed',
                       CASE WHEN $7::int IS NULL THEN NULL ELSE now() - ($7::int || ' minutes')::interval END)"#,
        )
        .bind(id)
        .bind(vendor)
        .bind(product)
        .bind(format!("{tag} Sensor {:03}", count - index))
        .bind(format!("{serial_prefix}-{:03}", (index * 37) % count))
        .bind(&product_key)
        // The clock is the database's: a test that computes its own timestamps can disagree with `now()` on the server.
        .bind(if index % 10 == 0 {
            None
        } else {
            Some(index as i32)
        })
        .execute(&mut **tx)
        .await
        .expect("device");
    }

    Some((vendor, email, password, ids))
}

/// One vendor with a fleet, and a second vendor with three devices to borrow an id from.
async fn fixture(base: &str, state: &AppState) -> Option<Fixture> {
    let mut tx = state.pg.pool().begin().await.ok()?;
    let (vendor, email, password, _) = build_vendor(&mut tx, "sort", "SORT", FLEET).await?;
    let (_, _, _, other_ids) = build_vendor(&mut tx, "other", "OTHER", 3).await?;
    sqlx::query("SELECT set_config('haven.vendor_id', '', false)")
        .execute(&mut *tx)
        .await
        .ok()?;
    tx.commit().await.ok()?;

    let response = reqwest::Client::new()
        .post(format!("{base}/v1/vendor/auth/login"))
        .json(&serde_json::json!({ "vendor_id": vendor, "email": email, "password": password }))
        .send()
        .await
        .expect("login");
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    assert!(status.is_success(), "fixture login failed {status}: {body}");

    Some(Fixture {
        token: body["access"].as_str()?.to_string(),
        foreign_device: *other_ids.first()?,
    })
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
    column(body, "serial")
}

/// One column of the page, in the order the server returned it.
fn column(body: &serde_json::Value, key: &str) -> Vec<String> {
    body["devices"]
        .as_array()
        .expect("devices")
        .iter()
        .map(|d| d[key].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn the_fleet_can_be_ordered_and_the_next_page_continues_the_same_order() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let fixture = fixture(&base, &state).await.expect("fixture");
    let token = fixture.token;

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
        // This fixture is the only fleet in these tests, so any other serial would be a tenancy failure wearing a
        // sorting hat.
        for serial in a.iter().chain(b.iter()) {
            assert!(serial.starts_with("SORT-"), "{sort}: {serial} is not ours");
        }

        if first_page_should_be_ascending {
            // Assert on the column that is actually ordered. Checking the serials after sorting by name would be a
            // test of nothing: the names ascend and the serials arrive in whatever order those names implied.
            let mut values = column(&first, sort);
            let ordered = values.clone();
            values.sort();
            assert_eq!(
                ordered, values,
                "{sort}: the default direction is the one it is read in"
            );
        }
    }
}

/// A device that never reported must sort as the *oldest*, not vanish.
///
/// `COALESCE(last_seen_at, 'epoch')` is what makes that true. Without it a comparison against NULL is never true, so the
/// rows without a timestamp drop out of the walk entirely — invisible on page one, and gone from every page after it.
#[tokio::test]
async fn a_device_that_never_reported_sorts_as_the_oldest_and_is_not_lost() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let fixture = fixture(&base, &state).await.expect("fixture");

    let first = page(&base, &fixture.token, "limit=12&sort=last_seen&dir=asc").await;
    let silent_on_page_one = first["devices"]
        .as_array()
        .expect("devices")
        .iter()
        .filter(|device| device["last_seen_at"].is_null())
        .count();
    assert!(
        silent_on_page_one > 0,
        "the never-seen devices are the oldest, so they are on page one: {first}"
    );

    // Walk the whole fleet: every device must be reachable, including the ones with no timestamp at all.
    let mut reached = serials(&first).len();
    let mut silent_total = silent_on_page_one;
    let mut cursor = first["next_cursor"].as_str().map(str::to_string);
    while let Some(next) = cursor {
        let body = page(
            &base,
            &fixture.token,
            &format!("limit=12&sort=last_seen&dir=asc&cursor={next}"),
        )
        .await;
        let batch = serials(&body);
        if batch.is_empty() {
            break;
        }
        reached += batch.len();
        silent_total += body["devices"]
            .as_array()
            .expect("devices")
            .iter()
            .filter(|device| device["last_seen_at"].is_null())
            .count();
        cursor = body["next_cursor"].as_str().map(str::to_string);
    }

    assert_eq!(reached, FLEET, "the walk reached every device");
    assert_eq!(
        silent_total,
        FLEET / 10,
        "every never-seen device was reachable, not only the ones on page one"
    );
}

#[tokio::test]
async fn an_unknown_sort_or_direction_is_refused_with_the_value_that_is_wrong() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let fixture = fixture(&base, &state).await.expect("fixture");
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
            .bearer_auth(&fixture.token)
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

/// A cursor is a bare device id, and an id from another vendor must not move this vendor's page.
///
/// The keyset reads the cursor row to find out what "next" means, so the one thing that would make the boundary porous
/// is that lookup being unscoped: vendor B passing vendor A's device id would then page *relative to A's device*, and
/// while the returned rows should still be B's, the shape of B's answer would be dictated by data B cannot see. This
/// drives the real case — a valid id, owned by somebody else — and asserts the answer is ordinary and stays inside A's
/// fleet.
#[tokio::test]
async fn a_cursor_from_another_vendor_does_not_cross_the_boundary() {
    let Some((base, state)) = boot().await else {
        eprintln!("skip: database or redis unavailable");
        return;
    };
    let fixture = fixture(&base, &state).await.expect("fixture");

    let response = reqwest::Client::new()
        .get(format!(
            "{base}/v1/vendor/devices?limit=50&sort=serial&dir=asc&cursor={}",
            fixture.foreign_device
        ))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .expect("page");

    // Not a 500, and not a refusal that tells the caller whether that id exists.
    assert!(
        response.status().is_success(),
        "a cursor this vendor does not own is an ordinary page, got {}",
        response.status()
    );
    let body: serde_json::Value = response.json().await.expect("json");
    for serial in serials(&body) {
        assert!(
            serial.starts_with("SORT-"),
            "the page must contain only this vendor's fleet, saw {serial}"
        );
    }
}
