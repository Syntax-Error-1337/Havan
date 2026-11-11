//! Error → HTTP mapping. This is the only place in the tree that knows a
//! `kernel::AppError` becomes a status code.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use kernel::AppError;
use serde_json::json;

pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.0.code_kind();

        // A failure the client cannot fix is worth a log line with the cause.
        if code.status() >= 500 {
            tracing::error!(error = ?self.0, code = code.as_str(), "request failed");
        }

        let mut error = json!({
            "code": code.as_str(),
            "message": self.0.public_message(),
        });
        if let AppError::Coded { details, .. } = &self.0
            && !details.is_empty()
        {
            error["details"] = json!(details);
        }

        let status =
            StatusCode::from_u16(code.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(json!({ "error": error }))).into_response()
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn render(err: AppError) -> (StatusCode, serde_json::Value) {
        let res = ApiError(err).into_response();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn coded_errors_map_to_the_envelope() {
        let (status, body) = render(AppError::not_found("device")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "not_found");
        assert_eq!(body["error"]["message"], "device not found");
    }

    #[tokio::test]
    async fn details_are_included_when_present() {
        let (_, body) = render(AppError::validation("bad").with_detail("field", "level")).await;
        assert_eq!(body["error"]["details"]["field"], "level");
    }

    #[tokio::test]
    async fn internal_errors_are_opaque() {
        let (status, body) = render(AppError::Internal("secret".into())).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"]["message"], "internal error");
    }
}
