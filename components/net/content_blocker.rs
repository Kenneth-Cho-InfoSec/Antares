/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};

use servo_url::ServoUrl;

#[derive(Default)]
struct FilterPolicy {
    blocked_domains: HashSet<String>,
    allowed_domains: HashSet<String>,
    url_fragments: Vec<String>,
}

#[derive(Default)]
struct ContentBlockingState {
    block_ads: bool,
    block_gifs: bool,
    policy: FilterPolicy,
}

static STATE: LazyLock<RwLock<ContentBlockingState>> =
    LazyLock::new(|| RwLock::new(ContentBlockingState::default()));

/// Replaces the process-wide policy used for subsequent network requests.
pub fn configure(policy_text: &str, block_ads: bool, block_gifs: bool) {
    let policy = FilterPolicy::parse(policy_text.lines());
    let mut state = STATE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *state = ContentBlockingState {
        block_ads,
        block_gifs,
        policy,
    };
}

/// Returns true when the current global policy rejects this request URL.
pub fn should_block(url: &ServoUrl) -> bool {
    let state = STATE
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.block_gifs && url.path().to_ascii_lowercase().ends_with(".gif") {
        return true;
    }
    if !state.block_ads {
        return false;
    }

    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() ||
        state
            .policy
            .domain_matches(&state.policy.allowed_domains, &host)
    {
        return false;
    }
    if state
        .policy
        .domain_matches(&state.policy.blocked_domains, &host)
    {
        return true;
    }

    let value = url.as_str().to_ascii_lowercase();
    state
        .policy
        .url_fragments
        .iter()
        .any(|fragment| value.contains(fragment))
}

impl FilterPolicy {
    fn parse<'a>(lines: impl Iterator<Item = &'a str>) -> Self {
        let mut policy = Self::default();
        for input in lines {
            let line = input.trim();
            if line.is_empty() ||
                line.starts_with('!') ||
                line.starts_with('#') ||
                line.starts_with('[') ||
                line.contains("##") ||
                line.contains("#@#")
            {
                continue;
            }

            if let Some(domain) = line.strip_prefix("@@||").and_then(parse_rule_domain) {
                policy.allowed_domains.insert(domain);
                continue;
            }
            if let Some(domain) = line.strip_prefix("||").and_then(parse_rule_domain) {
                policy.blocked_domains.insert(domain);
                continue;
            }

            let mut fields = line.split_whitespace();
            if let (Some(address), Some(domain)) = (fields.next(), fields.next()) {
                if is_hosts_address(address) {
                    let domain = normalize_domain(domain);
                    if !domain.is_empty() {
                        policy.blocked_domains.insert(domain);
                    }
                    continue;
                }
            }

            let fragment = line
                .split('$')
                .next()
                .unwrap_or_default()
                .trim_matches('|')
                .replace('*', "")
                .replace('^', "")
                .to_ascii_lowercase();
            if fragment.len() >= 3 {
                policy.url_fragments.push(fragment);
            }
        }
        policy
    }

    fn domain_matches(&self, domains: &HashSet<String>, host: &str) -> bool {
        let mut candidate = host;
        loop {
            if domains.contains(candidate) {
                return true;
            }
            let Some(dot) = candidate.find('.') else {
                return false;
            };
            candidate = &candidate[dot + 1..];
        }
    }
}

fn parse_rule_domain(value: &str) -> Option<String> {
    let domain = value
        .split(['^', '/', '*', '$'])
        .next()
        .map(normalize_domain)
        .unwrap_or_default();
    (!domain.is_empty()).then_some(domain)
}

fn normalize_domain(value: &str) -> String {
    value.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn is_hosts_address(value: &str) -> bool {
    value == "0.0.0.0" || value == "127.0.0.1" || value == "::" || value == "::1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hosts_and_filter_domains_with_exceptions() {
        let policy = FilterPolicy::parse(
            "127.0.0.1 ads.example\n||tracker.test^\n@@||safe.tracker.test^".lines(),
        );
        assert!(policy.domain_matches(&policy.blocked_domains, "cdn.ads.example"));
        assert!(policy.domain_matches(&policy.blocked_domains, "tracker.test"));
        assert!(policy.domain_matches(&policy.allowed_domains, "safe.tracker.test"));
    }
}
