//! Retry transport behavior.

use super::{
    ApiFailureKind, Duration, RETRY_AFTER, RetryPolicy, StatusCode, SystemTime, UNIX_EPOCH,
};

pub(super) fn api_failure_kind(status: StatusCode, rate_limited: bool) -> ApiFailureKind {
    if rate_limited {
        return ApiFailureKind::RateLimited;
    }
    match status {
        StatusCode::UNAUTHORIZED => ApiFailureKind::AuthenticationRequired,
        StatusCode::FORBIDDEN => ApiFailureKind::PermissionDenied,
        StatusCode::NOT_FOUND => ApiFailureKind::NotFound,
        StatusCode::CONFLICT => ApiFailureKind::Conflict,
        status if status.is_server_error() => ApiFailureKind::Server,
        _ => ApiFailureKind::Client,
    }
}

pub(super) fn body_identifies_rate_limit(body: &[u8]) -> bool {
    let body = String::from_utf8_lossy(body).to_ascii_lowercase();
    [
        "api rate limit exceeded",
        "secondary rate limit",
        "abuse detection",
    ]
    .iter()
    .any(|marker| body.contains(marker))
}

pub(super) fn retry_after_hint(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    if let Some(value) = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
    {
        if let Ok(seconds) = value.trim().parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
        if let Ok(retry_at) = httpdate::parse_http_date(value) {
            return Some(
                retry_at
                    .duration_since(SystemTime::now())
                    .unwrap_or(Duration::ZERO),
            );
        }
    }

    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok());
    if remaining != Some("0") {
        return None;
    }
    let reset = headers
        .get("x-ratelimit-reset")
        .and_then(|value| value.to_str().ok())?
        .parse::<u64>()
        .ok()?;
    let retry_at = UNIX_EPOCH.checked_add(Duration::from_secs(reset))?;
    Some(
        retry_at
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO),
    )
}

pub(super) fn retry_backoff(policy: &RetryPolicy, retry_index: u32) -> Duration {
    let multiplier = 1_u32.checked_shl(retry_index.min(31)).unwrap_or(u32::MAX);
    policy
        .initial_backoff
        .saturating_mul(multiplier)
        .min(policy.max_backoff)
}
