use fluxion_core::{FluxionError, FluxionErrorKind, HeaderPair, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

pub fn build_headers(headers: &[HeaderPair]) -> Result<HeaderMap> {
    let mut map = HeaderMap::new();
    for header in headers {
        let name = HeaderName::from_bytes(header.name.as_bytes()).map_err(|error| {
            FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                format!("invalid header name {}: {error}", header.name),
            )
        })?;
        let value = HeaderValue::from_str(&header.value).map_err(|error| {
            FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                format!("invalid value for header {}: {error}", header.name),
            )
        })?;
        // `append`, not `insert`: repeated header names (multiple Cookie or
        // custom headers) must all survive; `insert` silently kept only the
        // last occurrence.
        map.append(name, value);
    }
    Ok(map)
}
