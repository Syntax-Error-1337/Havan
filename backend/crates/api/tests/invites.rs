//! Inviting somebody who does not have an account yet.
//!
//! `POST /homes/{id}/members` used to be an add-existing-account-or-404: the person you actually want in a family home —
//! the one who has never installed the app — could not be added at all. These tests drive the whole path on a socket:
//! invite an unknown address, sign that address up, accept the token, and then check the home's member list and that the
//! token cannot be used twice or by somebody else.

use api::{AppState, router};
use uuid::Uuid;

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        &format!("{}-{}", "haven-api-invites-test", uuid::Uuid::now_v7()),
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

async fn register(base: &str, email: &str) -> String {
    reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "invites-Password-1" }))
        .send()
        .await
        .expect("register")
        .json::<serde_json::Value>()
        .await
        .expect("tokens")["access"]
        .as_str()
        .expect("access")
        .to_string()
}

async fn home(base: &str, token: &str) -> Uuid {
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/homes"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "name": "Family flat", "timezone": "UTC" }))
        .send()
        .await
        .expect("home")
        .json()
        .await
        .expect("home json");
    body["id"].as_str().expect("id").parse().expect("uuid")
}

#[tokio::test]
async fn an_invitation_joins_the_house_after_the_account_exists() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let owner = register(&base, &format!("owner-{}@haven.dev", Uuid::now_v7())).await;
    let home_id = home(&base, &owner).await;
    let guest_email = format!("guest-{}@haven.dev", Uuid::now_v7());

    // The address has no account yet: the answer is an invitation with a token, not a 404.
    let invited: serde_json::Value = client
        .post(format!("{base}/v1/homes/{home_id}/members"))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "email": guest_email, "role": "member" }))
        .send()
        .await
        .expect("invite")
        .json()
        .await
        .expect("json");

    assert_eq!(invited["outcome"], "invited", "body: {invited}");
    let token = invited["token"].as_str().expect("token").to_string();

    // The admin can see it pending, and the token itself is never in that list.
    let pending: serde_json::Value = client
        .get(format!("{base}/v1/homes/{home_id}/invitations"))
        .bearer_auth(&owner)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    let rows = pending.as_array().expect("array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["email"], guest_email);
    assert!(
        rows[0].get("token_hash").is_none(),
        "the hash must never leave the service"
    );

    // Now the person signs up — after the invitation, which is the whole point. Registering proves the address, so the
    // invitation is accepted on the spot: no link to find, and the address is proven better than a token proves it.
    let guest = register(&base, &guest_email).await;

    let members: serde_json::Value = client
        .get(format!("{base}/v1/homes/{home_id}/members"))
        .bearer_auth(&owner)
        .send()
        .await
        .expect("members")
        .json()
        .await
        .expect("json");
    assert_eq!(members.as_array().map(|m| m.len()), Some(2));

    // One invitation, one join: the token was spent by the registration, so presenting it afterwards is refused
    // rather than adding them twice.
    let again = client
        .post(format!("{base}/v1/invitations/accept"))
        .bearer_auth(&guest)
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .expect("accept again");
    let status = again.status().as_u16();
    let body = again.text().await.expect("body");
    // `validation_failed` answers 422: the request was well-formed and the state was wrong, which is the distinction the
    // error codes exist to make.
    assert_eq!(status, 422, "body: {body}");
}

#[tokio::test]
async fn an_invitation_is_bound_to_the_address_it_was_sent_to() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let owner = register(&base, &format!("owner2-{}@haven.dev", Uuid::now_v7())).await;
    let home_id = home(&base, &owner).await;
    let guest_email = format!("guest2-{}@haven.dev", Uuid::now_v7());

    let invited: serde_json::Value = client
        .post(format!("{base}/v1/homes/{home_id}/members"))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "email": guest_email, "role": "guest" }))
        .send()
        .await
        .expect("invite")
        .json()
        .await
        .expect("json");
    let token = invited["token"].as_str().expect("token").to_string();

    // A forwarded link is not a key: the token names an address, and the account that presents it must be that address.
    let stranger = register(&base, &format!("stranger-{}@haven.dev", Uuid::now_v7())).await;
    let refused = client
        .post(format!("{base}/v1/invitations/accept"))
        .bearer_auth(&stranger)
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .expect("accept");
    assert_eq!(refused.status().as_u16(), 403);

    // And the invitation is still there for the person it was addressed to: registering joins them, no link needed.
    let guest = register(&base, &guest_email).await;
    let homes: serde_json::Value = client
        .get(format!("{base}/v1/homes"))
        .bearer_auth(&guest)
        .send()
        .await
        .expect("homes")
        .json()
        .await
        .expect("json");
    assert!(
        homes
            .as_array()
            .expect("array")
            .iter()
            .any(|h| h["id"] == home_id.to_string()),
        "the stranger's refusal did not spend the invitation"
    );
}

#[tokio::test]
async fn signing_up_joins_the_homes_that_invited_the_address() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let owner = register(&base, &format!("owner3-{}@haven.dev", Uuid::now_v7())).await;
    let home_id = home(&base, &owner).await;
    let guest_email = format!("invited-{}@haven.dev", Uuid::now_v7());

    // Invited before the account exists - the whole reason invitations exist.
    let _invited: serde_json::Value = client
        .post(format!("{base}/v1/homes/{home_id}/members"))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "email": guest_email, "role": "member" }))
        .send()
        .await
        .expect("invite")
        .json()
        .await
        .expect("json");

    // They sign up. No token, no link: registering proves the address better than any link does.
    let guest: serde_json::Value = client
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": guest_email, "password": "invites-Password-2" }))
        .send()
        .await
        .expect("register")
        .json()
        .await
        .expect("tokens");
    let guest_token = guest["access"].as_str().expect("access");

    let homes: serde_json::Value = client
        .get(format!("{base}/v1/homes"))
        .bearer_auth(guest_token)
        .send()
        .await
        .expect("homes")
        .json()
        .await
        .expect("json");
    assert!(
        homes
            .as_array()
            .expect("array")
            .iter()
            .any(|h| h["id"] == home_id.to_string()),
        "the new account is in the home it was invited to: {homes}"
    );

    // And the invitation is spent, so the old link cannot add somebody else later.
    let pending: serde_json::Value = client
        .get(format!("{base}/v1/homes/{home_id}/invitations"))
        .bearer_auth(&owner)
        .send()
        .await
        .expect("invitations")
        .json()
        .await
        .expect("json");
    assert_eq!(pending.as_array().map(|p| p.len()), Some(0));
}

/// Tenancy on the invitation list itself.
///
/// The token is the interesting secret in this flow, and the routes that reveal it are the admin ones. Two properties,
/// both of which are one missing `WHERE home_id` away from being false: a member of another house cannot read the list,
/// and cannot withdraw an invitation out of it. The second matters more — a stray `DELETE` that answered 204 while
/// accepting any id would let any signed-in person cancel anybody's invitation if they had seen the id once.
#[tokio::test]
async fn another_house_cannot_read_or_withdraw_an_invitation() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();

    let owner = register(&base, &format!("owner4-{}@haven.dev", Uuid::now_v7())).await;
    let home_id = home(&base, &owner).await;
    let invited: serde_json::Value = client
        .post(format!("{base}/v1/homes/{home_id}/members"))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "email": format!("guest4-{}@haven.dev", Uuid::now_v7()), "role": "member" }))
        .send()
        .await
        .expect("invite")
        .json()
        .await
        .expect("json");
    assert_eq!(invited["outcome"], "invited");

    // The route answers a bare array of invitations, not an envelope.
    let pending: serde_json::Value = client
        .get(format!("{base}/v1/homes/{home_id}/invitations"))
        .bearer_auth(&owner)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    let invitation_id = pending[0]["id"].as_str().expect("id").to_string();

    // Somebody from a different house, with a perfectly valid session.
    let stranger = register(&base, &format!("stranger4-{}@haven.dev", Uuid::now_v7())).await;
    let home_of_their_own = home(&base, &stranger).await;
    assert_ne!(home_of_their_own, home_id);

    let peek = client
        .get(format!("{base}/v1/homes/{home_id}/invitations"))
        .bearer_auth(&stranger)
        .send()
        .await
        .expect("peek");
    // 404 rather than 403: the refusal should not confirm that the house exists to somebody outside it.
    assert!(
        matches!(peek.status().as_u16(), 403 | 404),
        "another house's invitations are not readable, got {}",
        peek.status()
    );

    let withdraw = client
        .delete(format!(
            "{base}/v1/homes/{home_id}/invitations/{invitation_id}"
        ))
        .bearer_auth(&stranger)
        .send()
        .await
        .expect("withdraw");
    assert!(
        matches!(withdraw.status().as_u16(), 403 | 404),
        "expected a refusal, got {}",
        withdraw.status()
    );

    // And it is still there for the house it belongs to — the refusal was a decision, not a side effect.
    let still_pending: serde_json::Value = client
        .get(format!("{base}/v1/homes/{home_id}/invitations"))
        .bearer_auth(&owner)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    let ids: Vec<String> = still_pending
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|i| i["id"].as_str().map(str::to_string))
        .collect();
    assert!(
        ids.contains(&invitation_id),
        "the invitation survived the stranger"
    );
}
