//! Check that every company's careers page and website still answer.
//!
//! Deliberately a plain command-line tool, so that anyone (or any coding agent) can run it from a shell, read the
//! report and decide what to fix in `src/careertwin/jobs/data/companies.json`. The exit code is not zero when a
//! broken link was found.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::Semaphore;

pub const TIMEOUT: Duration = Duration::from_secs(12);
const USER_AGENT: &str = "Mozilla/5.0 (compatible; CareerTwinLinkChecker/1.0)";

/// What came back from one address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub url: String,
    /// Where the redirects ended, when that is not the address asked for.
    pub final_url: Option<String>,
    pub status: Option<u16>,
    pub ok: bool,
    pub error: Option<String>,
}

impl Check {
    pub fn to_json(&self) -> Value {
        json!({
            "url": self.url,
            "final_url": self.final_url,
            "status": self.status,
            "ok": self.ok,
            "error": self.error,
        })
    }

    /// Why it is broken, for the report.
    pub fn detail(&self) -> String {
        self.error.clone().unwrap_or_else(|| {
            format!(
                "HTTP {}",
                self.status
                    .map_or_else(|| "?".to_owned(), |status| status.to_string())
            )
        })
    }
}

/// The checks of one company: `careers` is its jobs page, `website` its home page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: Option<String>,
    pub careers: Option<Check>,
    pub website: Option<Check>,
}

impl Entry {
    pub fn to_json(&self) -> Value {
        let mut entry = serde_json::Map::new();
        entry.insert("name".to_owned(), json!(self.name));
        if let Some(check) = &self.careers {
            entry.insert("careers".to_owned(), check.to_json());
        }
        if let Some(check) = &self.website {
            entry.insert("website".to_owned(), check.to_json());
        }
        Value::Object(entry)
    }
}

/// Which addresses of each company are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fields {
    pub careers: bool,
    pub domain: bool,
}

impl Fields {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "careers" => Some(Self {
                careers: true,
                domain: false,
            }),
            "domain" => Some(Self {
                careers: false,
                domain: true,
            }),
            "both" => Some(Self {
                careers: true,
                domain: true,
            }),
            _ => None,
        }
    }
}

pub fn client(timeout: Duration) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build()
}

fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "timeout".to_owned()
    } else if error.is_connect() {
        "ConnectError".to_owned()
    } else if error.is_redirect() {
        "TooManyRedirects".to_owned()
    } else if error.is_builder() {
        "InvalidURL".to_owned()
    } else {
        error.to_string()
    }
}

/// Ask for `url` and follow its redirects. Any failure means "broken", with the reason.
pub async fn check_url(client: &reqwest::Client, url: &str) -> Check {
    let asked =
        reqwest::Url::parse(url).map_or_else(|_| url.to_owned(), |parsed| parsed.to_string());
    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let end = response.url().to_string();
            Check {
                url: url.to_owned(),
                final_url: (end != asked).then_some(end),
                status: Some(status),
                ok: status < 400,
                error: None,
            }
        }
        Err(error) => Check {
            url: url.to_owned(),
            final_url: None,
            status: None,
            ok: false,
            error: Some(describe(&error)),
        },
    }
}

/// Check the companies, `concurrency` at a time; the results are in the order of the list.
pub async fn run(
    companies: &[Value],
    fields: Fields,
    concurrency: usize,
    timeout: Duration,
) -> Result<Vec<Entry>, String> {
    let client = client(timeout).map_err(|error| error.to_string())?;
    let permits = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut tasks = tokio::task::JoinSet::new();
    for (index, company) in companies.iter().enumerate() {
        let (client, permits) = (client.clone(), permits.clone());
        let text = |key: &str| {
            company
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let (name, careers, domain) = (
            text("name"),
            text("careers").filter(|_| fields.careers),
            text("domain").filter(|_| fields.domain),
        );
        tasks.spawn(async move {
            let _slot = permits
                .acquire()
                .await
                .expect("the semaphore is never closed");
            let careers = match careers {
                Some(url) => Some(check_url(&client, &url).await),
                None => None,
            };
            let website = match domain {
                Some(domain) => Some(check_url(&client, &format!("https://{domain}")).await),
                None => None,
            };
            (
                index,
                Entry {
                    name,
                    careers,
                    website,
                },
            )
        });
    }
    let mut results = Vec::with_capacity(companies.len());
    while let Some(done) = tasks.join_next().await {
        results.push(done.map_err(|error| error.to_string())?);
    }
    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, entry)| entry).collect())
}

/// The broken links as `(company, field, check)`.
pub fn broken(results: &[Entry]) -> Vec<(String, &'static str, &Check)> {
    let mut found = Vec::new();
    for entry in results {
        let name = entry.name.clone().unwrap_or_else(|| "?".to_owned());
        for (field, check) in [("careers", &entry.careers), ("website", &entry.website)] {
            if let Some(check) = check.as_ref().filter(|check| !check.ok) {
                found.push((name.clone(), field, check));
            }
        }
    }
    found
}

/// What the command prints.
pub fn report(results: &[Entry]) -> String {
    let mut text = format!("Checked {} companies.\n", results.len());
    let broken = broken(results);
    if broken.is_empty() {
        text.push_str("All links OK.\n");
    } else {
        text.push_str(&format!("\n{} broken link(s):\n\n", broken.len()));
        for (name, field, check) in broken {
            text.push_str(&format!(
                "  - {name} [{field}]: {}  ->  {}\n",
                check.url,
                check.detail()
            ));
        }
    }
    text
}
