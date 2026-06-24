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
    let head = client
        .head(config.url.clone())
        .headers(header_map.clone())
        .send()
        .await;

    if let Ok(response) = head
        && response.status().is_success()
    {
        let final_url = response.url().clone();
        let total_bytes = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        let supports_ranges = response
            .headers()
            .get(ACCEPT_RANGES)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.eq_ignore_ascii_case("bytes"))
            .unwrap_or(false);
        if supports_ranges && total_bytes.is_some() {
            return Ok(ResourceProbe {
                final_url,
                total_bytes,
                supports_ranges,
                file_name: file_name_from_headers(response.headers()),
                etag: header_string(response.headers(), "etag"),
                last_modified: header_string(response.headers(), "last-modified"),
            });
        }
    }

    let response = client
        .get(config.url.clone())
        .headers(header_map)
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(network_error)?;
    let final_url = response.url().clone();
    if response.status() == StatusCode::PARTIAL_CONTENT {
        let total_bytes = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_content_range_total);
        return Ok(ResourceProbe {
            final_url,
            total_bytes,
            supports_ranges: total_bytes.is_some(),
            file_name: file_name_from_headers(response.headers()),
            etag: header_string(response.headers(), "etag"),
            last_modified: header_string(response.headers(), "last-modified"),
        });
    }
    if response.status().is_success() {
        return Ok(ResourceProbe {
            final_url,
            total_bytes: response.content_length(),
            supports_ranges: false,
            file_name: file_name_from_headers(response.headers()),
            etag: header_string(response.headers(), "etag"),
            last_modified: header_string(response.headers(), "last-modified"),
        });
    }
    Err(http_status_error(response.status()))
}

fn parse_content_range_total(value: &str) -> Option<u64> {
    let (_, total) = value.split_once('/')?;
    total.parse().ok()
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
    let kind = match status.as_u16() {
        401 | 403 => FluxionErrorKind::Unauthorized,
        404 => FluxionErrorKind::NotFound,
        416 => FluxionErrorKind::RangeNotSupported,
        _ => FluxionErrorKind::HttpStatus,
    };
    FluxionError::new(kind, format!("HTTP status {status}"))
}

pub fn network_error(error: reqwest::Error) -> FluxionError {
    FluxionError::new(FluxionErrorKind::Network, error.to_string())
}
