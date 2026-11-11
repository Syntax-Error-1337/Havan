use domain::homes::{Home, HomeRepo, Member, Role, Room};
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
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
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
                time::OffsetDateTime,
            ),
        >(
            r#"SELECT h.id, h.owner_id, h.name, h.timezone, m.role,
                      (SELECT count(*) FROM home_members x WHERE x.home_id = h.id) AS member_count,
                      (SELECT count(*) FROM devices d WHERE d.home_id = h.id) AS device_count,
                      h.created_at
               FROM homes h
               JOIN home_members m ON m.home_id = h.id AND m.user_id = $1
               ORDER BY h.created_at"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(id, owner_id, name, timezone, role, member_count, device_count, created_at)| {
                    Home {
                        id,
                        owner_id,
                        name,
                        timezone,
                        role: role_of(&role),
                        member_count,
                        device_count,
                        created_at,
                    }
                },
            )
            .collect())
    }

    async fn get(&self, home_id: Uuid, user_id: Uuid) -> Result<Option<Home>, AppError> {
        sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                String,
                String,
                i64,
                i64,
                time::OffsetDateTime,
            ),
        >(
            r#"SELECT h.id, h.owner_id, h.name, h.timezone, m.role,
                      (SELECT count(*) FROM home_members x WHERE x.home_id = h.id) AS member_count,
                      (SELECT count(*) FROM devices d WHERE d.home_id = h.id) AS device_count,
                      h.created_at
               FROM homes h
               JOIN home_members m ON m.home_id = h.id AND m.user_id = $2
               WHERE h.id = $1"#,
        )
        .bind(home_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
        .map(|row| {
            row.map(
                |(id, owner_id, name, timezone, role, member_count, device_count, created_at)| {
                    Home {
                        id,
                        owner_id,
                        name,
                        timezone,
                        role: role_of(&role),
                        member_count,
                        device_count,
                        created_at,
                    }
                },
            )
        })
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
