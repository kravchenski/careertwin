//! Several API keys for one address: which one is in use, which are spare, how they are named and shown.
//!
//! An account is a key for a base address. The key in use is `gateway_key` in the settings; the other accounts
//! for the same address are tried in turn when it is rate limited or rejected, before the next model of the
//! fallback chain is asked. A key is never sent to an address it was not saved for.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::providers::base_url;

pub const MAX_ACCOUNTS: usize = 20;
pub const NAME_LIMIT: usize = 40;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AccountError {
    #[error("Paste the API key")]
    Empty,
    #[error("This key is already saved for this address")]
    AlreadySaved,
    #[error("an account needs a name, an address starting with http:// or https://, and a key")]
    Invalid,
    #[error("at most {MAX_ACCOUNTS} accounts")]
    TooMany,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub url: String,
    pub key: String,
}

impl Account {
    /// The end of the key, enough to tell two apart and never enough to use.
    pub fn tail(&self) -> String {
        let count = self.key.chars().count();
        let take = if count < 16 { 4 } else { 6 };
        format!("…{}", self.key.chars().skip(count.saturating_sub(take)).collect::<String>())
    }
}

fn same_address(a: &str, b: &str) -> bool {
    base_url(a).to_lowercase() == base_url(b).to_lowercase()
}

/// The saved list, checked: each item has a name, an http(s) address and a key. The same key twice for one
/// address is kept once, and two accounts for one address never share a name.
pub fn clean(items: Vec<Account>) -> Result<Vec<Account>, AccountError> {
    let mut result: Vec<Account> = Vec::new();
    for item in items {
        let name: String = item.name.trim().chars().take(NAME_LIMIT).collect();
        let (url, key) = (base_url(&item.url), item.key.trim().to_owned());
        if name.is_empty() || key.is_empty() || !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(AccountError::Invalid);
        }
        if result.iter().any(|a| same_address(&a.url, &url) && a.key == key) {
            continue;
        }
        let name = if result.iter().any(|a| same_address(&a.url, &url) && a.name == name) {
            next_name(&result, &url)
        } else {
            name
        };
        result.push(Account { name, url, key });
    }
    if result.len() > MAX_ACCOUNTS {
        return Err(AccountError::TooMany);
    }
    Ok(result)
}

/// The accounts saved for this address, in the order they were added.
pub fn for_url(items: &[Account], url: &str) -> Vec<Account> {
    items.iter().filter(|account| same_address(&account.url, url)).cloned().collect()
}

/// `default`, then `default-2`, `default-3`...: the first name not taken for this address.
pub fn next_name(items: &[Account], url: &str) -> String {
    let taken: Vec<String> = for_url(items, url).into_iter().map(|account| account.name).collect();
    if !taken.iter().any(|name| name == "default") {
        return "default".to_owned();
    }
    (2..).map(|number| format!("default-{number}")).find(|name| !taken.contains(name)).expect("an unbounded range")
}

/// The list with `key` added under the next free name.
pub fn add(items: &[Account], url: &str, key: &str) -> Result<Vec<Account>, AccountError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(AccountError::Empty);
    }
    if for_url(items, url).iter().any(|account| account.key == key) {
        return Err(AccountError::AlreadySaved);
    }
    let mut next = items.to_vec();
    next.push(Account { name: next_name(items, url), url: url.to_owned(), key: key.to_owned() });
    clean(next)
}

pub fn remove(items: &[Account], url: &str, name: &str) -> Vec<Account> {
    items.iter().filter(|account| !(same_address(&account.url, url) && account.name == name)).cloned().collect()
}

/// The keys to try after `active` for this address, in order.
pub fn spares(items: &[Account], url: &str, active: &str) -> Vec<String> {
    for_url(items, url).into_iter().map(|account| account.key).filter(|key| key != active).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROQ: &str = "https://api.groq.com/openai";

    #[test]
    fn names_follow_default_default_2_default_3_for_each_address() {
        let mut items = Vec::new();
        for key in ["k1", "k2", "k3"] {
            items = add(&items, GROQ, key).unwrap();
        }
        items = add(&items, "https://openrouter.ai/api", "o1").unwrap();
        let names: Vec<_> = items.iter().map(|a| (a.name.as_str(), a.key.as_str())).collect();
        assert_eq!(names, [("default", "k1"), ("default-2", "k2"), ("default-3", "k3"), ("default", "o1")]);
        let items = remove(&items, GROQ, "default-2");
        assert_eq!(next_name(&items, GROQ), "default-2"); // a freed name is used again
    }

    #[test]
    fn the_same_key_twice_for_one_address_is_refused_but_fine_for_another() {
        let items = add(&[], GROQ, "same").unwrap();
        assert_eq!(add(&items, &format!("{GROQ}/v1/"), "same"), Err(AccountError::AlreadySaved));
        assert_eq!(add(&items, "https://api.openai.com", "same").unwrap().len(), 2);
        assert_eq!(add(&items, GROQ, "   "), Err(AccountError::Empty));
    }

    #[test]
    fn accounts_are_found_by_address_however_it_was_written() {
        let items = add(&[], &format!("{GROQ}/v1"), "k1").unwrap();
        assert_eq!(for_url(&items, "HTTPS://API.GROQ.COM/openai/")[0].key, "k1");
        assert!(for_url(&items, "https://api.openai.com").is_empty());
    }

    #[test]
    fn the_end_of_a_key_is_shown_and_never_the_whole_key() {
        let long = Account { name: "default".into(), url: GROQ.into(), key: "gsk_abcdefghijklmnopqrstuv".into() };
        assert_eq!(long.tail(), "…qrstuv");
        assert_eq!(Account { key: "secret".into(), ..long }.tail(), "…cret");
    }

    #[test]
    fn a_saved_list_is_checked_and_tidied() {
        let tidy = clean(vec![
            Account { name: " work ".into(), url: format!("{GROQ}/"), key: " k1 ".into() },
            Account { name: "work".into(), url: GROQ.into(), key: "k2".into() }, // the name is taken
            Account { name: "again".into(), url: GROQ.into(), key: "k1".into() }, // the same key: kept once
        ])
        .unwrap();
        assert_eq!(
            tidy.iter().map(|a| (a.name.as_str(), a.key.as_str())).collect::<Vec<_>>(),
            [("work", "k1"), ("default", "k2")]
        );
        let bad = |name: &str, url: &str, key: &str| {
            clean(vec![Account { name: name.into(), url: url.into(), key: key.into() }])
        };
        assert_eq!(bad("x", "ftp://h", "k"), Err(AccountError::Invalid));
        assert_eq!(bad("", GROQ, "k"), Err(AccountError::Invalid));
        assert_eq!(bad("x", GROQ, ""), Err(AccountError::Invalid));
        let many = (0..=MAX_ACCOUNTS)
            .map(|n| Account { name: format!("a{n}"), url: GROQ.into(), key: format!("k{n}") })
            .collect();
        assert_eq!(clean(many), Err(AccountError::TooMany));
    }

    #[test]
    fn the_spare_keys_are_the_others_of_the_same_address_in_order() {
        let items =
            add(&add(&add(&[], GROQ, "k1").unwrap(), GROQ, "k2").unwrap(), "https://api.openai.com", "o1").unwrap();
        assert_eq!(spares(&items, GROQ, "k1"), ["k2"]);
        assert_eq!(spares(&items, GROQ, "k2"), ["k1"]);
        assert_eq!(spares(&items, GROQ, ""), ["k1", "k2"]);
    }
}
