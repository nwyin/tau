use std::time::Duration;

pub fn is_retryable(status: u16) -> bool {
    matches!(status, 429 | 529 | 500 | 502 | 503)
}

pub fn delay(attempt: u32, retry_after: Option<u64>) -> Duration {
    if let Some(secs) = retry_after {
        return Duration::from_secs(secs.min(120));
    }
    Duration::from_secs(2u64.pow(attempt).min(60))
}

pub const MAX_RETRIES: u32 = 3;
