use anyhow::{bail, Context, Result};
use reqwest::{Client, Response};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, LazyLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;

pub const DEFAULT_PROBE: &str = "http://www.gstatic.com/generate_204";
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const VERIFICATION_DELAYS_MS: [u64; 3] = [0, 250, 750];
const MAX_BODY: usize = 256 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub username: String,
    pub interval_seconds: u64,
    pub auto_connect: bool,
    pub start_at_login: bool,
    pub portal_url: String,
    pub renewal_minutes: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            username: String::new(),
            interval_seconds: 15,
            auto_connect: false,
            start_at_login: false,
            portal_url: String::new(),
            renewal_minutes: 30,
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
    portal_client: Client,
    probe: Url,
    portal: Option<Url>,
}

struct Page {
    url: Url,
    code: u16,
    body: String,
}

struct LoginForm {
    page: Url,
    action: Url,
    magic: String,
    return_url: Option<String>,
}

enum Connection {
    Online,
    NeedsLogin(Box<LoginForm>),
}

impl Engine {
    pub fn new() -> Result<Self> {
        Self::with_probe(DEFAULT_PROBE)
    }

    pub fn with_probe(probe: &str) -> Result<Self> {
        Self::with_probe_and_portal(probe, "")
    }

    pub fn with_probe_and_portal(probe: &str, portal: &str) -> Result<Self> {
        let probe = Url::parse(probe)?;
        validate_http(&probe)?;
        let portal = parse_portal_address(portal)?;
        let cookies = Arc::new(reqwest::cookie::Jar::default());
        let client = client_builder(cookies.clone())
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;
        let portal_client = client_builder(cookies)
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
            .build()?;
        Ok(Self {
            client,
            portal_client,
            probe,
            portal,
        })
    }

    async fn page(mut response: Response) -> Result<Page> {
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

    async fn get_page(client: &Client, url: &Url) -> Result<Page> {
        Self::page(
            client
                .get(url.clone())
                .header("Cache-Control", "no-cache")
                .timeout(PROBE_TIMEOUT)
                .send()
                .await?,
        )
        .await
    }

    fn permits_portal(&self, url: &Url) -> bool {
        validate_http(url).is_ok()
            && self
                .portal
                .as_ref()
                .is_none_or(|portal| portal.origin() == url.origin())
    }

    async fn find_login(&self, page: Page) -> Result<Option<LoginForm>> {
        let redirect = script_redirect(&page.url, &page.body, |url| {
            is_challenge(url) && self.permits_portal(url) && *url != page.url
        });
        let from_portal = is_challenge(&page.url)
            || self
                .portal
                .as_ref()
                .is_some_and(|portal| portal.origin() == page.url.origin());
        if from_portal && self.permits_portal(&page.url) && (200..300).contains(&page.code) {
            match login_form(&page.url, &page.body) {
                Ok(form) => return Ok(Some(form)),
                Err(error) if redirect.is_none() => return Err(error),
                Err(_) => {}
            }
        }
        let Some(redirect) = redirect else {
            return Ok(None);
        };
        let page = Self::get_page(&self.portal_client, &redirect).await?;
        anyhow::ensure!(
            (200..300).contains(&page.code),
            "Portal did not serve a login page"
        );
        login_form(&page.url, &page.body).map(Some)
    }

    async fn direct_login(&self, portal: &Url) -> Result<Option<LoginForm>> {
        let mut url = portal.clone();
        if matches!(url.path(), "/" | "/login") {
            // The request uses an opaque query. The form supplies its own challenge.
            let marker = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            url.set_path("/login");
            url.set_query(Some(&format!("{marker:016x}")));
        }
        let page = Self::get_page(&self.portal_client, &url)
            .await
            .context("Configured portal is unavailable")?;
        self.find_login(page).await
    }

    async fn discover(&self) -> Result<Connection> {
        let login = match Self::get_page(&self.client, &self.probe).await {
            Ok(page) if page.code == 204 && page.url == self.probe => {
                return Ok(Connection::Online)
            }
            Ok(page) => self.find_login(page).await,
            Err(error) => Err(error).context("Network check failed"),
        };
        let login = match (login, &self.portal) {
            (Ok(Some(form)), _) => Some(form),
            (_, Some(portal)) => self.direct_login(portal).await?,
            (result, None) => result?,
        };
        login
            .map(|form| Connection::NeedsLogin(Box::new(form)))
            .context("No Fortinet portal found")
    }

    pub async fn check(&self) -> Status {
        match self.discover().await {
            Ok(Connection::Online) => Status::Online,
            Ok(Connection::NeedsLogin(_)) => Status::NeedsLogin,
            Err(_) => Status::Unstable,
        }
    }

    /// Check and submit the same challenge, only loading credentials if the portal needs them.
    pub async fn reconnect(
        &self,
        credentials: impl FnOnce() -> Option<(String, String)>,
    ) -> Status {
        match self.discover().await {
            Ok(Connection::Online) => Status::Online,
            Ok(Connection::NeedsLogin(form)) => match credentials() {
                Some((username, password)) => self
                    .submit(*form, &username, &password)
                    .await
                    .unwrap_or(Status::LoginFailed),
                None => Status::MissingCredentials,
            },
            Err(_) => Status::Unstable,
        }
    }

    pub async fn connect(&self, username: &str, password: &str) -> Result<Status> {
        if username.trim().is_empty() || password.is_empty() {
            return Ok(Status::MissingCredentials);
        }
        match self.discover().await? {
            Connection::Online => Ok(Status::Online),
            Connection::NeedsLogin(form) => self.submit(*form, username, password).await,
        }
    }

    /// Ask a configured gateway to renew the session, even while the probe is online.
    pub async fn renew(&self, credentials: impl FnOnce() -> Option<(String, String)>) -> bool {
        let Some(portal) = &self.portal else {
            return false;
        };
        let Ok(Some(form)) = self.direct_login(portal).await else {
            return false;
        };
        let Some((username, password)) = credentials() else {
            return false;
        };
        if username.trim().is_empty() || password.is_empty() {
            return false;
        }
        self.authenticate(form, &username, &password)
            .await
            .unwrap_or(false)
    }

    async fn submit(&self, form: LoginForm, username: &str, password: &str) -> Result<Status> {
        if username.trim().is_empty() || password.is_empty() {
            return Ok(Status::MissingCredentials);
        }
        // The gateway can finish authentication even if the POST response is lost.
        let _ = self.authenticate(form, username, password).await;
        for delay in VERIFICATION_DELAYS_MS {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            if Self::get_page(&self.client, &self.probe)
                .await
                .is_ok_and(|page| page.code == 204 && page.url == self.probe)
            {
                return Ok(Status::Online);
            }
        }
        Ok(Status::LoginFailed)
    }

    async fn authenticate(&self, form: LoginForm, username: &str, password: &str) -> Result<bool> {
        let response = self
            .portal_client
            .post(form.action)
            .header("Origin", form.page.origin().ascii_serialization())
            .header("Referer", form.page.as_str())
            .form(&[
                (
                    "4Tredir",
                    form.return_url.as_deref().unwrap_or(self.probe.as_str()),
                ),
                ("magic", form.magic.as_str()),
                ("username", username),
                ("password", password),
            ])
            .send()
            .await?;
        let mut page = Self::page(response).await?;
        if let Some(keepalive) = script_redirect(&page.url, &page.body, |url| {
            url.path() == "/keepalive"
                && url.query().is_some_and(|query| !query.is_empty())
                && url.origin() == page.url.origin()
                && validate_http(url).is_ok()
                && *url != page.url
        }) {
            page = Self::get_page(&self.portal_client, &keepalive).await?;
        }
        Ok((200..300).contains(&page.code)
            && page.url.path() == "/keepalive"
            && page.body.contains("/logout?"))
    }
}

fn client_builder(cookies: Arc<reqwest::cookie::Jar>) -> reqwest::ClientBuilder {
    Client::builder()
        .cookie_provider(cookies)
        .no_proxy()
        .pool_max_idle_per_host(0)
        .timeout(Duration::from_secs(12))
        .connect_timeout(Duration::from_secs(4))
        .user_agent("NCUT-Auto-Login/4.0")
}

pub fn parse_portal_address(address: &str) -> Result<Option<Url>> {
    if address.trim().is_empty() {
        return Ok(None);
    }
    let url = Url::parse(address.trim())?;
    validate_http(&url)?;
    anyhow::ensure!(
        url.query().is_none() && url.fragment().is_none(),
        "Use a portal address without a query or fragment"
    );
    Ok(Some(url))
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

fn is_challenge(url: &Url) -> bool {
    matches!(url.path(), "/fgtauth" | "/login")
        && url.query().is_some_and(|query| !query.is_empty())
}

fn script_redirect(base: &Url, body: &str, matches: impl FnMut(&Url) -> bool) -> Option<Url> {
    static PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r#"\blocation\s*(?:(?:\.href)?\s*=|\.(?:replace|assign)\s*\()\s*["']([^"']+)["']"#,
        )
        .expect("Fortinet redirect pattern")
    });
    PATTERN
        .captures_iter(body)
        .filter_map(|capture| base.join(&capture[1].replace("&amp;", "&")).ok())
        .find(matches)
}

fn login_form(base: &Url, body: &str) -> Result<LoginForm> {
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
        let input_value = |name| {
            fields
                .iter()
                .find(|e| e.value().attr("name") == Some(name))
                .and_then(|e| e.value().attr("value"))
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let magic = input_value("magic")
            .or_else(|| {
                (base.path() == "/fgtauth").then_some(())?;
                let token = base.query()?.split('&').next()?;
                (!token.contains('=')).then(|| token.to_owned())
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
        return Ok(LoginForm {
            page: base.clone(),
            action: target,
            magic,
            return_url: input_value("4Tredir"),
        });
    }
    bail!("NCUT login form is missing")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirects_and_form_origin() {
        let base = Url::parse("http://192.168.1.1:1000/").unwrap();
        let url =
            script_redirect(&base, "window.location.href='/fgtauth?abc';", is_challenge).unwrap();
        assert_eq!(url.as_str(), "http://192.168.1.1:1000/fgtauth?abc");
        let body = "<title>勤益科技大學</title><form action='/auth'><input name='username'><input name='password'><input name='magic' value='a&amp;b'></form>";
        let form = login_form(&url, body).unwrap();
        assert_eq!(form.action.as_str(), "http://192.168.1.1:1000/auth");
        assert_eq!(form.magic, "a&b");
        assert!(login_form(&url, &body.replace("/auth", "https://example.com/")).is_err());
        assert!(
            validate_http(&Url::parse("http://user:pass@example.com/fgtauth?a").unwrap()).is_err()
        );
        let direct = base.join("/login?magic=").unwrap();
        let missing_magic = "<title>勤益科技大學</title><form action='/'>
            <input name='username'><input name='password'></form>";
        assert!(login_form(&direct, missing_magic).is_err());
    }
}
