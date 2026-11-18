//! Vendor teams against the database: invitation, acceptance, and the last-owner rule.

use domain::vendor_team::{Role, VendorTeamRepo};
use kernel::ErrorCode;
use uuid::Uuid;

struct Fixture {
    db_url: String,
    pg: infra::Pg,
    vendor_id: Uuid,
    owner_id: Uuid,
}

async fn fixture() -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db_url, 2).await.ok()?;
    let vendor_id = Uuid::now_v7();
    let owner_id = Uuid::now_v7();
    let tag = &vendor_id.to_string()[20..32];

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .ok()?;
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .ok()?;

    sqlx::query("INSERT INTO vendors (id, name, slug) VALUES ($1, 'Team Vendor', $2)")
        .bind(vendor_id)
        .bind(format!("team-{tag}"))
        .execute(&pool)
        .await
        .ok()?;

    let hash = domain::identity::hash_password("owner-Password-1").ok()?;
    sqlx::query(
        r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash)
           VALUES ($1, $2, $3, 'Owner', 'owner', $4)"#,
    )
    .bind(owner_id)
    .bind(vendor_id)
    .bind(format!("owner-{tag}@vendor.test"))
    .bind(hash)
    .execute(&pool)
    .await
    .ok()?;

    Some(Fixture {
        db_url,
        pg,
        vendor_id,
        owner_id,
    })
}

async fn admin_pool(f: &Fixture) -> sqlx::PgPool {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&f.db_url)
        .await
        .expect("pool");
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .expect("system role");
    pool
}

#[tokio::test]
async fn an_invitation_creates_a_working_account_with_the_invited_role() {
    let Some(f) = fixture().await else { return };
    let team = infra::PgVendorTeam::new(f.pg.clone());
    let email = format!("dev-{}@vendor.test", &Uuid::now_v7().to_string()[20..32]);

    let token = team
        .invite(f.vendor_id, f.owner_id, &email, Role::Developer)
        .await
        .expect("invite");
    assert!(token.len() >= 32, "the token must be unguessable");

    let (vendor, user) = team
        .accept(&token, "New Developer", "a-good-password-1")
        .await
        .expect("accept");
    assert_eq!(vendor, f.vendor_id);

    // The account exists, with the invited role, and the password works.
    let pool = admin_pool(&f).await;
    let (role, hash): (String, String) =
        sqlx::query_as("SELECT role, password_hash FROM vendor_users WHERE id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .expect("user");
    assert_eq!(role, "developer");
    assert!(domain::identity::verify_password(
        "a-good-password-1",
        &hash
    ));

    // The invitation cannot be replayed.
    let err = team
        .accept(&token, "Impostor", "another-password-1")
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Conflict);

    // And it is no longer listed as pending.
    let listing = team.team(f.vendor_id).await.expect("team");
    assert_eq!(listing.members.len(), 2);
    assert!(listing.invites.is_empty());
}

#[tokio::test]
async fn a_second_live_invitation_to_the_same_address_is_refused() {
    let Some(f) = fixture().await else { return };
    let team = infra::PgVendorTeam::new(f.pg.clone());
    let email = format!("dup-{}@vendor.test", &Uuid::now_v7().to_string()[20..32]);

    team.invite(f.vendor_id, f.owner_id, &email, Role::Viewer)
        .await
        .expect("first");
    let err = team
        .invite(f.vendor_id, f.owner_id, &email, Role::Viewer)
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Conflict);
}

#[tokio::test]
async fn an_expired_invitation_is_refused() {
    let Some(f) = fixture().await else { return };
    let team = infra::PgVendorTeam::new(f.pg.clone());
    let email = format!("late-{}@vendor.test", &Uuid::now_v7().to_string()[20..32]);

    let token = team
        .invite(f.vendor_id, f.owner_id, &email, Role::Viewer)
        .await
        .expect("invite");

    let pool = admin_pool(&f).await;
    sqlx::query("UPDATE vendor_invites SET expires_at = now() - interval '1 hour' WHERE accepted_at IS NULL AND vendor_id = $1")
        .bind(f.vendor_id)
        .execute(&pool)
        .await
        .expect("expire");

    let err = team
        .accept(&token, "Late", "a-good-password-1")
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Conflict);
}

#[tokio::test]
async fn the_last_owner_cannot_be_removed() {
    let Some(f) = fixture().await else { return };
    let team = infra::PgVendorTeam::new(f.pg.clone());

    let err = team
        .remove(f.vendor_id, f.owner_id, f.owner_id)
        .await
        .unwrap_err();
    // The last-owner rule is checked first, and that is the right order: "this is the last owner"
    // is the reason the person needs to hear, even when it is also themselves they are removing.
    assert_eq!(err.code_kind(), ErrorCode::Conflict);

    // A second owner makes removal legal in principle; the actor still cannot remove themselves.
    let second = Uuid::now_v7();
    let pool = admin_pool(&f).await;
    sqlx::query(
        r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash)
           VALUES ($1, $2, $3, 'Owner Two', 'owner', 'x')"#,
    )
    .bind(second)
    .bind(f.vendor_id)
    .bind(format!(
        "owner2-{}@vendor.test",
        &second.to_string()[20..32]
    ))
    .execute(&pool)
    .await
    .expect("second owner");

    let err = team.remove(f.vendor_id, f.owner_id, second).await;
    assert!(err.is_ok(), "one owner may remove another when two exist");
}

#[tokio::test]
async fn an_admin_cannot_remove_another_admin() {
    let Some(f) = fixture().await else { return };
    let team = infra::PgVendorTeam::new(f.pg.clone());
    let pool = admin_pool(&f).await;

    let admin_a = Uuid::now_v7();
    let admin_b = Uuid::now_v7();
    for (id, name) in [(admin_a, "Admin A"), (admin_b, "Admin B")] {
        sqlx::query(
            r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash)
               VALUES ($1, $2, $3, $4, 'admin', 'x')"#,
        )
        .bind(id)
        .bind(f.vendor_id)
        .bind(format!(
            "{}-{}@vendor.test",
            name.replace(' ', ""),
            &id.to_string()[20..32]
        ))
        .bind(name)
        .execute(&pool)
        .await
        .expect("admin");
    }

    let err = team
        .remove(f.vendor_id, admin_a, admin_b)
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Forbidden);
}

#[tokio::test]
async fn an_invitation_can_be_withdrawn_before_anybody_uses_it() {
    let Some(f) = fixture().await else { return };
    let team = infra::PgVendorTeam::new(f.pg.clone());

    let email = format!("withdraw-{}@vendor.test", &f.vendor_id.to_string()[20..32]);
    let token = team
        .invite(f.vendor_id, f.owner_id, &email, Role::Developer)
        .await
        .expect("invite");

    // The invitation is a working credential; withdrawing it has to take effect for the credential too.
    let team_view = team.team(f.vendor_id).await.expect("team");
    let invite = team_view
        .invites
        .iter()
        .find(|i| i.email == email)
        .expect("listed")
        .id;

    team.revoke_invite(f.vendor_id, f.owner_id, invite)
        .await
        .expect("revoke");

    let after = team.team(f.vendor_id).await.expect("team");
    assert!(
        after.invites.iter().all(|i| i.id != invite),
        "the invitation is gone from the list"
    );

    let refused = team
        .accept(&token, "Withdrawn", "withdraw-Password-1")
        .await
        .unwrap_err();
    assert_eq!(
        refused.code_kind(),
        ErrorCode::NotFound,
        "a withdrawn token is not a way in"
    );

    // A second withdrawal is a 404, not a second success: the caller cannot tell "already gone" from "never existed",
    // and does not need to.
    let again = team
        .revoke_invite(f.vendor_id, f.owner_id, invite)
        .await
        .unwrap_err();
    assert_eq!(again.code_kind(), ErrorCode::NotFound);
}

#[tokio::test]
async fn a_developer_cannot_withdraw_an_invitation() {
    let Some(f) = fixture().await else { return };
    let team = infra::PgVendorTeam::new(f.pg.clone());
    let pool = admin_pool(&f).await;

    let developer = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash)
           VALUES ($1, $2, $3, 'Developer', 'developer', 'x')"#,
    )
    .bind(developer)
    .bind(f.vendor_id)
    .bind(format!(
        "dev-{}@vendor.test",
        &developer.to_string()[20..32]
    ))
    .execute(&pool)
    .await
    .expect("developer");

    let email = format!("keep-{}@vendor.test", &f.vendor_id.to_string()[20..32]);
    team.invite(f.vendor_id, f.owner_id, &email, Role::Viewer)
        .await
        .expect("invite");
    let invite = team
        .team(f.vendor_id)
        .await
        .expect("team")
        .invites
        .iter()
        .find(|i| i.email == email)
        .expect("listed")
        .id;

    let refused = team
        .revoke_invite(f.vendor_id, developer, invite)
        .await
        .unwrap_err();
    assert_eq!(refused.code_kind(), ErrorCode::Forbidden);
}
