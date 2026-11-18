//! The audit trail: written by the API, readable only by the vendor it belongs to.
//!
//! The property worth locking in is the one that makes an audit trail evidence: a vendor
//! can read what happened in its own account and cannot read, or write into, another's.

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
        "haven-api-audit-test",
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });

    let db = sqlx::PgPool::connect(&db_url).await.ok()?;
    Some(Harness {
        base: format!("http://{addr}"),
        db,
    })
}

impl Harness {
    /// A vendor with a user that can log in. There is no vendor signup endpoint on
    /// purpose — vendors are onboarded — so the test writes the rows onboarding would.
    async fn vendor(&self, name: &str) -> (Uuid, String, String) {
        let id = Uuid::now_v7();
        let email = format!("{name}-{id}@vendor.test");
        let simple = id.simple().to_string();
        let slug = format!("{name}-{}", &simple[simple.len() - 8..]);
        let password = "audit-Password-1".to_string();

        let hash = domain::identity::hash_password(&password).expect("hash");

        let mut conn = self.db.acquire().await.expect("connection");
        sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
            .bind(id.to_string())
            .execute(&mut *conn)
            .await
            .expect("scope");
        sqlx::query("INSERT INTO vendors (id, name, slug, created_at) VALUES ($1,$2,$3,now())")
            .bind(id)
            .bind(name)
            .bind(&slug)
            .execute(&mut *conn)
            .await
            .expect("vendor");
        sqlx::query(
            r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash, created_at)
               VALUES ($1,$2,$3,$4,'owner',$5,now())"#,
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(&email)
        .bind(name)
        .bind(hash)
        .execute(&mut *conn)
        .await
        .expect("vendor user");
        sqlx::query("SELECT set_config('haven.vendor_id', '', false)")
            .execute(&mut *conn)
            .await
            .expect("unscope");

        let token: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/v1/vendor/auth/login", self.base))
            .json(&serde_json::json!({
                "vendor_id": id, "email": email, "password": password
            }))
            .send()
            .await
            .expect("login")
            .json()
            .await
            .expect("tokens");

        let access = token["access"].as_str().expect("access").to_string();
        (id, email, access)
    }

    async fn create_key(&self, token: &str, name: &str) -> Uuid {
        let key: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/v1/vendor/api-keys", self.base))
            .bearer_auth(token)
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await
            .expect("create key")
            .json()
            .await
            .expect("key");
        key["id"].as_str().expect("id").parse().expect("uuid")
    }

    /// How many audit rows one vendor can see, counted the way the application would.
    async fn rows_for(&self, vendor: Uuid) -> i64 {
        let mut conn = self.db.acquire().await.expect("connection");
        sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
            .bind(vendor.to_string())
            .execute(&mut *conn)
            .await
            .expect("scope");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log")
            .fetch_one(&mut *conn)
            .await
            .expect("count");
        sqlx::query("SELECT set_config('haven.vendor_id', '', false)")
            .execute(&mut *conn)
            .await
            .expect("unscope");
        count
    }

    async fn audit(&self, token: &str) -> serde_json::Value {
        reqwest::Client::new()
            .get(format!("{}/v1/vendor/audit?limit=50", self.base))
            .bearer_auth(token)
            .send()
            .await
            .expect("audit")
            .json()
            .await
            .expect("audit body")
    }
}

#[tokio::test]
async fn an_action_is_recorded_once_and_only_for_its_own_vendor() {
    let Some(h) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let (acme, _, acme_token) = h.vendor("audit-acme").await;
    let (globex, _, globex_token) = h.vendor("audit-globex").await;

    let key = h.create_key(&acme_token, "factory line 2").await;
    let revoke = reqwest::Client::new()
        .delete(format!("{}/v1/vendor/api-keys/{key}", h.base))
        .bearer_auth(&acme_token)
        .send()
        .await
        .expect("revoke");
    assert_eq!(revoke.status().as_u16(), 204);

    let mine = h.audit(&acme_token).await;
    let actions: Vec<&str> = mine["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e["action"].as_str())
        .collect();

    // Newest first, and both ends of the action are there.
    assert_eq!(actions.first(), Some(&"api_key.revoked"), "newest first");
    assert!(
        actions.contains(&"api_key.created"),
        "the create is there too"
    );

    let revoked = mine["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|e| e["action"] == "api_key.revoked")
        .expect("revoked entry");
    assert_eq!(
        revoked["target_id"].as_str(),
        Some(key.to_string().as_str())
    );
    assert!(
        revoked["actor_id"].is_string(),
        "a person, not an anonymous action"
    );
    assert_eq!(revoked["actor_kind"], "user");
    // The loopback address is recorded: the trail answers "from where", not just "who".
    assert_eq!(revoked["ip"], "127.0.0.1");

    // And the other vendor sees nothing, even though its token is equally valid.
    let theirs = h.audit(&globex_token).await;
    assert_eq!(
        theirs["entries"].as_array().map(Vec::len),
        Some(0),
        "one vendor's trail must not be visible to another: {theirs}"
    );

    // Belt and braces: the rows really do belong to different vendors. Counted inside
    // each vendor's scope, because counting on a bare pool returns zero for everyone —
    // row-level security fails closed and an unscoped count is a convincing lie.
    assert!(h.rows_for(acme).await >= 2, "the acting vendor has entries");
    assert_eq!(h.rows_for(globex).await, 0, "the other vendor has none");
}

/// The export is a file an auditor can attach to a report, so the properties that matter are the ones a spreadsheet
/// depends on: a header row, one line per row, and a detail blob that cannot shift a column.
#[tokio::test]
async fn the_export_is_a_file_a_spreadsheet_can_open() {
    let Some(h) = boot().await else {
        eprintln!("skip: database or redis unavailable");
        return;
    };
    let (vendor, email, token) = h.vendor("export").await;

    // A key name carrying a comma and a quote — the two characters that break naive CSV.
    h.create_key(&token, "shipped, \"v2\"").await;

    let res = reqwest::Client::new()
        .get(format!("{}/v1/vendor/audit.csv", h.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("export");

    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/csv; charset=utf-8")
    );
    assert_eq!(
        res.headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok()),
        Some("attachment; filename=\"haven-audit.csv\"")
    );

    let body = res.text().await.expect("body");
    let mut lines = body.lines();
    assert_eq!(
        lines.next(),
        Some("when,actor_kind,actor,action,target_kind,target_id,ip,request_id,detail")
    );

    let rows: Vec<&str> = lines.collect();
    assert!(
        !rows.is_empty(),
        "the vendor wrote a key, so there is a row"
    );
    // Every line has the same number of fields once quoting is respected — the property that keeps columns aligned.
    let fields = |line: &str| -> usize {
        let mut count = 1;
        let mut quoted = false;
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' if quoted && matches!(chars.clone().next(), Some('"')) => {
                    chars.next();
                }
                '"' => quoted = !quoted,
                ',' if !quoted => count += 1,
                _ => {}
            }
        }
        assert!(!quoted, "unbalanced quotes in {line}");
        count
    };
    for row in &rows {
        assert_eq!(fields(row), 9, "row has the wrong column count: {row}");
    }
    assert!(
        body.contains(&email),
        "the export names the actor, so the file answers who"
    );
    assert!(
        body.contains("shipped, "),
        "the comma survived inside quotes"
    );

    // Scoped: another vendor's export is their own rows, not this one's.
    let (other, _, other_token) = h.vendor("export-other").await;
    let other_body = reqwest::Client::new()
        .get(format!("{}/v1/vendor/audit.csv", h.base))
        .bearer_auth(&other_token)
        .send()
        .await
        .expect("export")
        .text()
        .await
        .expect("body");
    assert!(!other_body.contains(&email));
    let _ = (vendor, other);
}
