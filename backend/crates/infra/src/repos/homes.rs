use domain::homes::{Home, HomeInvite, HomeRepo, Member, Role, Room};
use kernel::AppError;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgHomes {
    pool: PgPool,
}

impl PgHomes {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// A transaction that may cross tenant boundaries, the same way `Pg::scoped(system)` does.
    ///
    /// Invitations need it for one specific reason: the person accepting is not a member of the home yet, so a policy
    /// keyed on membership cannot see the row they are accepting. The service has already checked the token's address
    /// against the signed-in account — this is plumbing, not the authority.
    async fn system_tx(&self) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, AppError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("SET LOCAL ROLE haven_system")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        Ok(tx)
    }

    /// Run a homes query with the caller's identity in place.
    ///
    /// `devices` carries a row-level policy keyed on `haven_user()`, and this
    /// query counts them. Without the setting the count is not an error — RLS
    /// simply returns zero rows — so a home with nine devices reported "0
    /// devices" while the device list beside it drew all nine.
    async fn as_user(
        &self,
        user_id: Uuid,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, AppError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("SELECT set_config('haven.user_id', $1, true)")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        Ok(tx)
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

type InviteRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    Uuid,
    time::OffsetDateTime,
    time::OffsetDateTime,
    Option<time::OffsetDateTime>,
);

fn invite_of(row: InviteRow) -> HomeInvite {
    let (id, home_id, email, role, token_hash, invited_by, created_at, expires_at, accepted_at) =
        row;
    HomeInvite {
        id,
        home_id,
        email,
        role: role_of(&role),
        token_hash,
        invited_by,
        created_at,
        expires_at,
        accepted_at,
    }
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::Member => "member",
        Role::Guest => "guest",
    }
}

fn role_of(s: &str) -> Role {
    // A CHECK constraint keeps this total; defaulting to the least privilege
    // means a corrupt row cannot escalate.
    Role::parse(s).unwrap_or(Role::Guest)
}

impl HomeRepo for PgHomes {
    async fn create_home(&self, home: &Home) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await.map_err(db)?;

        sqlx::query(r#"INSERT INTO homes (id, owner_id, name, timezone) VALUES ($1, $2, $3, $4)"#)
            .bind(home.id)
            .bind(home.owner_id)
            .bind(&home.name)
            .bind(&home.timezone)
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        sqlx::query(
            r#"INSERT INTO home_members (home_id, user_id, role, can_pair)
               VALUES ($1, $2, 'owner', true)"#,
        )
        .bind(home.id)
        .bind(home.owner_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)
    }

    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<Home>, AppError> {
        let mut tx = self.as_user(user_id).await?;
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                String,
                String,
                i64,
                i64,
                Option<f64>,
                Option<f64>,
                time::OffsetDateTime,
            ),
        >(
            r#"SELECT h.id, h.owner_id, h.name, h.timezone, m.role,
                      (SELECT count(*) FROM home_members x WHERE x.home_id = h.id) AS member_count,
                      (SELECT count(*) FROM devices d WHERE d.home_id = h.id) AS device_count,
                      h.geo_lat, h.geo_lon,
                      h.created_at
               FROM homes h
               JOIN home_members m ON m.home_id = h.id AND m.user_id = $1
               ORDER BY h.created_at"#,
        )
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    owner_id,
                    name,
                    timezone,
                    role,
                    member_count,
                    device_count,
                    geo_lat,
                    geo_lon,
                    created_at,
                )| {
                    Home {
                        id,
                        owner_id,
                        name,
                        timezone,
                        role: role_of(&role),
                        member_count,
                        device_count,
                        geo_lat,
                        geo_lon,
                        created_at,
                    }
                },
            )
            .collect())
    }

    async fn get(&self, home_id: Uuid, user_id: Uuid) -> Result<Option<Home>, AppError> {
        let mut tx = self.as_user(user_id).await?;
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                String,
                String,
                i64,
                i64,
                Option<f64>,
                Option<f64>,
                time::OffsetDateTime,
            ),
        >(
            r#"SELECT h.id, h.owner_id, h.name, h.timezone, m.role,
                      (SELECT count(*) FROM home_members x WHERE x.home_id = h.id) AS member_count,
                      (SELECT count(*) FROM devices d WHERE d.home_id = h.id) AS device_count,
                      h.geo_lat, h.geo_lon,
                      h.created_at
               FROM homes h
               JOIN home_members m ON m.home_id = h.id AND m.user_id = $2
               WHERE h.id = $1"#,
        )
        .bind(home_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(row.map(
            |(
                id,
                owner_id,
                name,
                timezone,
                role,
                member_count,
                device_count,
                geo_lat,
                geo_lon,
                created_at,
            )| Home {
                id,
                owner_id,
                name,
                timezone,
                role: role_of(&role),
                member_count,
                device_count,
                geo_lat,
                geo_lon,
                created_at,
            },
        ))
    }

    async fn rename(&self, home_id: Uuid, name: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE homes SET name = $2 WHERE id = $1")
            .bind(home_id)
            .bind(name)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db)
    }

    async fn set_timezone(&self, home_id: Uuid, timezone: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE homes SET timezone = $2 WHERE id = $1")
            .bind(home_id)
            .bind(timezone)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db)
    }

    async fn timezone_exists(&self, timezone: &str) -> Result<bool, AppError> {
        // Postgres ships the tz database as a view; asking it is cheaper and more correct than
        // carrying a third-party list that drifts from the one the scheduler will use.
        let row: (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1)")
                .bind(timezone)
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        Ok(row.0)
    }

    async fn set_location(
        &self,
        home_id: Uuid,
        lat: Option<f64>,
        lon: Option<f64>,
    ) -> Result<(), AppError> {
        sqlx::query("UPDATE homes SET geo_lat = $2, geo_lon = $3 WHERE id = $1")
            .bind(home_id)
            .bind(lat)
            .bind(lon)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db)
    }

    async fn set_presence(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        present: bool,
    ) -> Result<(), AppError> {
        sqlx::query("UPDATE home_members SET present = $3 WHERE home_id = $1 AND user_id = $2")
            .bind(home_id)
            .bind(user_id)
            .bind(present)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db)
    }

    async fn delete(&self, home_id: Uuid) -> Result<(), AppError> {
        // Devices are unclaimed rather than deleted: a vendor's fleet history
        // outlives one customer's home.
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("UPDATE devices SET home_id = NULL, room_id = NULL, state = 'released' WHERE home_id = $1")
            .bind(home_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("DELETE FROM homes WHERE id = $1")
            .bind(home_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn members(&self, home_id: Uuid) -> Result<Vec<Member>, AppError> {
        let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, String, bool)>(
            r#"SELECT u.id, u.display_name, u.email, m.role, m.can_pair
               FROM home_members m JOIN users u ON u.id = m.user_id
               WHERE m.home_id = $1
               ORDER BY m.created_at"#,
        )
        .bind(home_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        Ok(rows
            .into_iter()
            .map(|(user_id, display_name, email, role, can_pair)| Member {
                user_id,
                display_name,
                email,
                role: role_of(&role),
                can_pair,
            })
            .collect())
    }

    async fn add_member(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        role: Role,
        can_pair: bool,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO home_members (home_id, user_id, role, can_pair)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (home_id, user_id) DO UPDATE SET role = EXCLUDED.role"#,
        )
        .bind(home_id)
        .bind(user_id)
        .bind(role.as_str())
        .bind(can_pair)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db)
    }

    async fn find_member_by_email(
        &self,
        home_id: Uuid,
        email: &str,
    ) -> Result<Option<Uuid>, AppError> {
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT u.id FROM users u
               WHERE lower(u.email) = lower($1) AND u.disabled_at IS NULL
                 AND NOT EXISTS (
                   SELECT 1 FROM home_members m WHERE m.home_id = $2 AND m.user_id = u.id
                 )"#,
        )
        .bind(email)
        .bind(home_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    async fn set_member_role(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        role: Role,
    ) -> Result<(), AppError> {
        sqlx::query("UPDATE home_members SET role = $3 WHERE home_id = $1 AND user_id = $2")
            .bind(home_id)
            .bind(user_id)
            .bind(role.as_str())
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db)
    }

    async fn remove_member(&self, home_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        sqlx::query("DELETE FROM home_members WHERE home_id = $1 AND user_id = $2")
            .bind(home_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db)
    }

    async fn rooms(&self, home_id: Uuid) -> Result<Vec<Room>, AppError> {
        let rows = sqlx::query_as::<_, (Uuid, Uuid, String, Option<String>, i32)>(
            "SELECT id, home_id, name, icon, sort FROM rooms WHERE home_id = $1 ORDER BY sort, name",
        )
        .bind(home_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        Ok(rows
            .into_iter()
            .map(|(id, home_id, name, icon, sort)| Room {
                id,
                home_id,
                name,
                icon,
                sort,
            })
            .collect())
    }

    async fn rename_room(&self, room_id: Uuid, name: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE rooms SET name = $2 WHERE id = $1")
            .bind(room_id)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn create_room(&self, room: &Room) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO rooms (id, home_id, name, icon, sort) VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(room.id)
        .bind(room.home_id)
        .bind(&room.name)
        .bind(&room.icon)
        .bind(room.sort)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db)
    }

    async fn delete_room(&self, room_id: Uuid) -> Result<(), AppError> {
        // Devices in a deleted room are unassigned, not deleted.
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("UPDATE devices SET room_id = NULL WHERE room_id = $1")
            .bind(room_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("DELETE FROM rooms WHERE id = $1")
            .bind(room_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn create_invite(&self, invite: &HomeInvite) -> Result<(), AppError> {
        // One live invitation per address per home: re-inviting replaces the old one rather than stacking, because two
        // working links for the same person is how "which one did I send?" starts. Two statements in one transaction
        // rather than `ON CONFLICT`, which needs the partial index's predicate to be inferred exactly — a failure that
        // shows up as "a dependency is unavailable" at the edge and as nothing at all in a log.
        let mut tx = self.system_tx().await?;
        sqlx::query(
            r#"DELETE FROM home_invites
                WHERE home_id = $1 AND lower(email) = lower($2) AND accepted_at IS NULL"#,
        )
        .bind(invite.home_id)
        .bind(&invite.email)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        sqlx::query(
            r#"INSERT INTO home_invites (id, home_id, email, role, token_hash, invited_by, created_at, expires_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        )
        .bind(invite.id)
        .bind(invite.home_id)
        .bind(&invite.email)
        .bind(role_str(invite.role))
        .bind(&invite.token_hash)
        .bind(invite.invited_by)
        .bind(invite.created_at)
        .bind(invite.expires_at)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn invites(&self, home_id: Uuid) -> Result<Vec<HomeInvite>, AppError> {
        let rows = sqlx::query_as::<_, InviteRow>(
            r#"SELECT id, home_id, email, role, token_hash, invited_by, created_at, expires_at, accepted_at
                 FROM home_invites
                WHERE home_id = $1 AND accepted_at IS NULL
                ORDER BY created_at DESC"#,
        )
        .bind(home_id)
        .fetch_all(&mut *self.system_tx().await?)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(invite_of).collect())
    }

    async fn invites_for_email(&self, email: &str) -> Result<Vec<HomeInvite>, AppError> {
        let rows = sqlx::query_as::<_, InviteRow>(
            r#"SELECT id, home_id, email, role, token_hash, invited_by, created_at, expires_at, accepted_at
                 FROM home_invites
                WHERE lower(email) = lower($1) AND accepted_at IS NULL AND expires_at > now()
                ORDER BY created_at DESC"#,
        )
        .bind(email)
        .fetch_all(&mut *self.system_tx().await?)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(invite_of).collect())
    }

    async fn invite_by_hash(&self, token_hash: &str) -> Result<Option<HomeInvite>, AppError> {
        let row = sqlx::query_as::<_, InviteRow>(
            r#"SELECT id, home_id, email, role, token_hash, invited_by, created_at, expires_at, accepted_at
                 FROM home_invites
                WHERE token_hash = $1"#,
        )
        .bind(token_hash)
        .fetch_optional(&mut *self.system_tx().await?)
        .await
        .map_err(db)?;
        Ok(row.map(invite_of))
    }

    async fn accept_invite(&self, id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        // The transaction has to be committed, not merely dropped: a `SET LOCAL ROLE` transaction that is dropped
        // rolls back, and the first version of this method reported success while leaving the invitation live — which
        // a second accept then happily joined twice.
        let mut tx = self.system_tx().await?;
        let n = sqlx::query(
            "UPDATE home_invites SET accepted_at = now(), accepted_user_id = $2
              WHERE id = $1 AND accepted_at IS NULL",
        )
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        tx.commit().await.map_err(db)?;

        // Accepting twice is a second tap, not an error — but a row that was already accepted must not silently
        // re-add somebody who was later removed from the home.
        if n == 0 {
            return Err(AppError::validation("that invitation was already used"));
        }
        Ok(())
    }

    async fn revoke_invite(&self, home_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let mut tx = self.system_tx().await?;
        let n = sqlx::query("DELETE FROM home_invites WHERE id = $1 AND home_id = $2")
            .bind(id)
            .bind(home_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        tx.commit().await.map_err(db)?;
        if n == 0 {
            return Err(AppError::not_found("invitation"));
        }
        Ok(())
    }

    async fn room_belongs_to(&self, room_id: Uuid, home_id: Uuid) -> Result<bool, AppError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM rooms WHERE id = $1 AND home_id = $2)",
        )
        .bind(room_id)
        .bind(home_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }
}
