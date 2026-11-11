//! API keys: minted, used, revoked, and refused — end to end over HTTP.
//!
//! The property worth locking in is the one a customer cannot see and an auditor
//! will ask about: a revoked key stops working, immediately, and a vendor cannot
//! revoke someone else's.

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
        "haven-api-keys-test",
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    // Everything runs on the *test* database: seeding, the API under test, and the
    // assertions. Mixing in MIGRATE_DATABASE_URL looks convenient and silently
    // points half the test at a different database.
    let db = sqlx::PgPool::connect(
        &std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| db_url.clone()),
    )
    .await
    .ok()?;
    Some(Harness {
        base: format!("http://{addr}"),
        db,
    })
}

impl Harness {
    /// A vendor with an account that can log in.
    ///
    /// There is no vendor signup endpoint on purpose — vendors are onboarded —
    /// so the test makes the rows the onboarding would make. The vendor's own
    /// session uses the Sys role here only because the pool is the owner.
    async fn vendor(&self, name: &str) -> (Uuid, String) {
        let id = Uuid::now_v7();
        let email = format!("{name}-{id}@vendor.test");
        // The *tail* of a v7 uuid, not the head: the head is a timestamp, so two
        // runs in the same millisecond collide. Slugs are unique in the schema.
        let simple = id.simple().to_string();
        let slug = format!("{}-{}", name.replace(' ', "-"), &simple[simple.len() - 8..]);

        let hash = self.customer_hash().await;

        // `vendors` and `vendor_users` are RLS-scoped on haven_vendor(), so seed
        // them the way the application would: inside that vendor's scope. No
        // superuser, no bypassed policy.
        let mut conn = self.db.acquire().await.expect("connection");
        sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
            .bind(id.to_string())
            .execute(&mut *conn)
            .await
            .expect("scope");

        sqlx::query("INSERT INTO vendors (id, name, slug, created_at) VALUES ($1, $2, $3, now())")
            .bind(id)
            .bind(name)
            .bind(&slug)
            .execute(&mut *conn)
            .await
            .expect("vendor");

        sqlx::query(
            r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash, created_at)
               VALUES ($1, $2, $3, $4, 'owner', $5, now())"#,
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(&email)
        .bind(name)
        .bind(hash)
        .execute(&mut *conn)
        .await
        .expect("vendor user");

        // Leave the pooled connection as we found it.
        sqlx::query("SELECT set_config('haven.vendor_id', '', false)")
            .execute(&mut *conn)
            .await
            .expect("unscope");
        drop(conn);

        let token = self.login(&email, id).await;
        (id, token)
    }

    /// An argon2 hash for a password we know, produced by the same code path the
    /// app uses: register a throwaway account and borrow its hash.
    async fn customer_hash(&self) -> String {
        let email = format!("hash-{}@haven.dev", Uuid::now_v7());
        self.post(
            "/v1/auth/register",
            serde_json::json!({ "email": email, "password": "correct horse battery" }),
            None,
        )
        .await;

        let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&self.db)
            .await
            .expect("query")
            .unwrap_or_else(|| panic!("registered user {email} is not in the table"));
        hash
    }

    async fn login(&self, email: &str, vendor: Uuid) -> String {
        let body = self
            .post(
                "/v1/vendor/auth/login",
                serde_json::json!({ "email": email, "password": "correct horse battery", "vendor_id": vendor }),
                None,
            )
            .await;
        body["access"].as_str().expect("access token").to_string()
    }

    async fn post(
        &self,
        path: &str,
        body: serde_json::Value,
        token: Option<&str>,
    ) -> serde_json::Value {
        let mut req = reqwest::Client::new()
            .post(format!("{}{path}", self.base))
            .json(&body);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let res = req.send().await.expect("request");
        let status = res.status();
        let body = res.json().await.unwrap_or(serde_json::Value::Null);
        assert!(status.is_success(), "{path} -> {status}: {body}");
        body
    }

    async fn get(&self, path: &str, token: &str) -> reqwest::StatusCode {
        reqwest::Client::new()
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .expect("request")
            .status()
    }

    async fn delete(&self, path: &str, token: &str) -> reqwest::StatusCode {
        reqwest::Client::new()
            .delete(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .expect("request")
            .status()
    }
}

#[tokio::test]
async fn a_revoked_key_stops_working_and_a_key_is_never_shared() {
    let Some(h) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let (_vendor_a, token_a) = h.vendor("key vendor a").await;
    let (_vendor_b, token_b) = h.vendor("key vendor b").await;

    // A mints a key and uses it.
    let minted = h
        .post(
            "/v1/vendor/api-keys",
            serde_json::json!({ "name": "factory line" }),
            Some(&token_a),
        )
        .await;
    let secret = minted["secret"].as_str().expect("secret").to_string();
    let key_id = minted["id"].as_str().expect("id").to_string();

    assert_eq!(
        h.get("/v1/vendor/products", &secret).await,
        reqwest::StatusCode::OK,
        "a fresh key works"
    );

    // B cannot revoke A's key, and cannot even see it.
    assert_eq!(
        h.delete(&format!("/v1/vendor/api-keys/{key_id}"), &token_b)
            .await,
        reqwest::StatusCode::NOT_FOUND
    );
    assert_eq!(
        h.get("/v1/vendor/api-keys", &secret).await,
        reqwest::StatusCode::OK,
        "A's key still works after B's attempt"
    );

    // A revokes it. That is the whole point: the next call fails.
    assert_eq!(
        h.delete(&format!("/v1/vendor/api-keys/{key_id}"), &token_a)
            .await,
        reqwest::StatusCode::NO_CONTENT
    );
    assert_eq!(
        h.get("/v1/vendor/products", &secret).await,
        reqwest::StatusCode::UNAUTHORIZED,
        "a revoked key must not work"
    );

    // Revoked and never-existed are the same answer, so a key cannot be probed.
    assert_eq!(
        h.get(
            "/v1/vendor/products",
            "hv_deadbeef_0123456789abcdef0123456789abcdef"
        )
        .await,
        reqwest::StatusCode::UNAUTHORIZED
    );
}
