//! SSO configuration against the database: who may set it, and what it reports.

use domain::sso::SsoRepo;
use kernel::ErrorCode;
use uuid::Uuid;

struct Fixture {
    pg: infra::Pg,
    vendor_id: Uuid,
    owner_id: Uuid,
    developer_id: Uuid,
}

async fn fixture() -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db_url, 2).await.ok()?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .ok()?;
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .ok()?;

    let vendor_id = Uuid::now_v7();
    let owner_id = Uuid::now_v7();
    let developer_id = Uuid::now_v7();
    let tag = &vendor_id.to_string()[20..32];

    sqlx::query("INSERT INTO vendors (id, name, slug) VALUES ($1, 'SSO Vendor', $2)")
        .bind(vendor_id)
        .bind(format!("sso-{tag}"))
        .execute(&pool)
        .await
        .ok()?;
    for (id, role, name) in [
        (owner_id, "owner", "Owner"),
        (developer_id, "developer", "Dev"),
    ] {
        sqlx::query(
            r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash)
               VALUES ($1, $2, $3, $4, $5, 'x')"#,
        )
        .bind(id)
        .bind(vendor_id)
        .bind(format!("{name}-{tag}@vendor.test"))
        .bind(name)
        .bind(role)
        .execute(&pool)
        .await
        .ok()?;
    }

    Some(Fixture {
        pg,
        vendor_id,
        owner_id,
        developer_id,
    })
}

#[tokio::test]
async fn an_owner_can_record_the_intent_and_bind_identities() {
    let Some(f) = fixture().await else { return };
    let sso = infra::PgSso::new(f.pg.clone());

    // Nothing configured yet.
    let before = sso.config(f.vendor_id).await.expect("config");
    assert!(!before.required);
    assert_eq!(before.bound_members, 0);
    assert_eq!(before.total_members, 2);
    assert!(!before.enforced, "enforcement waits for a real IdP");

    sso.bind_subject(f.vendor_id, f.owner_id, "idp|owner-1")
        .await
        .expect("bind");

    // One of two members is still unbound, so requiring SSO is refused — the mistake is caught by
    // the owner pressing the toggle, not by a colleague who cannot sign in tomorrow.
    let err = sso
        .configure(f.vendor_id, f.owner_id, true, Some("vendor.test"))
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Conflict);

    // With everybody bound it is allowed, and the domain is stored.
    sso.bind_subject(f.vendor_id, f.developer_id, "idp|dev-1")
        .await
        .expect("bind dev");

    let after = sso
        .configure(f.vendor_id, f.owner_id, true, Some("vendor.test"))
        .await
        .expect("configure");

    assert!(after.required);
    assert_eq!(after.domain.as_deref(), Some("vendor.test"));
    assert_eq!(after.bound_members, 2);
    assert_eq!(after.total_members, 2);
    // Still not enforced: the flag records intent until an assertion can be validated (ADR-005).
    assert!(!after.enforced);
}

#[tokio::test]
async fn a_developer_cannot_change_how_the_company_signs_in() {
    let Some(f) = fixture().await else { return };
    let sso = infra::PgSso::new(f.pg.clone());

    let err = sso
        .configure(f.vendor_id, f.developer_id, true, None)
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Forbidden);
}

#[tokio::test]
async fn a_domain_that_is_not_a_domain_is_refused() {
    let Some(f) = fixture().await else { return };
    let sso = infra::PgSso::new(f.pg.clone());

    let err = sso
        .configure(f.vendor_id, f.owner_id, true, Some("not-a-domain"))
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
}

#[tokio::test]
async fn one_identity_cannot_be_bound_to_two_accounts() {
    let Some(f) = fixture().await else { return };
    let sso = infra::PgSso::new(f.pg.clone());

    sso.bind_subject(f.vendor_id, f.owner_id, "idp|shared")
        .await
        .expect("first bind");
    let err = sso
        .bind_subject(f.vendor_id, f.developer_id, "idp|shared")
        .await
        .unwrap_err();

    assert_eq!(err.code_kind(), ErrorCode::Conflict);
}
