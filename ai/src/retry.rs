use std::future::Future;
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

/// Send an HTTP request with exponential backoff retry on transient errors.
///
/// `make_request` is called on each attempt. Returns the successful response
/// or the final error after retries are exhausted.
pub async fn retry_request<F, Fut>(make_request: F) -> Result<reqwest::Response, reqwest::Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    let mut attempt = 0u32;
    loop {
        match make_request().await {
            Ok(resp) if is_retryable(resp.status().as_u16()) && attempt < MAX_RETRIES => {
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());
                tokio::time::sleep(delay(attempt, retry_after)).await;
                attempt += 1;
            }
            Ok(resp) => return Ok(resp),
            Err(e) if attempt < MAX_RETRIES => {
                tokio::time::sleep(delay(attempt, None)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
