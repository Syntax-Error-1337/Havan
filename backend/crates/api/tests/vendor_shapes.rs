//! What a vendor may never see: our customers.
//!
//! In a one-app business the homes, the users and the addresses belong to the platform. A
//! vendor's legitimate view is its own devices — serial, product, uptime, firmware. The risk
//! is never a deliberate leak; it is a *join* added later that pulls `homes.name` or
//! `users.email` into a vendor response because it was convenient at the time. No code review
//! reliably catches that.
//!
//! So this test writes a home whose name, whose owner's email and whose id are all
//! unmistakable strings, then calls every vendor endpoint and fails if any of them appears
//! anywhere in the response — including nested, including inside an array.

use api::{AppState, router};
use uuid::Uuid;

// Kept unique per run: a fixed email collides with the row a failed run left behind, and a
// guard test that only passes on a clean database is a guard test that cries wolf.
fn sentinel_home() -> String {
    format!("SECRET-HOME-NAME-{}", &Uuid::now_v7().to_string()[20..32])
}

fn sentinel_email() -> String {
    format!(
        "owner-secret-{}@example.test",
        &Uuid::now_v7().to_string()[20..32]
    )
}

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
        "haven-api-shapes-test",
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
    /// A vendor with a working login, plus one home, one user and one device of theirs.
    async fn vendor_with_a_customer(&self) -> (String, Uuid, String, String) {
        let home_name = sentinel_home();
        let owner_email = sentinel_email();
        let vendor_id = Uuid::now_v7();
        let email = format!("shapes-{vendor_id}@vendor.test");
        let password = "shapes-Password-1".to_string();
        let home_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let product_id = Uuid::now_v7();

        let hash = domain::identity::hash_password(&password).expect("hash");
        let mut conn = self.db.acquire().await.expect("connection");

        // The API's own role cannot write across tenants, so the fixture wears the system
        // role for the setup — the same one the claim path uses.
        sqlx::query("SET ROLE haven_system")
            .execute(&mut *conn)
            .await
            .expect("system role");

        let simple = vendor_id.simple().to_string();
        sqlx::query("INSERT INTO vendors (id, name, slug) VALUES ($1, 'Shapes Vendor', $2)")
            .bind(vendor_id)
            .bind(format!("shapes-{}", &simple[simple.len() - 8..]))
            .execute(&mut *conn)
            .await
            .expect("vendor");
        sqlx::query(
            r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash)
               VALUES ($1, $2, $3, 'Shapes', 'owner', $4)"#,
        )
        .bind(Uuid::now_v7())
        .bind(vendor_id)
        .bind(&email)
        .bind(&hash)
        .execute(&mut *conn)
        .await
        .expect("vendor user");

        // The customer side. Every field here is a sentinel: if it shows up in a vendor
        // response, the response found it through a join.
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(&owner_email)
            .execute(&mut *conn)
            .await
            .expect("user");
        sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, $3)")
            .bind(home_id)
            .bind(user_id)
            .bind(&home_name)
            .execute(&mut *conn)
            .await
            .expect("home");

        let model = serde_json::json!({
            "product_key": format!("hv_shapes_{}", &product_id.to_string()[20..32]),
            "name": "Shapes Bulb",
            "category": "light",
            "capabilities": [{ "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } }],
            "constraints": { "allow_unknown_serial": false }
        });
        sqlx::query(
            r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status, certification)
               VALUES ($1, $2, $3, 'Shapes Bulb', 'light', $4, 'published', 'certified')"#,
        )
        .bind(product_id)
        .bind(vendor_id)
        .bind(format!("hv_shapes_{}", &product_id.to_string()[20..32]))
        .bind(&model)
        .execute(&mut *conn)
        .await
        .expect("product");

        sqlx::query(
            r#"INSERT INTO devices
                 (id, vendor_id, product_id, product_key, category, home_id, name, serial,
                  secret_hash, state, claimed_at)
               VALUES ($1, $2, $3, $4, 'light', $5, 'Kitchen bulb', $6, 'x', 'claimed', now())"#,
        )
        .bind(Uuid::now_v7())
        .bind(vendor_id)
        .bind(product_id)
        .bind(format!("hv_shapes_{}", &product_id.to_string()[20..32]))
        .bind(home_id)
        .bind(format!("SN-{}", &product_id.to_string()[20..32]))
        .execute(&mut *conn)
        .await
        .expect("device");

        let token: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/v1/vendor/auth/login", self.base))
            .json(&serde_json::json!({ "vendor_id": vendor_id, "email": email, "password": password }))
            .send()
            .await
            .expect("login")
            .json()
            .await
            .expect("tokens");

        let access = token["access"].as_str().expect("access").to_string();
        (access, home_id, home_name, owner_email)
    }
}

/// The bundle has to be *useful*, not merely safe: a support engineer opens it to find out what the
/// device is and whether it has been commanded.
#[tokio::test]
async fn the_support_bundle_says_what_the_device_is() {
    let Some(h) = boot().await else { return };
    let (token, _home, _home_name, _owner_email) = h.vendor_with_a_customer().await;

    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/v1/vendor/support-bundle", h.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("bundle")
        .json()
        .await
        .expect("json");

    assert_eq!(body["format_version"], 1);
    assert_eq!(body["vendor"]["name"], "Shapes Vendor");
    assert!(
        body["devices"][0]["serial"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    assert_eq!(body["devices"][0]["bound"], serde_json::json!(true));
    assert!(body["licences"]["granted"].is_number());
    assert!(
        body["notes"].as_array().is_some_and(|n| !n.is_empty()),
        "a bundle explains what it leaves out"
    );
}

/// Every `/vendor/...` route the API declares, read out of the source rather than from a list
/// somebody remembers to update.
///
/// A hand-written list is the weakness this guard exists to cover: it passes for as long as nobody
/// adds an endpoint, and the endpoint that leaks is exactly the one added after the list was
/// written. Reading the routers means a new vendor route fails the test until somebody decides how
/// it is checked.
fn declared_vendor_routes() -> Vec<String> {
    let routes_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");
    let mut found = Vec::new();

    for entry in std::fs::read_dir(&routes_dir).expect("routes dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("route source");

        // `.route("/vendor/thing", get(handler))` — the literal is all this needs, and it may sit on
        // the next line: rustfmt splits the call as soon as the handler list gets long, and a
        // line-based scan then reports a route that exists as missing. That failure looks like a
        // stale entry, which is the opposite of the truth.
        let mut cursor = source.as_str();
        while let Some(at) = cursor.find(".route(") {
            cursor = &cursor[at + ".route(".len()..];
            let rest = cursor.trim_start();
            let Some(rest) = rest.strip_prefix('"') else {
                continue;
            };
            let Some(end) = rest.find('"') else { continue };
            let route = &rest[..end];
            if route.starts_with("/vendor") {
                found.push(route.to_string());
            }
            cursor = &rest[end..];
        }
    }

    found.sort();
    found.dedup();
    found
}

/// Routes that cannot be exercised as a plain GET, with the reason. A new POST or DELETE under
/// `/vendor` has to be added here deliberately — that decision is the point of the guard.
const NOT_FETCHABLE: &[(&str, &str)] = &[
    (
        "/vendor/auth/login",
        "POST: needs a body and returns credentials",
    ),
    ("/vendor/api-keys/{id}", "DELETE revokes a key"),
    // Both were invisible to this guard until its scanner learned that rustfmt splits `.route(`
    // across lines. They are GETs, but parameterised, and this guard calls literal paths — so they
    // are explained rather than fetched, on purpose, not by omission.
    (
        "/vendor/products/{id}/serials",
        "GET needs a product id, which the guard does not mint",
    ),
    (
        "/vendor/products/{product_id}/firmware",
        "GET needs a product id, which the guard does not mint",
    ),
    (
        "/vendor/campaigns/{id}",
        "GET needs a campaign id, which does not exist yet",
    ),
    ("/vendor/campaigns/{id}/halt", "POST halts a rollout"),
    ("/vendor/campaigns/{id}/rollback", "POST starts a rollback"),
    ("/vendor/campaigns/{id}/start", "POST starts a rollout"),
    (
        "/vendor/invites/accept",
        "POST: the public accept door, no vendor session",
    ),
    ("/vendor/products", "POST creates; GET is fetched"),
    (
        "/vendor/products/{id}",
        "PATCH and DELETE on the same path as a GET",
    ),
    ("/vendor/products/{id}/publish", "POST: a state change"),
    (
        "/vendor/products/{id}/validate",
        "POST: validates a thing model",
    ),
    (
        "/vendor/products/{id}/certification/submit",
        "POST: asks the operator to test it",
    ),
    ("/vendor/team", "POST invites; GET is fetched"),
    ("/vendor/team/members/{id}", "DELETE removes a member"),
    (
        "/vendor/team/{id}/sso",
        "POST binds an identity to a member",
    ),
    ("/vendor/webhooks", "POST subscribes; GET is fetched"),
    ("/vendor/webhooks/{id}", "DELETE unsubscribes"),
    (
        "/vendor/security",
        "PATCH changes the policy; GET is fetched",
    ),
    (
        "/vendor/invoices/{period}",
        "GET with a period path parameter, which this guard does not know",
    ),
    ("/vendor/totp/enrol", "POST: issues a new secret"),
    ("/vendor/totp/confirm", "POST: proves the secret works"),
    ("/vendor/totp", "DELETE disables the member's factor"),
    (
        "/vendor/security",
        "PATCH changes the policy; GET is fetched",
    ),
];

/// The GETs the guard actually calls.
const FETCHED: &[&str] = &[
    "/vendor/devices",
    "/vendor/analytics",
    "/vendor/audit",
    "/vendor/licences",
    "/vendor/products",
    "/vendor/team",
    "/vendor/sso",
    "/vendor/webhooks",
    "/vendor/api-keys",
    "/vendor/campaigns",
    "/vendor/security",
    "/vendor/invoices",
    // The bundle is the artefact that leaves the building — pasted into a ticket — so it is
    // exactly the response that must never carry a customer.
    "/vendor/support-bundle",
];

#[test]
fn every_vendor_route_is_checked_or_explained() {
    let declared = declared_vendor_routes();
    assert!(
        declared.len() >= 10,
        "the scan found only {} vendor routes; it is looking in the wrong place",
        declared.len()
    );

    let fetched: Vec<String> = FETCHED.iter().map(|p| p.to_string()).collect();
    let explained: Vec<String> = NOT_FETCHABLE.iter().map(|(p, _)| p.to_string()).collect();

    for route in &declared {
        let covered = fetched.contains(route) || explained.contains(route);
        assert!(
            covered,
            "{route} is declared under /vendor but nothing checks it: add it to FETCHED if a GET \
             can be called, or to NOT_FETCHABLE with the reason it cannot"
        );
    }

    // The other direction: a stale entry hides a route that was renamed and is now unchecked.
    for route in fetched.iter().chain(explained.iter()) {
        assert!(
            declared.contains(route),
            "{route} is listed as a vendor route, but no router declares it any more"
        );
    }
}

#[tokio::test]
async fn no_vendor_response_carries_a_customer() {
    let Some(h) = boot().await else { return };
    let (token, home_id, home_name, owner_email) = h.vendor_with_a_customer().await;

    let client = reqwest::Client::new();
    let endpoints: Vec<String> = FETCHED.iter().map(|p| format!("/v1{p}")).collect();

    for path in endpoints {
        let response = client
            .get(format!("{}{path}", h.base))
            .bearer_auth(&token)
            .send()
            .await
            .expect("request");

        let status = response.status();
        let body = response.text().await.expect("body");

        assert!(
            status.is_success(),
            "{path} answered {status}: {}",
            &body[..body.len().min(200)]
        );

        // A vendor asking about its own fleet is exactly the request that would carry the
        // home if anything joined it in.
        for forbidden in [
            home_name.as_str(),
            owner_email.as_str(),
            &home_id.to_string(),
        ] {
            assert!(
                !body.contains(forbidden),
                "{path} leaked a customer: found {forbidden:?} in {}",
                &body[..body.len().min(400)]
            );
        }
        for key in [
            "home_name",
            "owner_email",
            "owner_id",
            "latitude",
            "longitude",
        ] {
            assert!(
                !body.contains(&format!("\"{key}\"")),
                "{path} has a {key} field in a vendor response"
            );
        }
    }
}
