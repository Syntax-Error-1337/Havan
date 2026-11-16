//! Device licences: a device cannot be activated unless the vendor paid for it, and a
//! retried claim is billed once.
//!
//! These run against the real database because the property is a database property — the
//! balance is a query, the charge is a row, and the uniqueness is a constraint. A fake
//! would let all three be true in memory and false in production.

use domain::licence::{LicenceRepo, NewGrant};
use domain::pairing::{PairingRepo, SerialTake};
use domain::serials::{SerialRepo, normalise};
use kernel::ErrorCode;
use uuid::Uuid;

struct Fixture {
    pg: infra::Pg,
    db_url: String,
    vendor_id: Uuid,
    product_id: Uuid,
}

/// Skip (not fail) when there is no test database: the suite runs in CI with one and on a
/// laptop without the containers, and a test that fails for "no database" trains people to
/// ignore it.
async fn fixture() -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db_url, 2).await.ok()?;
    Some(Fixture {
        pg,
        db_url,
        vendor_id: Uuid::now_v7(),
        product_id: Uuid::now_v7(),
    })
}

/// A one-connection pool wearing `haven_system`, which is the role the API itself uses
/// for provisioning. Fixtures need it because a plain connection sees no tenant and
/// `FORCE ROW LEVEL SECURITY` refuses the insert — which is the behaviour under test.
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

async fn seed(f: &Fixture) {
    let admin = admin(f).await;

    let vendor = f.vendor_id;
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, $3)")
        .bind(vendor)
        .bind(format!("lic-{}", &vendor.to_string()[20..32]))
        .bind("Licence Test Vendor")
        .execute(&admin)
        .await
        .expect("vendor");

    // A minimal thing model: the pairing slice only reads `constraints`, and the licence
    // path never looks inside it.
    let model = serde_json::json!({
        "schema": 1,
        "properties": {},
        "actions": {},
        "constraints": { "allow_unknown_serial": false }
    });
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status)
         VALUES ($1, $2, $3, $4, 'light', $5, 'published')",
    )
    .bind(f.product_id)
    .bind(vendor)
    .bind(format!("prd-{}", &f.product_id.to_string()[20..32]))
    .bind("Licence Test Bulb")
    .bind(&model)
    .execute(&admin)
    .await
    .expect("product");
}

/// Two claimed devices, a grant to pay for them, and the current month as the period to invoice.
async fn two_activations(f: &Fixture, unit_price_cents: i32) -> String {
    let licences = infra::PgLicence::new(f.pg.clone());
    let pairing = infra::PgPairing::new(f.pg.clone());
    let serials = infra::PgSerials::new(f.pg.clone());

    serials
        .add(
            f.vendor_id,
            f.product_id,
            &normalise(vec!["INV-1".to_string(), "INV-2".to_string()]),
        )
        .await
        .expect("manifest");

    licences
        .grant(&NewGrant {
            vendor_id: f.vendor_id,
            quantity: 10,
            unit_price_cents,
            currency: None,
            expires_in_days: None,
            note: None,
        })
        .await
        .expect("grant");

    for serial in ["INV-1", "INV-2"] {
        let take = pairing
            .take_serial(f.product_id, serial, false)
            .await
            .expect("take");
        assert_eq!(take, SerialTake::Taken, "claiming {serial}");
    }

    domain::licence::period_of(time::OffsetDateTime::now_utc())
}

#[tokio::test]
async fn a_draft_counts_this_months_activations_at_their_own_price() {
    let Some(f) = fixture().await else { return };
    seed(&f).await;
    let period = two_activations(&f, 249).await;

    let licences = infra::PgLicence::new(f.pg.clone());
    let invoice = licences
        .invoice_preview(f.vendor_id, &period)
        .await
        .expect("preview");

    assert_eq!(invoice.status, "draft");
    assert_eq!(invoice.quantity, 2);
    // The price comes from the activation rows, not from today's grant: changing the price
    // tomorrow cannot rewrite yesterday's bill.
    assert_eq!(invoice.amount_cents, 498);
    assert_eq!(invoice.currency, "USD");
    assert_eq!(invoice.lines.len(), 2);
    assert!(invoice.issued_at.is_none());
    assert!(invoice.lines.iter().all(|l| l.unit_price_cents == 249));
}

#[tokio::test]
async fn an_issued_month_stops_moving() {
    let Some(f) = fixture().await else { return };
    seed(&f).await;
    let period = two_activations(&f, 249).await;

    let licences = infra::PgLicence::new(f.pg.clone());
    let pairing = infra::PgPairing::new(f.pg.clone());
    let serials = infra::PgSerials::new(f.pg.clone());

    let issued = licences
        .invoice_issue(f.vendor_id, &period, None)
        .await
        .expect("issue");
    assert_eq!(issued.status, "issued");
    assert_eq!(issued.quantity, 2);

    // Another device goes live in the same month, after the invoice was sent.
    serials
        .add(
            f.vendor_id,
            f.product_id,
            &normalise(vec!["INV-3".to_string()]),
        )
        .await
        .expect("manifest");
    let take = pairing
        .take_serial(f.product_id, "INV-3", false)
        .await
        .expect("take");
    assert_eq!(take, SerialTake::Taken);

    let after = licences
        .invoice_preview(f.vendor_id, &period)
        .await
        .expect("preview");

    // The document is what was sent; the third device belongs to the next one. An invoice that
    // grew after being approved is a bill changing under the person who approved it.
    assert_eq!(after.status, "issued");
    assert_eq!(after.quantity, 2);
    assert_eq!(after.amount_cents, 498);
    assert_eq!(after.issued_at, issued.issued_at);

    // And the ledger still knows about it, which is the difference between "not invoiced" and
    // "not activated".
    let summary = licences.summary(f.vendor_id).await.expect("summary");
    assert_eq!(summary.used, 3);
}

#[tokio::test]
async fn issuing_twice_returns_the_first_document() {
    let Some(f) = fixture().await else { return };
    seed(&f).await;
    let period = two_activations(&f, 249).await;

    let licences = infra::PgLicence::new(f.pg.clone());
    let first = licences
        .invoice_issue(f.vendor_id, &period, None)
        .await
        .expect("first");
    let second = licences
        .invoice_issue(f.vendor_id, &period, None)
        .await
        .expect("second");

    // A nightly retry, or two operators clicking at once, must not produce two bills.
    assert_eq!(first.issued_at, second.issued_at);
    assert_eq!(second.quantity, 2);
}

#[tokio::test]
async fn a_month_that_has_not_started_cannot_be_invoiced() {
    let Some(f) = fixture().await else { return };
    seed(&f).await;

    let licences = infra::PgLicence::new(f.pg.clone());
    let future =
        domain::licence::period_of(time::OffsetDateTime::now_utc() + time::Duration::days(40));
    let err = licences
        .invoice_issue(f.vendor_id, &future, None)
        .await
        .unwrap_err();

    assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
}

#[tokio::test]
async fn a_vendor_with_no_licence_cannot_activate_a_device() {
    let Some(f) = fixture().await else { return };
    seed(&f).await;

    let pairing = infra::PgPairing::new(f.pg.clone());
    let licences = infra::PgLicence::new(f.pg.clone());
    let serials = infra::PgSerials::new(f.pg.clone());

    serials
        .add(
            f.vendor_id,
            f.product_id,
            &normalise(vec!["SN-1".to_string()]),
        )
        .await
        .expect("manifest");

    let take = pairing
        .take_serial(f.product_id, "SN-1", false)
        .await
        .expect("take");

    assert_eq!(
        take,
        SerialTake::LicenceExhausted,
        "a vendor with no grant must not activate a device"
    );

    // The serial must still be on the shelf: a refused claim cannot burn a unit.
    let summary = licences.summary(f.vendor_id).await.expect("summary");
    assert_eq!(summary.used, 0);
    assert_eq!(summary.remaining, 0);
}

#[tokio::test]
async fn one_grant_activates_one_device_and_the_balance_says_so() {
    let Some(f) = fixture().await else { return };
    seed(&f).await;

    let pairing = infra::PgPairing::new(f.pg.clone());
    let licences = infra::PgLicence::new(f.pg.clone());
    let serials = infra::PgSerials::new(f.pg.clone());

    serials
        .add(
            f.vendor_id,
            f.product_id,
            &normalise(vec!["SN-10".to_string(), "SN-11".to_string()]),
        )
        .await
        .expect("manifest");

    licences
        .grant(&NewGrant {
            vendor_id: f.vendor_id,
            quantity: 1,
            unit_price_cents: 250,
            currency: None,
            expires_in_days: None,
            note: Some("test grant".into()),
        })
        .await
        .expect("grant");

    let before = licences.summary(f.vendor_id).await.expect("summary");
    assert_eq!((before.granted, before.used, before.remaining), (1, 0, 1));
    assert_eq!(before.unit_price_cents, 250);

    assert_eq!(
        pairing
            .take_serial(f.product_id, "SN-10", false)
            .await
            .expect("take"),
        SerialTake::Taken
    );

    let after = licences.summary(f.vendor_id).await.expect("summary");
    assert_eq!((after.granted, after.used, after.remaining), (1, 1, 0));
    assert_eq!(after.used_value_cents, 250);
    assert_eq!(after.activated_this_period, 1);

    // The second device is refused, and refused for the right reason.
    assert_eq!(
        pairing
            .take_serial(f.product_id, "SN-11", false)
            .await
            .expect("take"),
        SerialTake::LicenceExhausted
    );

    let activations = licences.activations(f.vendor_id, 10).await.expect("list");
    assert_eq!(activations.len(), 1, "one paid activation, one row");
    assert_eq!(activations[0].serial, "SN-10");
    assert_eq!(activations[0].unit_price_cents, 250);
}

#[tokio::test]
async fn a_replayed_claim_is_charged_once() {
    let Some(f) = fixture().await else { return };
    seed(&f).await;

    let pairing = infra::PgPairing::new(f.pg.clone());
    let licences = infra::PgLicence::new(f.pg.clone());
    let serials = infra::PgSerials::new(f.pg.clone());

    serials
        .add(
            f.vendor_id,
            f.product_id,
            &normalise(vec!["SN-20".to_string()]),
        )
        .await
        .expect("manifest");
    licences
        .grant(&NewGrant {
            vendor_id: f.vendor_id,
            quantity: 1,
            unit_price_cents: 500,
            currency: Some("EUR".into()),
            expires_in_days: None,
            note: None,
        })
        .await
        .expect("grant");

    assert_eq!(
        pairing
            .take_serial(f.product_id, "SN-20", false)
            .await
            .expect("take"),
        SerialTake::Taken
    );

    // Same serial, second attempt: the manifest says it is spoken for, and the ledger must
    // not grow. This is the shape of a claim retried over a flaky link.
    assert_eq!(
        pairing
            .take_serial(f.product_id, "SN-20", false)
            .await
            .expect("take"),
        SerialTake::AlreadyClaimed
    );

    let summary = licences.summary(f.vendor_id).await.expect("summary");
    assert_eq!(summary.used, 1, "a retried claim is still one device");
    assert_eq!(summary.currency, "EUR");
}

#[tokio::test]
async fn an_expired_grant_stops_activating() {
    let Some(f) = fixture().await else { return };
    seed(&f).await;

    let pairing = infra::PgPairing::new(f.pg.clone());
    let licences = infra::PgLicence::new(f.pg.clone());
    let serials = infra::PgSerials::new(f.pg.clone());

    serials
        .add(
            f.vendor_id,
            f.product_id,
            &normalise(vec!["SN-30".to_string()]),
        )
        .await
        .expect("manifest");

    // A grant that expired yesterday is money that has run out, not a licence.
    let admin = admin(&f).await;
    sqlx::query(
        "INSERT INTO licence_grants (id, vendor_id, quantity, unit_price_cents, expires_at)
         VALUES ($1, $2, 5, 100, now() - interval '1 day')",
    )
    .bind(Uuid::now_v7())
    .bind(f.vendor_id)
    .execute(&admin)
    .await
    .expect("expired grant");

    let summary = licences.summary(f.vendor_id).await.expect("summary");
    assert_eq!(summary.granted, 0, "an expired grant does not count");

    assert_eq!(
        pairing
            .take_serial(f.product_id, "SN-30", false)
            .await
            .expect("take"),
        SerialTake::LicenceExhausted
    );
}

#[tokio::test]
async fn a_grant_to_another_vendor_does_not_pay_for_this_one() {
    let Some(f) = fixture().await else { return };
    seed(&f).await;

    let pairing = infra::PgPairing::new(f.pg.clone());
    let licences = infra::PgLicence::new(f.pg.clone());
    let serials = infra::PgSerials::new(f.pg.clone());

    serials
        .add(
            f.vendor_id,
            f.product_id,
            &normalise(vec!["SN-40".to_string()]),
        )
        .await
        .expect("manifest");

    let admin = admin(&f).await;
    let other = Uuid::now_v7();
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Other Vendor')")
        .bind(other)
        .bind(format!("oth-{}", &other.to_string()[20..32]))
        .execute(&admin)
        .await
        .expect("other vendor");

    licences
        .grant(&NewGrant {
            vendor_id: other,
            quantity: 100,
            unit_price_cents: 100,
            currency: None,
            expires_in_days: None,
            note: None,
        })
        .await
        .expect("grant");

    assert_eq!(
        pairing
            .take_serial(f.product_id, "SN-40", false)
            .await
            .expect("take"),
        SerialTake::LicenceExhausted,
        "another vendor's balance must not activate this vendor's devices"
    );
}

#[tokio::test]
async fn the_error_never_reads_as_a_permission_problem() {
    // 402, not 403: the caller is allowed, the vendor simply has not paid.
    assert_eq!(
        ErrorCode::LicenceExhausted.status(),
        402,
        "licence exhaustion is a payment state, not an authorisation one"
    );
}
