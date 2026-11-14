//! Logging setup. JSON in production, human-readable in dev, and one filter
//! string that every service uses so a support engineer can grep one way.

use crate::config::Env;
use std::io::IsTerminal;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Install the global subscriber. Idempotent: a second call is a no-op, which
/// matters because integration tests boot several services in one process.
pub fn init(env: &Env) {
    let filter = EnvFilter::try_from_env("HAVEN_LOG")
        .unwrap_or_else(|_| EnvFilter::new(default_filter(env)));

    let registry = tracing_subscriber::registry().with(filter);
    if env.is_prod() {
        let _ = registry.with(fmt::layer().json()).try_init();
    } else {
        // Colour only when a person is watching. Piped into a file or a container log it is
        // escape bytes in the middle of every field name — which is how a drill script ends up
        // grepping for `redelivered` and finding nothing, because the space is an escape code.
        let _ = registry
            .with(
                fmt::layer()
                    .compact()
                    .with_ansi(std::io::stdout().is_terminal()),
            )
            .try_init();
    }
}

fn default_filter(env: &Env) -> &'static str {
    match env {
        Env::Prod => "info,hyper=warn,sqlx=warn",
        Env::Dev | Env::Test => "info,haven=debug,sqlx=info",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prod_is_quieter_than_dev() {
        assert!(default_filter(&Env::Prod).starts_with("info"));
        assert!(default_filter(&Env::Dev).contains("haven=debug"));
    }
}
