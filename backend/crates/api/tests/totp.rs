//! A second factor for the person who owns the devices.
//!
//! The portal has had TOTP since the vendor work; the account in the customer's pocket had a password and nothing else.
//! These tests drive it end to end on a socket: enrol, prove the authenticator, sign in without a code and be told which
//! factor is missing, sign in with one, and check that the same code cannot be used twice.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-totp-test",
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

#[tokio::test]
async fn the_second_factor_is_proven_before_it_gates_a_sign_in() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let email = format!("totp-{}@haven.dev", uuid::Uuid::now_v7());
    let password = "totp-Password-1";

    let tokens: serde_json::Value = client
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("register")
        .json()
        .await
        .expect("tokens");
    let access = tokens["access"].as_str().expect("access").to_string();

    let off: serde_json::Value = client
        .get(format!("{base}/v1/me/totp"))
        .bearer_auth(&access)
        .send()
        .await
        .expect("state")
        .json()
        .await
        .expect("json");
    assert_eq!(off["state"], "off");

    // Enrolling stores a seed but does not gate anything yet: a second factor nobody has proved is a way to lock
    // yourself out of your own house.
    let started_raw = client
        .post(format!("{base}/v1/me/totp"))
        .bearer_auth(&access)
        .send()
        .await
        .expect("start");
    let status = started_raw.status().as_u16();
    let started_text = started_raw.text().await.expect("body");
    assert_eq!(status, 200, "enrolment answered {status}: {started_text}");
    let started: serde_json::Value = serde_json::from_str(&started_text).expect("json");
    let secret = started["secret"].as_str().expect("secret").to_string();
    assert!(
        started["uri"]
            .as_str()
            .is_some_and(|u| u.starts_with("otpauth://totp/")),
        "an authenticator needs a URI it can scan: {started}"
    );

    let pending: serde_json::Value = client
        .get(format!("{base}/v1/me/totp"))
        .bearer_auth(&access)
        .send()
        .await
        .expect("state")
        .json()
        .await
        .expect("json");
    assert_eq!(pending["state"], "pending");

    // A wrong code does not confirm it.
    let wrong = client
        .post(format!("{base}/v1/me/totp/confirm"))
        .bearer_auth(&access)
        .json(&serde_json::json!({ "code": "000000" }))
        .send()
        .await
        .expect("confirm wrong");
    assert_eq!(wrong.status().as_u16(), 401);

    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let code = domain::totp::current(&secret, now).expect("code");
    let confirmed = client
        .post(format!("{base}/v1/me/totp/confirm"))
        .bearer_auth(&access)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await
        .expect("confirm");
    assert_eq!(confirmed.status().as_u16(), 200);

    // Now the password alone is not enough, and the refusal says which half is missing rather than "wrong password".
    let no_code = client
        .post(format!("{base}/v1/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login");
    assert_eq!(no_code.status().as_u16(), 401);
    let body: serde_json::Value = no_code.json().await.expect("json");
    assert_eq!(body["error"]["code"], "second_factor_required", "{body}");

    // The code that confirmed the enrolment has been spent; a replay of it must not sign anybody in.
    let replay = client
        .post(format!("{base}/v1/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password, "code": code }))
        .send()
        .await
        .expect("replay");
    assert_eq!(replay.status().as_u16(), 401, "a used code is not a key");

    // The next step's code works.
    let next = domain::totp::current(&secret, now + 30).expect("code");
    let signed_in = client
        .post(format!("{base}/v1/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password, "code": next }))
        .send()
        .await
        .expect("login with code");
    assert_eq!(signed_in.status().as_u16(), 200);

    // Turning it off restores the single factor, and the row goes with it.
    let off_again = client
        .delete(format!("{base}/v1/me/totp"))
        .bearer_auth(&access)
        .send()
        .await
        .expect("disable");
    assert_eq!(off_again.status().as_u16(), 204);

    let back_to_password = client
        .post(format!("{base}/v1/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login");
    assert_eq!(back_to_password.status().as_u16(), 200);
}
