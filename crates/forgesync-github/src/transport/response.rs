//! Response transport behavior.

use super::*;

pub(super) async fn acquire_request_slot(
    slots: &std::sync::Arc<Semaphore>,
    cancellation: &CancellationToken,
) -> Result<OwnedSemaphorePermit, GitHubError> {
    tokio::select! {
        _ = cancellation.cancelled() => Err(GitHubError::Cancelled),
        permit = slots.clone().acquire_owned() => {
            permit.map_err(|_| GitHubError::ConcurrencyUnavailable)
        }
    }
}

pub(super) async fn classify_api_response(
    response: Response,
    status: StatusCode,
) -> Result<ResponseBody, RequestFailure> {
    let headers = response.headers().clone();
    let retry_after = retry_after_hint(&headers);
    let body = read_error_prefix(response).await.unwrap_or_default();
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN
            && (headers
                .get("x-ratelimit-remaining")
                .and_then(|value| value.to_str().ok())
                == Some("0")
                || body_identifies_rate_limit(&body)));
    let kind = api_failure_kind(status, rate_limited);
    let error = GitHubError::Api {
        status: status.as_u16(),
        kind,
    };
    let retryable = status == StatusCode::TOO_MANY_REQUESTS
        || matches!(status.as_u16(), 500 | 502 | 503 | 504)
        || rate_limited;
    if retryable {
        Err(RequestFailure {
            error,
            retryable: true,
            retry_after,
        })
    } else {
        Err(RequestFailure::terminal(error))
    }
}

pub(super) async fn read_error_prefix(response: Response) -> Result<Vec<u8>, reqwest::Error> {
    read_body_prefix(response, MAX_ERROR_BODY_BYTES).await
}

pub(super) async fn read_body(response: Response, limit: usize) -> Result<Vec<u8>, BodyReadError> {
    let mut response = response;
    if response
        .content_length()
        .is_some_and(|length| length > u64::try_from(limit).unwrap_or(u64::MAX))
    {
        return Err(BodyReadError::TooLarge);
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(BodyReadError::Transport)? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(BodyReadError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) async fn read_body_prefix(
    mut response: Response,
    limit: usize,
) -> Result<Vec<u8>, reqwest::Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let room = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if bytes.len() == limit {
            break;
        }
    }
    Ok(bytes)
}

pub(super) fn redirect_target(
    response: &Response,
    origin: &TrustedOrigin,
) -> Result<Url, GitHubError> {
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok());
    let Some(location) = location else {
        return Err(GitHubError::RedirectRejected);
    };
    let target = response
        .url()
        .join(location)
        .map_err(|_| GitHubError::RedirectRejected)?;
    origin.validate(&target)?;
    Ok(target)
}

pub(super) fn classify_transport_error(error: reqwest::Error) -> RequestFailure {
    if error.is_timeout() {
        RequestFailure::retryable(GitHubError::Timeout, None)
    } else {
        RequestFailure::retryable(GitHubError::Network, None)
    }
}
