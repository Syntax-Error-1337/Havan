//! Sessions: what the account is signed in on, and getting rid of the ones you cannot see.
//!
//! The app could not answer either question. `refresh_tokens` has carried a `family_id` and a `revoked_at` since the
//! first migration — one family per sign-in — and nothing exposed it, so a customer who left a session open on a phone
//! they sold had no way to end it from anywhere they could still sign in.
//!
//! These tests drive the two routes the app calls, on a socket, and then use the tokens themselves: a session list that
//! lies is worse than no list at all.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        &format!("{}-{}", "haven-api-sessions-test", uuid::Uuid::now_v7()),
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

async fn sign_in(base: &str, email: &str, password: &str) -> serde_json::Value {
    reqwest::Client::new()
        .post(format!("{base}/v1/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("login")
        .json()
        .await
        .expect("tokens")
}

#[tokio::test]
async fn a_person_can_see_their_sessions_and_end_the_ones_they_cannot() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let email = format!("sessions-{}@haven.dev", uuid::Uuid::now_v7());
    let password = "sessions-Password-1";

    client
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("register");

    // Registering already signs you in, so the baseline is one session and the two logins below add one each.
    let baseline: serde_json::Value = client
        .get(format!("{base}/v1/me/sessions"))
        .bearer_auth(
            sign_in(&base, &email, password).await["access"]
                .as_str()
                .expect("access"),
        )
        .send()
        .await
        .expect("sessions")
        .json()
        .await
        .expect("json");
    let start = baseline["count"].as_u64().expect("count");

    // Two more sign-ins: the phone in the customer's pocket, and the one they left on a laptop.
    let phone = sign_in(&base, &email, password).await;
    let laptop = sign_in(&base, &email, password).await;
    let phone_access = phone["access"].as_str().expect("access").to_string();
    let phone_refresh = phone["refresh"].as_str().expect("refresh").to_string();
    let laptop_refresh = laptop["refresh"].as_str().expect("refresh").to_string();

    let listed: serde_json::Value = client
        .get(format!("{base}/v1/me/sessions"))
        .bearer_auth(&phone_access)
        .send()
        .await
        .expect("sessions")
        .json()
        .await
        .expect("json");
    assert_eq!(
        listed["count"].as_u64().expect("count"),
        start + 2,
        "one row per sign-in"
    );

    // Sign out the others, from the phone.
    let revoked: serde_json::Value = client
        .post(format!("{base}/v1/auth/logout-others"))
        .bearer_auth(&phone_access)
        .json(&serde_json::json!({ "refresh": phone_refresh }))
        .send()
        .await
        .expect("logout others")
        .json()
        .await
        .expect("json");
    // Everything except the phone: the laptop's login and the session registration created.
    assert_eq!(revoked["revoked"].as_u64().expect("revoked"), start + 1);

    let after: serde_json::Value = client
        .get(format!("{base}/v1/me/sessions"))
        .bearer_auth(&phone_access)
        .send()
        .await
        .expect("sessions")
        .json()
        .await
        .expect("json");
    assert_eq!(after["count"], 1, "only the phone that asked is left");

    // And the claim is true in the only way that matters: the revoked refresh token cannot be exchanged.
    let laptop_refresh_attempt = client
        .post(format!("{base}/v1/auth/refresh"))
        .json(&serde_json::json!({ "refresh": laptop_refresh }))
        .send()
        .await
        .expect("refresh laptop");
    assert_eq!(
        laptop_refresh_attempt.status().as_u16(),
        401,
        "a revoked session cannot refresh"
    );

    let phone_refresh_attempt = client
        .post(format!("{base}/v1/auth/refresh"))
        .json(&serde_json::json!({ "refresh": phone_refresh }))
        .send()
        .await
        .expect("refresh phone");
    assert_eq!(
        phone_refresh_attempt.status().as_u16(),
        200,
        "the session that asked is untouched"
    );
}

#[tokio::test]
async fn a_broken_refresh_token_does_not_sign_anybody_out() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let email = format!("sessions-bad-{}@haven.dev", uuid::Uuid::now_v7());
    let password = "sessions-Password-2";

    client
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .expect("register");

    let phone = sign_in(&base, &email, password).await;
    let access = phone["access"].as_str().expect("access").to_string();

    // Registration and the login above are two sessions; the number matters only as a before/after.
    let before: serde_json::Value = client
        .get(format!("{base}/v1/me/sessions"))
        .bearer_auth(&access)
        .send()
        .await
        .expect("sessions")
        .json()
        .await
        .expect("json");
    let count = before["count"].as_u64().expect("count");

    // A malformed token must revoke nothing: a typo is not an instruction to end sessions the customer is still using.
    let revoked: serde_json::Value = client
        .post(format!("{base}/v1/auth/logout-others"))
        .bearer_auth(&access)
        .json(&serde_json::json!({ "refresh": "not-a-token" }))
        .send()
        .await
        .expect("logout others")
        .json()
        .await
        .expect("json");
    assert_eq!(revoked["revoked"], 0);

    let after: serde_json::Value = client
        .get(format!("{base}/v1/me/sessions"))
        .bearer_auth(&access)
        .send()
        .await
        .expect("sessions")
        .json()
        .await
        .expect("json");
    assert_eq!(after["count"].as_u64().expect("count"), count);
}

#[tokio::test]
async fn changing_the_password_needs_the_old_one_and_ends_the_other_sessions() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let email = format!("password-{}@haven.dev", uuid::Uuid::now_v7());
    let old = "password-Old-1";

    client
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": old }))
        .send()
        .await
        .expect("register");

    // Two sessions: the phone and a laptop somebody else may be holding.
    let phone = sign_in(&base, &email, old).await;
    let laptop = sign_in(&base, &email, old).await;
    let phone_access = phone["access"].as_str().expect("access").to_string();
    let phone_refresh = phone["refresh"].as_str().expect("refresh").to_string();
    let laptop_refresh = laptop["refresh"].as_str().expect("refresh").to_string();

    // The old password is the proof, and a wrong one changes nothing.
    let wrong = client
        .post(format!("{base}/v1/auth/password"))
        .bearer_auth(&phone_access)
        .json(&serde_json::json!({ "current": "not-the-password", "next": "password-New-1" }))
        .send()
        .await
        .expect("wrong");
    assert_eq!(wrong.status().as_u16(), 401);

    // The new password is the current one is refused before anything is written.
    let same = client
        .post(format!("{base}/v1/auth/password"))
        .bearer_auth(&phone_access)
        .json(&serde_json::json!({ "current": old, "next": old }))
        .send()
        .await
        .expect("same");
    assert_eq!(same.status().as_u16(), 422);

    let changed: serde_json::Value = client
        .post(format!("{base}/v1/auth/password"))
        .bearer_auth(&phone_access)
        .json(&serde_json::json!({ "current": old, "next": "password-New-1", "refresh": phone_refresh }))
        .send()
        .await
        .expect("change")
        .json()
        .await
        .expect("json");
    // The laptop's session, and the one registration created.
    assert_eq!(changed["signed_out"], 2);

    // The laptop cannot refresh; the phone can.
    let laptop_try = client
        .post(format!("{base}/v1/auth/refresh"))
        .json(&serde_json::json!({ "refresh": laptop_refresh }))
        .send()
        .await
        .expect("refresh laptop");
    assert_eq!(laptop_try.status().as_u16(), 401);
    let phone_try = client
        .post(format!("{base}/v1/auth/refresh"))
        .json(&serde_json::json!({ "refresh": phone["refresh"].as_str().expect("r") }))
        .send()
        .await
        .expect("refresh phone");
    assert_eq!(
        phone_try.status().as_u16(),
        200,
        "the device that changed it stays signed in"
    );

    // And the sign-in itself moved: old fails, new works.
    let old_login = client
        .post(format!("{base}/v1/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": old }))
        .send()
        .await
        .expect("login old");
    assert_eq!(old_login.status().as_u16(), 401);
    let new_login = client
        .post(format!("{base}/v1/auth/login"))
        .json(&serde_json::json!({ "email": email, "password": "password-New-1" }))
        .send()
        .await
        .expect("login new");
    assert_eq!(new_login.status().as_u16(), 200);
}
