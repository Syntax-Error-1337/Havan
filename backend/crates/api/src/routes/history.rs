use axum::Json;
use axum::extract::{Path, Query, State};
use domain::history::{History, HistoryRepo, bucket_secs, range_secs};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};

#[derive(Deserialize)]
pub struct HistoryQuery {
    /// The datapoint to chart, e.g. `switch.power`.
    code: String,
    /// `1h`, `1d`, `7d`, `30d`. Anything else is refused rather than trusted.
    #[serde(default = "default_range")]
    range: String,
}

fn default_range() -> String {
    "1d".to_string()
}

/// Downsampled history for one datapoint.
///
/// Membership is checked by asking the device service for the device: it is the
/// same check the detail screen already passes, and doing it here means a
/// non-member gets 404 rather than an empty chart they might read as "no data".
pub async fn history(
    State(state): State<AppState>,
    auth: Auth,
    Path(device_id): Path<Uuid>,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<History>> {
    let device = state
        .devices
        .get(device_id, auth.user.id)
        .await
        .map_err(ApiError)?;

    let point = device
        .points
        .iter()
        .find(|p| p.code == q.code)
        .ok_or_else(|| ApiError(kernel::AppError::not_found("datapoint")))?;

    let window = range_secs(&q.range).ok_or_else(|| {
        ApiError(kernel::AppError::validation(
            "range must be one of 1h, 1d, 7d, 30d",
        ))
    })?;
    let bucket = bucket_secs(window);

    let buckets = state
        .history
        .history(device_id, auth.user.id, point.id, window, bucket)
        .await
        .map_err(ApiError)?;

    let now = kernel::now();
    Ok(Json(History {
        device_id,
        code: q.code,
        from: now - time::Duration::seconds(window),
        to: now,
        bucket_secs: bucket,
        buckets,
    }))
}
