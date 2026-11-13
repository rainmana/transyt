//! Native HTTPS adapter. Redirects are disabled to preserve destination validation.
use crate::Http;
use serde_json::Value;

/// Reusable Rustls HTTP client; no system OpenSSL installation is required.
pub struct Client(reqwest::Client);

impl Client {
    pub fn new() -> Result<Self, reqwest::Error> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("transyt/0.1")
            .build()
            .map(Self)
    }
}

#[async_trait::async_trait]
impl Http for Client {
    async fn request(&self, url: &str, body: Option<Value>) -> Result<String, String> {
        let request = match body {
            Some(body) => self.0.post(url).json(&body),
            None => self.0.get(url),
        };
        let response = request
            .send()
            .await
            .map_err(|e| format!("YouTube network error: {e}"))?;
        if response.status().is_redirection() {
            return Err(
                "YouTube redirected the request; consent or authentication may be required".into(),
            );
        }
        response
            .error_for_status()
            .map_err(|e| format!("YouTube HTTP error (possibly IP blocking): {e}"))?
            .text()
            .await
            .map_err(|e| e.to_string())
    }
}
