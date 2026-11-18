use std::env;
use std::fmt;
use std::marker::PhantomData;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use serde::de::{Error as _, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use ureq::http::{HeaderValue, Uri};

const MAX_BATCH: usize = 16;
const MAX_DIM: usize = 16_384;
const MAX_RESPONSE: u64 = 8 * 1024 * 1024;

// Deliberately do not retain the transport error, request URI, or response
// text, even as an error source: CLI diagnostics include the full chain.
#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum ProviderError {
    #[error("no embeddings endpoint (set KNOWBOX_EMBED_URL)")]
    NoEndpoint,
    #[error(
        "invalid embeddings endpoint (use HTTPS or numeric loopback HTTP, without userinfo or fragment)"
    )]
    InvalidEndpoint,
    #[error("no auth token (set KNOWBOX_API_KEY)")]
    NoToken,
    #[error("invalid KNOWBOX_API_KEY")]
    InvalidToken,
    #[error("embedding request timed out")]
    Timeout,
    #[error("embedding request failed")]
    Transport,
    #[error("embedding provider returned HTTP {0}")]
    Status(u16),
    #[error("embedding response exceeds the size limit")]
    Oversize,
    #[error("invalid embedding response")]
    Response,
    #[error("inconsistent embedding dimensions")]
    Dimension,
    #[error("invalid embedding batch size")]
    BatchSize,
}

fn endpoint(value: &str) -> Result<Uri, ProviderError> {
    use ProviderError::InvalidEndpoint;
    if value.is_empty() {
        return Err(ProviderError::NoEndpoint);
    }
    if value.contains('#') {
        return Err(InvalidEndpoint);
    }
    let uri: Uri = value.parse().map_err(|_| InvalidEndpoint)?;
    let authority = uri.authority().ok_or(InvalidEndpoint)?;
    let host = authority.host();
    if host.is_empty() || authority.as_str().contains(['@', '%', '\\']) {
        return Err(InvalidEndpoint);
    }
    if host != authority.as_str() && authority.port_u16().is_none() {
        return Err(InvalidEndpoint);
    }
    match uri.scheme_str() {
        Some("https") => Ok(uri),
        Some("http") => {
            let host = host
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .unwrap_or(host);
            if host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()) {
                Ok(uri)
            } else {
                Err(InvalidEndpoint)
            }
        }
        _ => Err(InvalidEndpoint),
    }
}

pub(super) fn model() -> String {
    match env::var("KNOWBOX_EMBED_MODEL") {
        Ok(s) if !s.trim().is_empty() => s.trim().to_owned(),
        _ => "text-embedding-v4".into(),
    }
}

pub(super) struct Provider {
    agent: ureq::Agent,
    uri: Uri,
    authorization: HeaderValue,
    model: String,
    timeout: Duration,
}

impl Provider {
    pub(super) fn from_env(model: &str, timeout: Duration) -> Result<Self, ProviderError> {
        let url = env::var("KNOWBOX_EMBED_URL").map_err(|e| match e {
            env::VarError::NotPresent => ProviderError::NoEndpoint,
            env::VarError::NotUnicode(_) => ProviderError::InvalidEndpoint,
        })?;
        let uri = endpoint(url.trim())?;
        let key = env::var("KNOWBOX_API_KEY").map_err(|e| match e {
            env::VarError::NotPresent => ProviderError::NoToken,
            env::VarError::NotUnicode(_) => ProviderError::InvalidToken,
        })?;
        Self::new(uri, key.trim(), model, timeout)
    }

    fn new(uri: Uri, key: &str, model: &str, timeout: Duration) -> Result<Self, ProviderError> {
        if key.is_empty() {
            return Err(ProviderError::NoToken);
        }
        if !key.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(ProviderError::InvalidToken);
        }
        let mut authorization = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| ProviderError::InvalidToken)?;
        authorization.set_sensitive(true);
        let timeout = timeout.min(Duration::from_secs(90));
        let agent = ureq::Agent::config_builder()
            .max_redirects(0)
            .proxy(None)
            .timeout_global(Some(timeout))
            .build()
            .new_agent();
        Ok(Self {
            agent,
            uri,
            authorization,
            model: model.to_owned(),
            timeout,
        })
    }

    pub(super) fn vectors(&self, texts: &[&str]) -> Result<Vec<Vec<f64>>, ProviderError> {
        let start = Instant::now();
        vectors_with(texts, |input| {
            let payload = serde_json::to_vec(&Request {
                model: &self.model,
                input,
            })
            .map_err(|_| ProviderError::Transport)?;
            // One total network budget for the batch AND all status fallbacks.
            let remaining = self.timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(ProviderError::Timeout);
            }
            let mut response = self
                .agent
                .post(self.uri.clone())
                .config()
                .timeout_global(Some(remaining))
                .build()
                .header("Authorization", self.authorization.clone())
                .header("Content-Type", "application/json")
                .send(payload.as_slice())
                .map_err(redact)?;
            if !response.status().is_success() {
                return Err(ProviderError::Status(response.status().as_u16()));
            }
            response
                .body_mut()
                .with_config()
                .limit(MAX_RESPONSE)
                .read_to_vec()
                .map_err(redact)
        })
    }
}

fn redact(e: ureq::Error) -> ProviderError {
    match e {
        ureq::Error::StatusCode(s) => ProviderError::Status(s),
        ureq::Error::Timeout(_) => ProviderError::Timeout,
        ureq::Error::BodyExceedsLimit(_) => ProviderError::Oversize,
        _ => ProviderError::Transport,
    }
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    input: Input<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Input<'a> {
    Batch(&'a [&'a str]),
    Single(&'a str),
}

fn vectors_with(
    texts: &[&str],
    mut send: impl FnMut(Input<'_>) -> Result<Vec<u8>, ProviderError>,
) -> Result<Vec<Vec<f64>>, ProviderError> {
    if !(1..=MAX_BATCH).contains(&texts.len()) {
        return Err(ProviderError::BatchSize);
    }
    match send(Input::Batch(texts)) {
        Ok(bytes) => decode(&bytes, texts.len()),
        Err(ProviderError::Status(400..=599)) if texts.len() > 1 => {
            let mut vectors = Vec::with_capacity(texts.len());
            for text in texts {
                let bytes = send(Input::Single(text))?;
                let mut next = decode(&bytes, 1)?;
                if vectors
                    .first()
                    .map(Vec::len)
                    .is_some_and(|dim| next[0].len() != dim)
                {
                    return Err(ProviderError::Dimension);
                }
                vectors.append(&mut next);
            }
            Ok(vectors)
        }
        Err(e) => Err(e),
    }
}

#[derive(Deserialize)]
struct Response {
    #[serde(deserialize_with = "limited_seq::<_, _, MAX_BATCH>")]
    data: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    #[serde(default, deserialize_with = "index")]
    index: Option<usize>,
    #[serde(deserialize_with = "limited_seq::<_, _, MAX_DIM>")]
    embedding: Vec<f64>,
}

// `null` is not a valid explicit index. Only absence is allowed for a single row.
fn index<'de, D: Deserializer<'de>>(d: D) -> Result<Option<usize>, D::Error> {
    usize::deserialize(d).map(Some)
}

fn limited_seq<'de, D: Deserializer<'de>, T: Deserialize<'de>, const CAP: usize>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    struct Limited<T, const CAP: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const CAP: usize> Visitor<'de> for Limited<T, CAP> {
        type Value = Vec<T>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "an array of up to {CAP} elements")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut result = Vec::new();
            while result.len() < CAP {
                match seq.next_element()? {
                    Some(x) => result.push(x),
                    None => return Ok(result),
                }
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("array too long"));
            }
            Ok(result)
        }
    }
    d.deserialize_seq(Limited::<T, CAP>(PhantomData))
}

fn decode(bytes: &[u8], expected: usize) -> Result<Vec<Vec<f64>>, ProviderError> {
    let Response { mut data } =
        serde_json::from_slice(bytes).map_err(|_| ProviderError::Response)?;
    if data.len() != expected {
        return Err(ProviderError::Response);
    }
    let dim = data.first().ok_or(ProviderError::Response)?.embedding.len();
    if dim == 0 {
        return Err(ProviderError::Dimension);
    }
    data.sort_unstable_by_key(|row| row.index);
    data.into_iter()
        .enumerate()
        .map(|(index, row)| {
            if row.index != Some(index) && !(row.index.is_none() && expected == 1) {
                return Err(ProviderError::Response);
            }
            if row.embedding.len() != dim || !row.embedding.iter().all(|x| x.is_finite()) {
                return Err(ProviderError::Dimension);
            }
            Ok(row.embedding)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_is_allowed_only_for_numeric_loopback() {
        for url in [
            "http://127.0.0.1:8000/embeddings",
            "http://[::1]/embeddings",
            "https://example.invalid/embeddings",
        ] {
            assert_eq!(endpoint(url).map(|uri| uri.to_string()), Ok(url.to_owned()));
        }
        for url in [
            "http://localhost/embeddings",
            "http://192.0.2.1/embeddings",
            "https://user@example.invalid/embeddings",
            "https://example.invalid/#x",
        ] {
            assert_eq!(endpoint(url), Err(ProviderError::InvalidEndpoint));
        }
    }
}
