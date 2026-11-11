//! The one error type every layer returns, and the stable codes clients switch
//! on. `api` maps this to HTTP; nothing else needs to know about status codes.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // 400
    BadRequest,
    // 401
    Unauthorized,
    InvalidCredentials,
    TokenExpired,
    // 403
    Forbidden,
    // 404
    NotFound,
    // 409
    Conflict,
    AlreadyClaimed,
    // 422
    ValidationFailed,
    UnknownPoint,
    ValueOutOfRange,
    // 429
    RateLimited,
    // 500
    Internal,
    // 503
    Unavailable,
    // 504
    Timeout,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::BadRequest => "bad_request",
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::InvalidCredentials => "invalid_credentials",
            ErrorCode::TokenExpired => "token_expired",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::NotFound => "not_found",
            ErrorCode::Conflict => "conflict",
            ErrorCode::AlreadyClaimed => "already_claimed",
            ErrorCode::ValidationFailed => "validation_failed",
            ErrorCode::UnknownPoint => "unknown_point",
            ErrorCode::ValueOutOfRange => "value_out_of_range",
            ErrorCode::RateLimited => "rate_limited",
            ErrorCode::Internal => "internal",
            ErrorCode::Unavailable => "unavailable",
            ErrorCode::Timeout => "timeout",
        }
    }

    pub fn status(self) -> u16 {
        match self {
            ErrorCode::BadRequest => 400,
            ErrorCode::Unauthorized | ErrorCode::InvalidCredentials | ErrorCode::TokenExpired => {
                401
            }
            ErrorCode::Forbidden => 403,
            ErrorCode::NotFound => 404,
            ErrorCode::Conflict | ErrorCode::AlreadyClaimed => 409,
            ErrorCode::ValidationFailed | ErrorCode::UnknownPoint | ErrorCode::ValueOutOfRange => {
                422
            }
            ErrorCode::RateLimited => 429,
            ErrorCode::Internal => 500,
            ErrorCode::Unavailable => 503,
            ErrorCode::Timeout => 504,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Coded {
        code: ErrorCode,
        message: String,
        details: BTreeMap<String, String>,
    },

    #[error("database error")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("cache error")]
    Cache(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("broker error")]
    Broker(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn code(code: ErrorCode, message: impl Into<String>) -> Self {
        AppError::Coded {
            code,
            message: message.into(),
            details: BTreeMap::new(),
        }
    }

    pub fn not_found(what: impl Into<String>) -> Self {
        Self::code(ErrorCode::NotFound, format!("{} not found", what.into()))
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::code(ErrorCode::Forbidden, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::code(ErrorCode::ValidationFailed, message)
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        if let AppError::Coded { details, .. } = &mut self {
            details.insert(key.into(), value.into());
        }
        self
    }

    pub fn code_kind(&self) -> ErrorCode {
        match self {
            AppError::Coded { code, .. } => *code,
            AppError::Database(_) | AppError::Cache(_) | AppError::Broker(_) => {
                ErrorCode::Unavailable
            }
            AppError::Internal(_) => ErrorCode::Internal,
        }
    }

    /// The message safe to return to a client. Internal errors never leak.
    pub fn public_message(&self) -> String {
        match self {
            AppError::Coded { message, .. } => message.clone(),
            AppError::Database(_) | AppError::Cache(_) | AppError::Broker(_) => {
                "a dependency is unavailable".to_string()
            }
            AppError::Internal(_) => "internal error".to_string(),
        }
    }
}

/// Any boxed source error becomes a dependency-class failure; callers that
/// know better attach a code with [`AppError::code`].
impl From<Box<dyn std::error::Error + Send + Sync>> for AppError {
    fn from(e: Box<dyn std::error::Error + Send + Sync>) -> Self {
        AppError::Database(e)
    }
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_map_to_statuses() {
        assert_eq!(ErrorCode::NotFound.status(), 404);
        assert_eq!(ErrorCode::ValidationFailed.status(), 422);
        assert_eq!(ErrorCode::AlreadyClaimed.status(), 409);
        assert_eq!(ErrorCode::Timeout.status(), 504);
    }

    #[test]
    fn internal_errors_do_not_leak_their_message() {
        let e = AppError::Internal("connection string is postgres://u:p@host".into());
        assert_eq!(e.public_message(), "internal error");
        assert_eq!(e.code_kind().as_str(), "internal");
    }

    #[test]
    fn details_attach_only_to_coded_errors() {
        let e = AppError::validation("bad").with_detail("field", "level");
        match e {
            AppError::Coded { details, .. } => assert_eq!(details["field"], "level"),
            _ => panic!("expected a coded error"),
        }
    }
}
