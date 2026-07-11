use fluxion_core::{FluxionError, FluxionErrorKind, HeaderPair, HttpTaskConfig, Result};
use reqwest::{
    Client, StatusCode,
    header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, RANGE},
};
use url::Url;

use crate::headers::build_headers;

#[derive(Debug, Clone)]
pub struct ResourceProbe {
    pub final_url: Url,
    pub total_bytes: Option<u64>,
    pub supports_ranges: bool,
    pub file_name: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    Segmented,
    Single,
}

pub async fn probe(
    client: &Client,
    config: &HttpTaskConfig,
    headers: &[HeaderPair],
) -> Result<ResourceProbe> {
    let header_map = build_headers(headers)?;
    // HEAD first for cheap metadata (final URL, length, validators,
    // Content-Disposition). Its Accept-Ranges answer is treated as a hint
    // only — some servers advertise `bytes` and then ignore Range, so
    // segmentation is decided by an actual `bytes=0-0` probe below.
    let mut head_probe: Option<ResourceProbe> = None;
    if let Ok(response) = client
        .head(config.url.clone())
        .headers(header_map.clone())
        .send()
        .await
        && response.status().is_success()
    {
        let total_bytes = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        let claims_ranges = response
            .headers()
            .get(ACCEPT_RANGES)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.eq_ignore_ascii_case("bytes"))
            .unwrap_or(false);
        head_probe = Some(ResourceProbe {
            final_url: response.url().clone(),
            total_bytes,
            supports_ranges: claims_ranges,
            file_name: file_name_from_headers(response.headers()),
            etag: header_string(response.headers(), "etag"),
            last_modified: header_string(response.headers(), "last-modified"),
        });
    }

    let response = client
        .get(config.url.clone())
        .headers(header_map)
        .header(RANGE, "bytes=0-0")
        .send()
        .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            // The Range GET failed at the network level; if HEAD already gave
            // us metadata, fall back to a single-connection download rather
            // than failing the whole task.
            if let Some(mut probe) = head_probe {
                probe.supports_ranges = false;
                return Ok(probe);
            }
            return Err(network_error(error));
        }
    };
    let final_url = response.url().clone();
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE
        && head_probe.as_ref().and_then(|probe| probe.total_bytes) == Some(0)
    {
        let mut probe = head_probe.expect("zero-length fallback requires HEAD metadata");
        probe.final_url = final_url;
        probe.supports_ranges = false;
        return Ok(probe);
    }
    if response.status() == StatusCode::PARTIAL_CONTENT {
        let total_bytes = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_content_range_total)
            .or_else(|| head_probe.as_ref().and_then(|probe| probe.total_bytes));
        return Ok(ResourceProbe {
            final_url,
            total_bytes,
            // Only a confirmed 206 with a determinable length enables
            // segmented download (requirements §4.2).
            supports_ranges: total_bytes.is_some(),
            file_name: file_name_from_headers(response.headers()).or_else(|| {
                head_probe
                    .as_ref()
                    .and_then(|probe| probe.file_name.clone())
            }),
            etag: header_string(response.headers(), "etag")
                .or_else(|| head_probe.as_ref().and_then(|probe| probe.etag.clone())),
            last_modified: header_string(response.headers(), "last-modified").or_else(|| {
                head_probe
                    .as_ref()
                    .and_then(|probe| probe.last_modified.clone())
            }),
        });
    }
    if response.status().is_success() {
        return Ok(ResourceProbe {
            final_url,
            total_bytes: response
                .content_length()
                .or_else(|| head_probe.as_ref().and_then(|probe| probe.total_bytes)),
            supports_ranges: false,
            file_name: file_name_from_headers(response.headers()).or_else(|| {
                head_probe
                    .as_ref()
                    .and_then(|probe| probe.file_name.clone())
            }),
            etag: header_string(response.headers(), "etag")
                .or_else(|| head_probe.as_ref().and_then(|probe| probe.etag.clone())),
            last_modified: header_string(response.headers(), "last-modified").or_else(|| {
                head_probe
                    .as_ref()
                    .and_then(|probe| probe.last_modified.clone())
            }),
        });
    }
    Err(http_status_error(response.status()))
}

fn parse_content_range_total(value: &str) -> Option<u64> {
    let (_, total) = value.split_once('/')?;
    total.trim().parse().ok()
}

/// Parse a `Content-Range: bytes <start>-<end>/<total>` header into
/// `(start, end, total)`. `total` is None for `*` (unknown length).
pub fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.trim().split_once('-')?;
    let start = start.trim().parse().ok()?;
    let end = end.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        value => Some(value.parse().ok()?),
    };
    Some((start, end, total))
}

fn file_name_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let value = headers
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())?;
    value.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix("filename=")
            .map(|name| name.trim_matches('"').to_string())
    })
}

fn header_string(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

pub fn http_status_error(status: StatusCode) -> FluxionError {
    // Only 5xx and 429 are transient (design §4.6); other 4xx are permanent
    // and mapped to non-retryable kinds so the retry loop skips them.
    let kind = match status.as_u16() {
        401 | 403 => FluxionErrorKind::Unauthorized,
        404 | 410 => FluxionErrorKind::NotFound,
        416 => FluxionErrorKind::RangeNotSupported,
        429 => FluxionErrorKind::HttpStatus,
        code if code >= 500 => FluxionErrorKind::HttpStatus,
        _ => FluxionErrorKind::InvalidConfig,
    };
    FluxionError::new(kind, format!("HTTP status {status}"))
}

pub fn network_error(error: reqwest::Error) -> FluxionError {
    FluxionError::new(FluxionErrorKind::Network, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_parses() {
        assert_eq!(
            parse_content_range("bytes 0-0/1234"),
            Some((0, 0, Some(1234)))
        );
        assert_eq!(
            parse_content_range("bytes 100-199/*"),
            Some((100, 199, None))
        );
        assert_eq!(
            parse_content_range("bytes 5-9 / 20"),
            Some((5, 9, Some(20)))
        );
        assert_eq!(parse_content_range("garbage"), None);
        assert_eq!(parse_content_range("bytes */1234"), None);
    }

    #[test]
    fn status_classification() {
        assert_eq!(
            http_status_error(StatusCode::TOO_MANY_REQUESTS).kind(),
            FluxionErrorKind::HttpStatus
        );
        assert_eq!(
            http_status_error(StatusCode::BAD_GATEWAY).kind(),
            FluxionErrorKind::HttpStatus
        );
        assert_eq!(
            http_status_error(StatusCode::BAD_REQUEST).kind(),
            FluxionErrorKind::InvalidConfig
        );
        assert_eq!(
            http_status_error(StatusCode::GONE).kind(),
            FluxionErrorKind::NotFound
        );
        assert_eq!(
            http_status_error(StatusCode::UNAUTHORIZED).kind(),
            FluxionErrorKind::Unauthorized
        );
    }
}
