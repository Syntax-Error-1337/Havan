//! Redis: presence, hot shadow values, rate-limit counters, pub/sub fan-out.
//!
//! Nothing here is the source of truth. If Redis is lost the platform degrades
//! (presence becomes unknown, counters reset) but no device state is corrupted,
//! because every write is also a Postgres row.

use kernel::AppError;
use redis::aio::ConnectionManager;
use std::time::Duration;

#[derive(Clone)]
pub struct Cache {
    conn: ConnectionManager,
}

impl Cache {
    /// A cache that is *gone* must fail in a second or two, not hang the request that
    /// touched it. Observed in the chaos drill: with Valkey stopped, a login request never
    /// answered at all — the client waited on a TCP connect that would never complete, and
    /// the caller's deadline expired instead of the cache saying "no".
    ///
    /// Both timeouts are set on the connection manager so every command is bounded, not
    /// only the ones somebody remembered to wrap.
    pub async fn connect(url: &str) -> Result<Self, AppError> {
        let client = redis::Client::open(url).map_err(|e| AppError::Cache(Box::new(e)))?;
        let config = redis::aio::ConnectionManagerConfig::new()
            .set_connection_timeout(Duration::from_secs(2))
            .set_response_timeout(Duration::from_secs(2));
        let conn = client
            .get_connection_manager_with_config(config)
            .await
            .map_err(|e| AppError::Cache(Box::new(e)))?;
        Ok(Self { conn })
    }

    pub async fn ping(&self) -> bool {
        let mut conn = self.conn.clone();
        match tokio::time::timeout(
            Duration::from_secs(2),
            redis::cmd("PING").query_async::<String>(&mut conn),
        )
        .await
        {
            Ok(result) => result.is_ok(),
            Err(_) => false,
        }
    }

    /// Mark a device online with a TTL. The TTL is the mechanism: a device that
    /// stops publishing simply expires, so a crashed worker cannot leave a
    /// device "online" forever.
    pub async fn mark_online(&self, device_id: &str, ttl: Duration) -> Result<(), AppError> {
        let mut conn = self.conn.clone();
        redis::cmd("SET")
            .arg(format!("presence:{device_id}"))
            .arg("1")
            .arg("PX")
            .arg(ttl.as_millis() as u64)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| AppError::Cache(Box::new(e)))
    }

    pub async fn is_online(&self, device_id: &str) -> Result<bool, AppError> {
        let mut conn = self.conn.clone();
        let v: Option<String> = redis::cmd("GET")
            .arg(format!("presence:{device_id}"))
            .query_async(&mut conn)
            .await
            .map_err(|e| AppError::Cache(Box::new(e)))?;
        Ok(v.is_some())
    }

    /// Publish an event to every API instance. The worker never needs a socket
    /// to every pod; it publishes once and whoever holds the socket forwards.
    pub async fn publish_event(&self, channel: &str, payload: &str) -> Result<(), AppError> {
        let mut conn = self.conn.clone();
        redis::cmd("PUBLISH")
            .arg(channel)
            .arg(payload)
            .query_async::<i64>(&mut conn)
            .await
            .map(|_| ())
            .map_err(|e| AppError::Cache(Box::new(e)))
    }

    /// Fixed-window counter. Good enough for abuse control; the window boundary
    /// burst is bounded by the caller's limit.
    /// How many hits a window already holds, without counting one.
    ///
    /// Used by the limiter's check-then-record path: a wrong password must not count
    /// against the limit until it has been *proven* wrong.
    pub async fn peek(&self, key: &str) -> Result<u64, AppError> {
        let mut conn = self.conn.clone();
        redis::cmd("GET")
            .arg(key)
            .query_async::<Option<u64>>(&mut conn)
            .await
            .map(|v| v.unwrap_or(0))
            .map_err(|e| AppError::Cache(Box::new(e)))
    }

    pub async fn incr_with_ttl(&self, key: &str, window: Duration) -> Result<u64, AppError> {
        let mut conn = self.conn.clone();
        let script = redis::Script::new(
            r"
            local n = redis.call('INCR', KEYS[1])
            if n == 1 then redis.call('PEXPIRE', KEYS[1], ARGV[1]) end
            return n
            ",
        );
        script
            .key(key)
            .arg(window.as_millis() as u64)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| AppError::Cache(Box::new(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn presence_expires() {
        let Ok(url) = std::env::var("TEST_REDIS_URL") else {
            eprintln!("skipping: TEST_REDIS_URL not set");
            return;
        };
        let cache = Cache::connect(&url).await.expect("connect");
        let id = format!("test-{}", kernel::new_id());
        cache
            .mark_online(&id, Duration::from_millis(150))
            .await
            .unwrap();
        assert!(cache.is_online(&id).await.unwrap());
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(!cache.is_online(&id).await.unwrap());
    }

    #[tokio::test]
    async fn rate_limit_counter_starts_at_one() {
        let Ok(url) = std::env::var("TEST_REDIS_URL") else {
            eprintln!("skipping: TEST_REDIS_URL not set");
            return;
        };
        let cache = Cache::connect(&url).await.expect("connect");
        let key = format!("rl:{}", kernel::new_id());
        assert_eq!(
            cache
                .incr_with_ttl(&key, Duration::from_secs(1))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            cache
                .incr_with_ttl(&key, Duration::from_secs(1))
                .await
                .unwrap(),
            2
        );
    }
}
