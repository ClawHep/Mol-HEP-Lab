//! Exponential backoff retry logic for LLM API calls.

use std::time::Duration;

use anyhow::{Result, anyhow};
use reqwest::Response;
use tokio::time::sleep;
use tracing::{info, warn};

/// Keywords in a 400 response body that indicate a transient/overload error
/// (as opposed to a genuine bad-request that should not be retried).
const TRANSIENT_400_KEYWORDS: &[&str] = &[
    "rate limit",
    "ratelimit",
    "overloaded",
    "temporarily",
    "capacity",
    "throttl",
    "too many",
    "retry",
];

/// HTTP status codes that are always retryable (excluding 400, which requires
/// body inspection).
const RETRYABLE_STATUSES: &[u16] = &[429, 500, 502, 503, 504, 529];

/// HTTP status codes that are never retried regardless of body content.
const NON_RETRYABLE_STATUSES: &[u16] = &[401, 403, 404];

/// Determine whether a response status + body is retryable.
pub fn is_retryable(status: u16, body: &str) -> bool {
    if NON_RETRYABLE_STATUSES.contains(&status) {
        return false;
    }
    if RETRYABLE_STATUSES.contains(&status) {
        return true;
    }
    if status == 400 {
        let lower = body.to_lowercase();
        return TRANSIENT_400_KEYWORDS.iter().any(|kw| lower.contains(kw));
    }
    false
}

/// Compute the delay for attempt `n` (zero-indexed) using exponential backoff
/// with up to 30% random jitter.
///
/// `base_delay` is the delay before the first retry. Subsequent attempts
/// double the delay: base, 2×base, 4×base, …
pub fn backoff_delay(attempt: u32, base_delay: Duration) -> Duration {
    let multiplier = 2u64.saturating_pow(attempt);
    let base_ms = base_delay.as_millis() as u64;
    let delay_ms = base_ms.saturating_mul(multiplier);

    // Add jitter: up to 30% of the computed delay
    let jitter_ms = (delay_ms as f64 * 0.3 * rand_fraction()) as u64;
    Duration::from_millis(delay_ms + jitter_ms)
}

/// Returns a pseudo-random fraction in [0, 1) using the current timestamp
/// as a seed (no external RNG crate needed).
fn rand_fraction() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(12345);
    // Mix bits with a simple xorshift-derived constant to spread distribution
    let mixed = nanos.wrapping_mul(2654435761) ^ nanos.wrapping_shr(16);
    (mixed as f64) / (u32::MAX as f64)
}

/// Call the async factory `f` with retry and exponential backoff.
///
/// `f` is called with the attempt index (0-based). On a non-2xx response
/// the body is read to determine whether the error is retryable.
///
/// Returns the first successful [`Response`], or the last error after
/// `max_retries + 1` total attempts.
pub async fn call_with_retry<F, Fut>(
    f: F,
    max_retries: u32,
    base_delay: Duration,
) -> Result<Response>
where
    F: Fn(u32) -> Fut,
    Fut: std::future::Future<Output = Result<reqwest::Response>>,
{
    let mut last_err: Option<anyhow::Error> = None;
    let total_attempts = max_retries + 1;

    for attempt in 0..total_attempts {
        match f(attempt).await {
            Ok(response) => {
                let status = response.status().as_u16();

                if response.status().is_success() {
                    return Ok(response);
                }

                // Read body to inspect transient-400 keywords
                let body_bytes = response.bytes().await.unwrap_or_default();
                let body = String::from_utf8_lossy(&body_bytes);
                let body_preview = &body[..body.len().min(500)];

                if status == 400 {
                    info!("[LLM 400] body={}", body_preview);
                }

                if is_retryable(status, body_preview) && attempt < max_retries {
                    let delay = backoff_delay(attempt, base_delay);
                    info!(
                        attempt = attempt + 1,
                        total = total_attempts,
                        status,
                        delay_ms = delay.as_millis(),
                        "Retryable HTTP error, backing off"
                    );
                    sleep(delay).await;
                    last_err = Some(anyhow!("HTTP {status}: {body_preview}"));
                    continue;
                }

                return Err(anyhow!("HTTP {status}: {body_preview}"));
            }
            Err(e) => {
                if attempt < max_retries {
                    let delay = backoff_delay(attempt, base_delay);
                    warn!(
                        attempt = attempt + 1,
                        total = total_attempts,
                        error = %e,
                        delay_ms = delay.as_millis(),
                        "Request error, backing off"
                    );
                    sleep(delay).await;
                    last_err = Some(e);
                    continue;
                }
                return Err(e);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow!("All retry attempts exhausted")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_400_detected() {
        assert!(is_retryable(400, "rate limit exceeded"));
        assert!(is_retryable(400, "Too Many Requests - throttled"));
        assert!(!is_retryable(400, "invalid model id"));
    }

    #[test]
    fn non_retryable_statuses() {
        assert!(!is_retryable(401, ""));
        assert!(!is_retryable(403, "not allowed to use model"));
        assert!(!is_retryable(404, "not found"));
    }

    #[test]
    fn retryable_statuses() {
        assert!(is_retryable(429, ""));
        assert!(is_retryable(500, ""));
        assert!(is_retryable(502, ""));
        assert!(is_retryable(503, ""));
        assert!(is_retryable(504, ""));
        assert!(is_retryable(529, ""));
    }

    #[test]
    fn backoff_increases() {
        let base = Duration::from_millis(1000);
        let d0 = backoff_delay(0, base);
        let d1 = backoff_delay(1, base);
        let d2 = backoff_delay(2, base);
        // Each step is at least 2× the previous (jitter only adds, never subtracts)
        assert!(d1 >= Duration::from_millis(2000));
        assert!(d2 >= Duration::from_millis(4000));
        // And bounded: jitter is at most 30%, so d0 < 1300ms
        assert!(d0 <= Duration::from_millis(1300));
    }
}
