use anyhow::{bail, Context, Result};
use reqwest::{Client, Response};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use url::Url;

pub const DEFAULT_PROBE: &str = "http://www.gstatic.com/generate_204";
const MAX_BODY: usize = 256 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub username: String,
    pub interval_seconds: u64,
    pub auto_connect: bool,
    pub start_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            username: String::new(),
            interval_seconds: 15,
            auto_connect: false,
            start_at_login: false,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !(5..=3600).contains(&self.interval_seconds) {
            bail!("Check interval must be between 5 and 3600 seconds");
        }
        if self.username.len() > 256 {
            bail!("Account is too long");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Online,
    NeedsLogin,
    Unstable,
    Checking,
    Authenticating,
    Stopped,
    MissingCredentials,
    LoginFailed,
}

#[derive(Clone)]
pub struct Engine {
    client: Client,
    submit_client: Client,
    probe: Url,
}

struct Page {
    url: Url,
    code: u16,
    body: String,
}

impl Engine {
    pub fn new() -> Result<Self> {
        Self::with_probe(DEFAULT_PROBE)
    }

    pub fn with_probe(probe: &str) -> Result<Self> {
        let probe = Url::parse(probe)?;
        validate_http(&probe)?;
        let cookies = Arc::new(reqwest::cookie::Jar::default());
        let client = Client::builder()
            .cookie_provider(cookies.clone())
            .no_proxy()
            .timeout(Duration::from_secs(12))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("NCUT-Auto-Login/4.0")
            .build()?;
        let submit_client = Client::builder()
            .cookie_provider(cookies)
            .no_proxy()
            .timeout(Duration::from_secs(12))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    attempt.error("Too many redirects")
                } else if attempt
                    .previous()
                    .first()
                    .is_some_and(|url| url.origin() != attempt.url().origin())
                {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .user_agent("NCUT-Auto-Login/4.0")
            .build()?;
        Ok(Self {
            client,
            submit_client,
            probe,
        })
    }

    async fn page(&self, mut response: Response) -> Result<Page> {
        let url = response.url().clone();
        let code = response.status().as_u16();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > MAX_BODY {
                bail!("Portal response is too large");
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(Page {
            url,
            code,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }

    async fn probe_page(&self) -> Result<Page> {
        self.page(self.client.get(self.probe.clone()).send().await?)
            .await
    }

    pub async fn check(&self) -> Status {
        match self.probe_page().await {
            Ok(page) if page.code == 204 => Status::Online,
            Ok(page) if portal_url(&page.url, &page.body).is_some() => Status::NeedsLogin,
            _ => Status::Unstable,
        }
    }

    pub async fn connect(&self, username: &str, password: &str) -> Result<Status> {
        if username.trim().is_empty() || password.is_empty() {
            return Ok(Status::MissingCredentials);
        }
        let initial = self.probe_page().await.context("Network check failed")?;
        if initial.code == 204 {
            return Ok(Status::Online);
        }
        let portal = portal_url(&initial.url, &initial.body).context("No Fortinet portal found")?;
        validate_http(&portal)?;
        let page = self
            .page(self.client.get(portal.clone()).send().await?)
            .await?;
        validate_http(&page.url)?;
        if page.url.origin() != portal.origin() {
            bail!("Portal changed origin");
        }
        let (target, magic) = login_form(&page.url, &page.body)?;
        let origin = page.url.origin().ascii_serialization();
        let response = self
            .submit_client
            .post(target)
            .header("Origin", origin)
            .header("Referer", page.url.as_str())
            .form(&[
                ("4Tredir", self.probe.as_str()),
                ("magic", magic.as_str()),
                ("username", username),
                ("password", password),
            ])
            .send()
            .await?;
        let result = self.page(response).await?;
        if !(200..300).contains(&result.code)
            || !result.body.to_ascii_lowercase().contains("/keepalive?")
        {
            return Ok(Status::LoginFailed);
        }
        Ok(if self.check().await == Status::Online {
            Status::Online
        } else {
            Status::LoginFailed
        })
    }
}

fn validate_http(url: &Url) -> Result<()> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("Invalid portal URL");
    }
    Ok(())
}

fn portal_url(base: &Url, body: &str) -> Option<Url> {
    if base.path() == "/fgtauth" && base.query().is_some() {
        return Some(base.clone());
    }
    let pattern =
        regex::Regex::new(r#"(?:window\.)?location(?:\.href)?\s*=\s*["']([^"']+)["']"#).ok()?;
    let capture = pattern.captures(body)?;
    let url = base.join(&capture[1].replace("&amp;", "&")).ok()?;
    (url.path() == "/fgtauth" && url.query().is_some()).then_some(url)
}

fn login_form(base: &Url, body: &str) -> Result<(Url, String)> {
    let html = Html::parse_document(body);
    let title = Selector::parse("title").unwrap();
    let title = html
        .select(&title)
        .next()
        .map(|e| e.text().collect::<String>())
        .unwrap_or_default();
    if !title.contains("勤益科技大學") {
        bail!("Portal is not an NCUT login page");
    }
    let forms = Selector::parse("form").unwrap();
    let inputs = Selector::parse("input").unwrap();
    for form in html.select(&forms) {
        let fields: Vec<_> = form.select(&inputs).collect();
        if !["username", "password"]
            .iter()
            .all(|name| fields.iter().any(|e| e.value().attr("name") == Some(name)))
        {
            continue;
        }
        let magic = fields
            .iter()
            .find(|e| e.value().attr("name") == Some("magic"))
            .and_then(|e| e.value().attr("value"))
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                base.query()
                    .map(|s| s.split('&').next().unwrap_or_default().to_owned())
            })
            .filter(|s| !s.is_empty())
            .context("Portal token is missing")?;
        let target = base.join(
            form.value()
                .attr("action")
                .filter(|s| !s.is_empty())
                .unwrap_or("/"),
        )?;
        validate_http(&target)?;
        if target.origin() != base.origin() {
            bail!("Login form changed origin");
        }
        return Ok((target, magic));
    }
    bail!("NCUT login form is missing")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirects_and_form_origin() {
        let base = Url::parse("http://192.168.1.1:1000/").unwrap();
        let url = portal_url(&base, "window.location.href='/fgtauth?abc';").unwrap();
        assert_eq!(url.as_str(), "http://192.168.1.1:1000/fgtauth?abc");
        let body = "<title>勤益科技大學</title><form action='/auth'><input name='username'><input name='password'><input name='magic' value='a&amp;b'></form>";
        let (target, magic) = login_form(&url, body).unwrap();
        assert_eq!(target.as_str(), "http://192.168.1.1:1000/auth");
        assert_eq!(magic, "a&b");
        assert!(login_form(&url, &body.replace("/auth", "https://example.com/")).is_err());
        assert!(
            validate_http(&Url::parse("http://user:pass@example.com/fgtauth?a").unwrap()).is_err()
        );
    }
}
