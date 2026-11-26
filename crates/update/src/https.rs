//! HTTPS to GitHub (Windows): reqwest's blocking client, rustls with the Windows certificate
//! store. Every request and every redirect is checked against [`crate::fetch::allowed`]; the
//! requests carry the User-Agent and nothing else of ours (no Referer, no cookies, no tokens).

use std::io::Write;
use std::time::Duration;

use crate::fetch::{FetchError, Transport, allowed, copy_limited};

/// How long connecting may take.
const CONNECT: Duration = Duration::from_secs(15);
/// How long a small answer (the release JSON, SHA256SUMS.txt) may take in all.
const SMALL: Duration = Duration::from_secs(30);
/// How long the installer's download may take in all.
const LARGE: Duration = Duration::from_secs(20 * 60);
/// Redirects followed at most (GitHub's downloads take one or two).
const MAX_REDIRECTS: usize = 5;
/// Above this limit a request counts as a download and gets [`LARGE`].
const SMALL_LIMIT: u64 = 1 << 20;

pub struct HttpsTransport {
    client: reqwest::blocking::Client,
}

impl HttpsTransport {
    pub fn new(user_agent: &str) -> Result<HttpsTransport, FetchError> {
        let policy = reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                return attempt.error("too many redirects");
            }
            match url_allowed(attempt.url()) {
                Ok(()) => attempt.follow(),
                Err(e) => attempt.error(e),
            }
        });
        let client = base(user_agent).https_only(true).redirect(policy).build().map_err(|e| FetchError::Network(e.to_string()))?;
        Ok(HttpsTransport { client })
    }
}

/// The client settings that decide what a request carries: the User-Agent, no Referer, and
/// reqwest's own defaults otherwise. [`HttpsTransport::new`] adds HTTPS only and the redirect
/// check; the header test (`tests.rs`) sends with exactly these settings to a local server.
pub(crate) fn base(user_agent: &str) -> reqwest::blocking::ClientBuilder {
    reqwest::blocking::Client::builder().user_agent(user_agent).referer(false).connect_timeout(CONNECT)
}

/// [`allowed`] for a parsed address, which also rules out a user name, a password and a port
/// other than 443 however they are written. The path checked is the parsed one (dot segments
/// already resolved), so a redirect within `github.com` stays under this repository's release
/// downloads too.
pub(crate) fn url_allowed(url: &reqwest::Url) -> Result<(), String> {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() || url.port_or_known_default() != Some(443) {
        return Err(format!("{url} is not an allowed address (HTTPS to GitHub only)"));
    }
    let host = url.host_str().unwrap_or_default();
    allowed(&format!("https://{host}{}", url.path())).map_err(|_| format!("{url} is not an allowed address (HTTPS to GitHub only)"))
}

impl Transport for HttpsTransport {
    fn get(&self, url: &str, limit: u64, sink: &mut dyn Write) -> Result<u64, FetchError> {
        allowed(url).map_err(FetchError::Refused)?;
        let parsed = reqwest::Url::parse(url).map_err(|e| FetchError::Refused(format!("{url}: {e}")))?;
        url_allowed(&parsed).map_err(FetchError::Refused)?;
        let timeout = if limit > SMALL_LIMIT { LARGE } else { SMALL };
        let response = self
            .client
            .get(parsed)
            .timeout(timeout)
            .send()
            .map_err(|e| if e.is_redirect() { FetchError::Refused(e.to_string()) } else { FetchError::Network(e.to_string()) })?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(FetchError::Status(status));
        }
        if response.content_length().is_some_and(|n| n > limit) {
            return Err(FetchError::TooLarge);
        }
        copy_limited(response, limit, sink)
    }
}
