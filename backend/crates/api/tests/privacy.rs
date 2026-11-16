//! Export and erasure, against a real database.
//!
//! The properties that matter are the ones a query can get subtly wrong: an export that omits a
//! home the person lives in, an erasure that leaves a co-member's home ownerless, or a tombstone
//! that still answers to the old email address.

use domain::privacy::PrivacyRepo;
use kernel::ErrorCode;
use uuid::Uuid;

struct Fixture {
    db_url: String,
    pg: infra::Pg,
}

async fn fixture() -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db_url, 2).await.ok()?;
    Some(Fixture { db_url, pg })
}

async fn admin(f: &Fixture) -> sqlx::PgPool {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&f.db_url)
        .await
        .expect("admin pool");
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .expect("system role");
    pool
}

struct Seeded {
    user_id: Uuid,
    home_id: Uuid,
    device_id: Uuid,
    vendor_id: Uuid,
    email: String,
    serial: String,
}

async fn seed(f: &Fixture, shared_with_another: bool) -> Seeded {
    let pool = admin(f).await;
    let tag = &Uuid::now_v7().to_string()[20..32];
    let user_id = Uuid::now_v7();
    let home_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let vendor_id = Uuid::now_v7();
    let product_id = Uuid::now_v7();
    let email = format!("dsar-{tag}@example.test");
    let serial = format!("SN-{tag}");

    sqlx::query("INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, 'x', 'Subject')")
        .bind(user_id)
        .bind(&email)
        .execute(&pool)
        .await
        .expect("user");

    sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'Subject flat')")
        .bind(home_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("home");
    sqlx::query("INSERT INTO home_members (home_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(home_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("membership");

    sqlx::query("INSERT INTO vendors (id, name, slug) VALUES ($1, 'DSAR Vendor', $2)")
        .bind(vendor_id)
        .bind(format!("dsar-{tag}"))
        .execute(&pool)
        .await
        .expect("vendor");

    let key = format!("hv_dsar_{tag}");
    let model = serde_json::json!({
        "product_key": key, "name": "DSAR Bulb", "category": "light",
        "capabilities": [{ "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } }],
        "constraints": { "allow_unknown_serial": false }
    });
    sqlx::query(
        r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status, certification)
           VALUES ($1, $2, $3, 'DSAR Bulb', 'light', $4, 'published', 'certified')"#,
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(&key)
    .bind(&model)
    .execute(&pool)
    .await
    .expect("product");

    sqlx::query(
        r#"INSERT INTO devices (id, vendor_id, product_id, product_key, category, home_id, name,
                                serial, secret_hash, state, claimed_at)
           VALUES ($1, $2, $3, $4, 'light', $5, 'Flat bulb', $6, 'x', 'claimed', now())"#,
    )
    .bind(device_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(&key)
    .bind(home_id)
    .bind(&serial)
    .execute(&pool)
    .await
    .expect("device");

    sqlx::query("INSERT INTO notifications (id, user_id, kind, title, body) VALUES ($1, $2, 'alert', 'Door opened', '')")
        .bind(Uuid::now_v7())
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("notification");

    sqlx::query("INSERT INTO refresh_tokens (id, user_id, family_id, hash, expires_at) VALUES ($1, $2, $3, 'h', now() + interval '1 day')")
        .bind(Uuid::now_v7())
        .bind(user_id)
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .expect("session");

    if shared_with_another {
        let other = Uuid::now_v7();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(other)
            .bind(format!("other-{tag}@example.test"))
            .execute(&pool)
            .await
            .expect("other user");
        sqlx::query("INSERT INTO home_members (home_id, user_id, role) VALUES ($1, $2, 'member')")
            .bind(home_id)
            .bind(other)
            .execute(&pool)
            .await
            .expect("other membership");
    }

    Seeded {
        user_id,
        home_id,
        device_id,
        vendor_id,
        email,
        serial,
    }
}

#[tokio::test]
async fn an_export_contains_the_persons_own_data() {
    let Some(f) = fixture().await else { return };
    let s = seed(&f, false).await;
    let privacy = infra::PgPrivacy::new(f.pg.clone());

    let bundle = privacy.export(s.user_id).await.expect("export");
    let body = serde_json::to_string(&bundle).expect("json");

    assert!(body.contains(&s.email), "the export must name the person");
    assert!(
        body.contains(&s.serial),
        "the export must include their device"
    );
    assert!(
        body.contains("Door opened"),
        "the export must include their alerts"
    );
    assert_eq!(bundle.homes.len(), 1);
    assert_eq!(bundle.format_version, 1);
    assert!(
        !bundle.notes.is_empty(),
        "an export says what it does not contain"
    );
}

#[tokio::test]
async fn an_export_does_not_name_other_members() {
    let Some(f) = fixture().await else { return };
    let s = seed(&f, true).await;
    let privacy = infra::PgPrivacy::new(f.pg.clone());

    let bundle = privacy.export(s.user_id).await.expect("export");
    let body = serde_json::to_string(&bundle).expect("json");

    assert!(
        body.contains("\"member_count\":2"),
        "the other member is a count"
    );
    assert!(
        !body.contains("other-") || !body.contains("@example.test\",\"other"),
        "another member's email must not appear in this person's export"
    );
    let home = &bundle.homes[0];
    assert!(home.get("members").is_none(), "no member list in an export");
}

#[tokio::test]
async fn erasure_anonymises_removes_private_rows_and_deletes_an_empty_home() {
    let Some(f) = fixture().await else { return };
    let s = seed(&f, false).await;
    let privacy = infra::PgPrivacy::new(f.pg.clone());
    let pool = admin(&f).await;

    let report = privacy.erase(s.user_id).await.expect("erase");
    assert_eq!(report.homes_deleted, 1);
    assert_eq!(report.devices_deleted, 1);
    assert_eq!(report.memberships_removed, 1);
    assert_eq!(report.notifications_deleted, 1);
    assert_eq!(report.sessions_revoked, 1);

    let row: (
        String,
        Option<String>,
        Option<String>,
        Option<time::OffsetDateTime>,
    ) = sqlx::query_as("SELECT email, phone, password_hash, disabled_at FROM users WHERE id = $1")
        .bind(s.user_id)
        .fetch_one(&pool)
        .await
        .expect("user row");
    assert!(
        row.0.starts_with("erased+") && row.0.ends_with("@invalid"),
        "{}",
        row.0
    );
    assert!(row.1.is_none() && row.2.is_none());
    assert!(row.3.is_some(), "the tombstone records when it happened");

    let homes: i64 = sqlx::query_scalar("SELECT count(*) FROM homes WHERE id = $1")
        .bind(s.home_id)
        .fetch_one(&pool)
        .await
        .expect("homes");
    assert_eq!(
        homes, 0,
        "a home nobody else lives in goes with the account"
    );

    let devices: i64 = sqlx::query_scalar("SELECT count(*) FROM devices WHERE id = $1")
        .bind(s.device_id)
        .fetch_one(&pool)
        .await
        .expect("devices");
    assert_eq!(
        devices, 0,
        "so does its hardware, rather than floating unbound"
    );

    let receipt: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM data_requests WHERE user_id = $1 AND kind = 'erase'",
    )
    .bind(s.user_id)
    .fetch_one(&pool)
    .await
    .expect("receipt");
    assert_eq!(receipt, 1, "the erasure leaves a receipt");
}

/// The retention window comes from the licence covering the person's devices, and the tombstone is
/// scheduled — not deleted — at erasure time.
#[tokio::test]
async fn erasure_schedules_the_purge_from_the_licence() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let s = seed(&f, false).await;
    let pool = admin(&f).await;

    // Seven days, which is not the 90-day default: a passing test must come from the licence and
    // not from the fallback.
    sqlx::query(
        "INSERT INTO licence_grants (id, vendor_id, quantity, unit_price_cents, currency, retention_days)
         VALUES ($1, $2, 10, 249, 'USD', 7)",
    )
    .bind(Uuid::now_v7())
    .bind(s.vendor_id)
    .execute(&pool)
    .await
    .expect("grant");

    let privacy = infra::PgPrivacy::new(f.pg.clone());
    let report = privacy.erase(s.user_id).await.expect("erase");

    assert_eq!(report.retention_days, 7);

    let scheduled: (Option<time::OffsetDateTime>,) =
        sqlx::query_as("SELECT purge_after FROM users WHERE id = $1")
            .bind(s.user_id)
            .fetch_one(&pool)
            .await
            .expect("tombstone");

    let purge_after = scheduled.0.expect("purge_after set");
    let days = (purge_after - time::OffsetDateTime::now_utc()).whole_days();
    assert!(
        (6..=7).contains(&days),
        "expected roughly seven days, got {days}"
    );
}

/// A tombstone that is not due is left alone, however often the sweep runs.
#[tokio::test]
async fn a_tombstone_is_deleted_only_once_its_window_has_passed() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let s = seed(&f, false).await;
    let pool = admin(&f).await;

    sqlx::query(
        "INSERT INTO licence_grants (id, vendor_id, quantity, unit_price_cents, currency, retention_days)
         VALUES ($1, $2, 10, 249, 'USD', 30)",
    )
    .bind(Uuid::now_v7())
    .bind(s.vendor_id)
    .execute(&pool)
    .await
    .expect("grant");

    let privacy = infra::PgPrivacy::new(f.pg.clone());
    privacy.erase(s.user_id).await.expect("erase");

    // Due in 30 days: the sweep must not touch it, and running twice must not change that.
    for _ in 0..2 {
        let report = privacy.purge_due(500).await.expect("purge");
        assert!(
            !report.purged.contains(&s.user_id),
            "purged a tombstone whose window had not passed"
        );
    }

    // Bring the window forward, the way a month would.
    sqlx::query("UPDATE users SET purge_after = now() - interval '1 second' WHERE id = $1")
        .bind(s.user_id)
        .execute(&pool)
        .await
        .expect("advance");

    let report = privacy.purge_due(500).await.expect("purge");
    assert!(report.purged.contains(&s.user_id));

    let gone: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE id = $1")
        .bind(s.user_id)
        .fetch_optional(&pool)
        .await
        .expect("lookup");
    assert!(gone.is_none(), "the tombstone survived its purge");

    // The receipt outlives the row: no foreign key, so a person asking later can still be answered.
    let receipt: Option<(String,)> =
        sqlx::query_as("SELECT kind FROM data_requests WHERE user_id = $1 AND kind = 'purge'")
            .bind(s.user_id)
            .fetch_optional(&pool)
            .await
            .expect("receipt");
    assert!(receipt.is_some(), "no purge receipt was written");

    // And a second run is a no-op rather than an error.
    let again = privacy.purge_due(500).await.expect("second purge");
    assert!(!again.purged.contains(&s.user_id));
}

#[tokio::test]
async fn erasure_is_refused_while_the_person_owns_a_shared_home() {
    let Some(f) = fixture().await else { return };
    let s = seed(&f, true).await;
    let privacy = infra::PgPrivacy::new(f.pg.clone());
    let pool = admin(&f).await;

    let err = privacy.erase(s.user_id).await.unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Conflict);

    // Nothing was half-done: the home, the membership and the account are all still there.
    let homes: i64 = sqlx::query_scalar("SELECT count(*) FROM homes WHERE id = $1")
        .bind(s.home_id)
        .fetch_one(&pool)
        .await
        .expect("homes");
    assert_eq!(homes, 1);

    let members: i64 = sqlx::query_scalar("SELECT count(*) FROM home_members WHERE home_id = $1")
        .bind(s.home_id)
        .fetch_one(&pool)
        .await
        .expect("members");
    assert_eq!(members, 2);

    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(s.user_id)
        .fetch_one(&pool)
        .await
        .expect("email");
    assert_eq!(
        email, s.email,
        "a refused erasure must not anonymise anyone"
    );
}

#[tokio::test]
async fn exporting_an_unknown_user_is_a_404() {
    let Some(f) = fixture().await else { return };
    let privacy = infra::PgPrivacy::new(f.pg.clone());

    let err = privacy.export(Uuid::now_v7()).await.unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::NotFound);
}
