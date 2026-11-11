use domain::scenes::{Scene, SceneAction, SceneRepo};
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

        sqlx::query(
            r#"INSERT INTO scenes (id, home_id, name, icon, actions)
               VALUES ($1, $2, $3, $4, $5)"#,
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
            "SELECT {COLUMNS} FROM scenes WHERE home_id = $1 ORDER BY created_at DESC"
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
}
