use domain::scenes::{Scene, SceneAction, SceneChange, SceneRepo};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgScenes {
    pg: Pg,
}

impl PgScenes {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    /// Scenes are read and written by a *person*, so the context carries the
    /// user: the RLS policy on `scenes` is home membership, and it needs
    /// `haven_user()` set to answer at all.
    async fn tx(
        &self,
        user_id: Uuid,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, AppError> {
        self.pg
            .scoped(&TenantCtx {
                vendor_id: None,
                actor: kernel::Actor::User {
                    user_id,
                    vendor_id: None,
                },
                request_id: "scenes".into(),
            })
            .await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

type Row = (
    Uuid,
    Uuid,
    String,
    Option<String>,
    serde_json::Value,
    time::OffsetDateTime,
);

fn map(row: Row) -> Result<Scene, AppError> {
    let (id, home_id, name, icon, actions, created_at) = row;
    let actions: Vec<SceneAction> = serde_json::from_value(actions)
        .map_err(|e| AppError::Internal(format!("stored scene actions are invalid: {e}")))?;

    Ok(Scene {
        id,
        home_id,
        name,
        icon,
        actions,
        created_at,
    })
}

const COLUMNS: &str = "id, home_id, name, icon, actions, created_at";

impl SceneRepo for PgScenes {
    async fn create(&self, home_id: Uuid, user_id: Uuid, scene: &Scene) -> Result<(), AppError> {
        let mut tx = self.tx(user_id).await?;

        let actions = serde_json::to_value(&scene.actions)
            .map_err(|e| AppError::Internal(format!("scene actions: {e}")))?;

        // A new scene lands on top, which is where `created_at DESC` used to put it: `MAX + 1` in the same statement, so
        // two scenes created at once cannot both claim the top place.
        sqlx::query(
            r#"INSERT INTO scenes (id, home_id, name, icon, actions, sort_order)
               VALUES ($1, $2, $3, $4, $5,
                       (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM scenes WHERE home_id = $2))"#,
        )
        .bind(scene.id)
        .bind(home_id)
        .bind(&scene.name)
        .bind(&scene.icon)
        .bind(&actions)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)
    }

    async fn list(&self, home_id: Uuid, user_id: Uuid) -> Result<Vec<Scene>, AppError> {
        let mut tx = self.tx(user_id).await?;
        let rows = sqlx::query_as::<_, Row>(&format!(
            // The person's order first (bigger is nearer the top), then the moment it was made: two scenes that share a
            // position — a migrated row, or a home written by two requests at once — still come back in a stable order.
            "SELECT {COLUMNS} FROM scenes WHERE home_id = $1 ORDER BY sort_order DESC, created_at DESC"
        ))
        .bind(home_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        rows.into_iter().map(map).collect()
    }

    async fn get(&self, id: Uuid, user_id: Uuid) -> Result<Option<Scene>, AppError> {
        let mut tx = self.tx(user_id).await?;
        let row = sqlx::query_as::<_, Row>(&format!("SELECT {COLUMNS} FROM scenes WHERE id = $1"))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;

        row.map(map).transpose()
    }

    async fn delete(&self, id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.tx(user_id).await?;
        let n = sqlx::query("DELETE FROM scenes WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        tx.commit().await.map_err(db)?;

        if n == 0 {
            return Err(AppError::not_found("scene"));
        }
        Ok(())
    }

    async fn move_scene(
        &self,
        home_id: Uuid,
        id: Uuid,
        user_id: Uuid,
        up: bool,
    ) -> Result<(), AppError> {
        let mut tx = self.tx(user_id).await?;

        // The list as the screen shows it, then the swap. Reading it in the same transaction as the write is what stops
        // two people reordering the same home from producing an order neither of them asked for.
        let ordered: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM scenes WHERE home_id = $1 ORDER BY sort_order DESC, created_at DESC",
        )
        .bind(home_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let ids: Vec<Uuid> = ordered.into_iter().map(|(id,)| id).collect();
        let Some(at) = ids.iter().position(|candidate| *candidate == id) else {
            return Err(AppError::not_found("scene"));
        };
        let to = if up {
            at.checked_sub(1)
        } else {
            Some(at + 1).filter(|next| *next < ids.len())
        };

        // Already at the end: not an error, and not a reason to rewrite the table. The screens do not offer the control
        // there, so this is the case where somebody's list changed under them.
        let Some(to) = to else {
            tx.commit().await.map_err(db)?;
            return Ok(());
        };

        let mut reordered = ids.clone();
        reordered.swap(at, to);

        for (position, scene_id) in reordered.iter().enumerate() {
            sqlx::query("UPDATE scenes SET sort_order = $2 WHERE id = $1")
                .bind(scene_id)
                .bind((reordered.len() - position) as i32)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }

        tx.commit().await.map_err(db)
    }

    async fn update(&self, id: Uuid, user_id: Uuid, change: SceneChange) -> Result<(), AppError> {
        if change.is_empty() {
            // Nothing to do is not a failure, but it is also not a write: a PATCH with no fields must not bump a row
            // that a second phone is reading.
            return Ok(());
        }

        let mut tx = self.tx(user_id).await?;

        let actions = match &change.actions {
            Some(list) => Some(
                serde_json::to_value(list)
                    .map_err(|e| AppError::Internal(format!("scene actions: {e}")))?,
            ),
            None => None,
        };

        // `COALESCE` per column rather than a query built by string concatenation: three optional fields would be
        // eight shapes to build, test and get wrong, and the row-level policy already answers "is this yours".
        let n = sqlx::query(
            r#"UPDATE scenes
                  SET name    = COALESCE($2, name),
                      icon    = CASE WHEN $3 THEN $4 ELSE icon END,
                      actions = COALESCE($5, actions)
                WHERE id = $1"#,
        )
        .bind(id)
        .bind(&change.name)
        .bind(change.icon.is_some())
        .bind(change.icon.clone().flatten())
        .bind(&actions)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();

        tx.commit().await.map_err(db)?;

        // Zero rows means either "no such scene" or "not yours" — the policy cannot tell the caller which, and it must
        // not: an id from another home reads as missing, exactly like a delete of one.
        if n == 0 {
            return Err(AppError::not_found("scene"));
        }
        Ok(())
    }
}
