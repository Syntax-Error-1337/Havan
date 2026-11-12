//! Rollback: which devices it touches, and when it refuses.
//!
//! The two properties that matter are the ones a mistake would cost devices: it must
//! queue **only** the devices running the bad image, and it must refuse when there is
//! nothing to go back to — silently doing nothing while claiming success is how a
//! stranded fleet gets discovered by customers instead of by the vendor.

use api::{AppState, router};
use uuid::Uuid;

struct Harness {
    base: String,
    db: sqlx::PgPool,
}

async fn boot() -> Option<Harness> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db_url, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-rollback-test",
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let db = sqlx::PgPool::connect(&db_url).await.ok()?;
    Some(Harness {
        base: format!("http://{addr}"),
        db,
    })
}

/// A vendor with a product, two firmware images and three devices: one on the good
/// image, two on the bad one.
struct Fixture {
    vendor: Uuid,
    token: String,
    product: Uuid,
    good: Uuid,
    bad: Uuid,
    bad_version: String,
    on_bad: Vec<Uuid>,
}

impl Harness {
    async fn fixture(&self) -> Fixture {
        let vendor = Uuid::now_v7();
        let simple = vendor.simple().to_string();
        let slug = format!("rb-{}", &simple[simple.len() - 8..]);
        let email = format!("rb-{vendor}@vendor.test");
        let password = "rollback-Password-1";
        let hash = domain::identity::hash_password(password).expect("hash");

        let product = Uuid::now_v7();
        let good = Uuid::now_v7();
        let bad = Uuid::now_v7();
        // Versions carry the uuid tail so repeated runs do not collide on
        // `firmware (product_id, version)`.
        let tail = &simple[simple.len() - 6..];
        let good_version = format!("1.0.{tail}");
        let bad_version = format!("9.{tail}");

        // Seeded inside the vendor's scope, the way onboarding would: the policies apply
        // to the fixture too, so a fixture that gets it wrong fails here rather than
        // passing and hiding a policy hole.
        let mut conn = self.db.acquire().await.expect("conn");
        sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
            .bind(vendor.to_string())
            .execute(&mut *conn)
            .await
            .expect("scope");
        sqlx::query("INSERT INTO vendors (id, name, slug, created_at) VALUES ($1,'RB',$2,now())")
            .bind(vendor)
            .bind(&slug)
            .execute(&mut *conn)
            .await
            .expect("vendor");
        sqlx::query(
            r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash, created_at)
               VALUES ($1,$2,$3,'RB','owner',$4,now())"#,
        )
        .bind(Uuid::now_v7())
        .bind(vendor)
        .bind(&email)
        .bind(hash)
        .execute(&mut *conn)
        .await
        .expect("vendor user");
        sqlx::query(
            r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model)
               VALUES ($1,$2,$3,'RB bulb','light','{}'::jsonb)"#,
        )
        .bind(product)
        .bind(vendor)
        .bind(format!("rb_{}", &simple[simple.len() - 6..]))
        .execute(&mut *conn)
        .await
        .expect("product");

        // `good` is older; `bad` comes after it, so a rollback from `bad` may choose it.
        for (id, version, minutes_ago) in [
            (good, good_version.clone(), 10),
            (bad, bad_version.clone(), 5),
        ] {
            // `firmware` carries no vendor column: an image belongs to a product, and the
            // product belongs to the vendor.
            sqlx::query(
                r#"INSERT INTO firmware (id, product_id, version, size, sha256, ed25519_sig, object_key, created_at)
                   VALUES ($1,$2,$3,1024,$4,'sig',$5, now() - ($6 || ' minutes')::interval)"#,
            )
            .bind(id)
            .bind(product)
            .bind(&version)
            .bind("a".repeat(64))
            .bind(format!("fw/{version}.bin"))
            .bind(minutes_ago.to_string())
            .execute(&mut *conn)
            .await
            .expect("firmware");
        }

        let mut on_bad = Vec::new();
        for (index, version) in [
            good_version.clone(),
            bad_version.clone(),
            bad_version.clone(),
        ]
        .into_iter()
        .enumerate()
        {
            let device = Uuid::now_v7();
            sqlx::query(
                r#"INSERT INTO devices (id, vendor_id, product_id, product_key, name, serial,
                                        secret_hash, state, category, fw_version)
                   VALUES ($1,$2,$3,'rb_' || $4, $5, $6, 'x', 'claimed', 'light', $7)"#,
            )
            .bind(device)
            .bind(vendor)
            .bind(product)
            .bind(&simple[simple.len() - 6..])
            .bind(format!("device {index}"))
            .bind(format!("RB-{simple}-{index}"))
            .bind(&version)
            .execute(&mut *conn)
            .await
            .expect("device");
            if version == bad_version {
                on_bad.push(device);
            }
        }
        sqlx::query("SELECT set_config('haven.vendor_id', '', false)")
            .execute(&mut *conn)
            .await
            .expect("unscope");
        drop(conn);

        let token: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/v1/vendor/auth/login", self.base))
            .json(&serde_json::json!({ "vendor_id": vendor, "email": email, "password": password }))
            .send()
            .await
            .expect("login")
            .json()
            .await
            .expect("tokens");

        Fixture {
            vendor,
            token: token["access"].as_str().expect("access").to_string(),
            product,
            good,
            bad,
            bad_version,
            on_bad,
        }
    }

    async fn campaign(&self, f: &Fixture, firmware: Uuid) -> Uuid {
        let campaign: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/v1/vendor/campaigns", self.base))
            .bearer_auth(&f.token)
            .json(&serde_json::json!({
                "product_id": f.product, "firmware_id": firmware, "rollout_pct": 100
            }))
            .send()
            .await
            .expect("campaign")
            .json()
            .await
            .expect("campaign body");
        campaign["id"]
            .as_str()
            .expect("campaign id")
            .parse()
            .expect("uuid")
    }

    async fn rollback(&self, f: &Fixture, campaign: Uuid) -> (u16, serde_json::Value) {
        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/vendor/campaigns/{campaign}/rollback",
                self.base
            ))
            .bearer_auth(&f.token)
            .send()
            .await
            .expect("rollback");
        let status = response.status().as_u16();
        (
            status,
            response.json().await.unwrap_or(serde_json::Value::Null),
        )
    }

    /// Queued devices, counted inside the vendor's scope. A bare-pool query returns
    /// nothing at all here: row-level security fails closed, and an empty list would look
    /// exactly like "the rollback queued nothing".
    async fn jobs(&self, vendor: Uuid, campaign: Uuid) -> Vec<Uuid> {
        let mut conn = self.db.acquire().await.expect("conn");
        sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
            .bind(vendor.to_string())
            .execute(&mut *conn)
            .await
            .expect("scope");
        let rows = sqlx::query_scalar("SELECT device_id FROM ota_jobs WHERE campaign_id = $1")
            .bind(campaign)
            .fetch_all(&mut *conn)
            .await
            .expect("jobs");
        sqlx::query("SELECT set_config('haven.vendor_id', '', false)")
            .execute(&mut *conn)
            .await
            .expect("unscope");
        rows
    }
}

#[tokio::test]
async fn a_rollback_queues_only_the_devices_running_the_bad_image() {
    let Some(h) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let f = h.fixture().await;
    let campaign = h.campaign(&f, f.bad).await;
    let start = reqwest::Client::new()
        .post(format!("{}/v1/vendor/campaigns/{campaign}/start", h.base))
        .bearer_auth(&f.token)
        .send()
        .await
        .expect("start");
    assert_eq!(start.status().as_u16(), 200);

    let (status, body) = h.rollback(&f, campaign).await;
    assert_eq!(status, 201, "rollback: {body}");
    assert_eq!(body["from_version"], f.bad_version, "from the bad version");
    assert_eq!(body["queued"], 2, "the two devices on the bad image");

    let rollback_campaign: Uuid = body["campaign_id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid");
    let mut queued = h.jobs(f.vendor, rollback_campaign).await;
    queued.sort();
    let mut expected = f.on_bad.clone();
    expected.sort();
    assert_eq!(
        queued, expected,
        "exactly the devices that took the bad image"
    );

    // The rollback targets the older image, and no newer one.
    assert_ne!(
        body["to_version"], f.bad_version,
        "a rollback must not re-offer the image it is rolling back from"
    );

    // A second rollback of the same campaign is harmless and still narrow — it targets
    // the same devices, not the whole fleet.
    let (status, second) = h.rollback(&f, campaign).await;
    assert_eq!(status, 201, "a repeated rollback is allowed: {second}");
    assert_eq!(second["queued"], 2);
}

#[tokio::test]
async fn a_rollback_with_nothing_to_go_back_to_is_refused() {
    let Some(h) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let f = h.fixture().await;
    // A campaign for the *oldest* image: there is no earlier firmware than this one.
    let campaign = h.campaign(&f, f.good).await;
    let (status, body) = h.rollback(&f, campaign).await;

    assert_eq!(
        status, 422,
        "expected a validation error, got {status}: {body}"
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("no earlier firmware"),
        "the refusal must say why: {body}"
    );
}
