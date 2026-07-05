use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use obscura_browser::lifecycle::WaitUntil;
use obscura_browser::{BrowserContext, Page};
use obscura_net::CookieInfo;
use tokio::time::timeout;

use crate::api::{FlareCookie, Solution};
use crate::challenge::{has_cf_clearance, is_cloudflare_challenge};

pub struct SolveOptions {
    pub url: String,
    pub max_timeout_ms: u64,
    pub proxy: Option<String>,
    pub stealth: bool,
    pub user_agent: Option<String>,
}

pub struct SolveResult {
    pub solution: Solution,
    pub message: String,
}

pub async fn solve_get(
    page: &mut Page,
    context: &Arc<BrowserContext>,
    url: &str,
    max_timeout_ms: u64,
) -> Result<SolveResult> {
    let wait = WaitUntil::NetworkIdle0;

    timeout(
        Duration::from_millis(max_timeout_ms),
        page.navigate_with_wait(url, wait),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Navigation timed out after {max_timeout_ms}ms"))?
    .with_context(|| format!("Failed to navigate to {url}"))?;

    let deadline = tokio::time::Instant::now() + Duration::from_millis(max_timeout_ms);
    let mut saw_challenge = false;

    loop {
        page.settle(500).await;

        let html = page_html(page);
        let cookies = context.cookie_jar.get_all_cookies();
        let challenged = is_cloudflare_challenge(&html);

        if challenged {
            saw_challenge = true;
        }

        if has_cf_clearance(&cookies) {
            break;
        }

        if !challenged && !html.is_empty() {
            break;
        }

        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("Cloudflare challenge not cleared within {max_timeout_ms}ms");
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let html = page_html(page);
    let cookies = context.cookie_jar.get_all_cookies();
    let final_url = page.url_string();
    let user_agent = context.user_agent.clone();

    let message = if saw_challenge {
        if has_cf_clearance(&cookies) {
            "Challenge solved!".to_string()
        } else {
            "Challenge not detected!".to_string()
        }
    } else {
        "Challenge not detected!".to_string()
    };

    Ok(SolveResult {
        message,
        solution: Solution {
            url: final_url,
            status: 200,
            response: html,
            user_agent,
            cookies: flare_cookies(&cookies),
        },
    })
}

pub async fn solve_ephemeral(opts: SolveOptions) -> Result<SolveResult> {
    let context = Arc::new(BrowserContext::with_storage_full(
        "solverr-ephemeral".to_string(),
        opts.proxy,
        opts.stealth,
        opts.user_agent.clone(),
        None,
    ));
    let mut page = Page::new("solverr-ephemeral-page".to_string(), context.clone());

    if let Some(ref ua) = opts.user_agent {
        page.http_client.set_user_agent(ua).await;
    }

    solve_get(
        &mut page,
        &context,
        &opts.url,
        opts.max_timeout_ms,
    )
    .await
}

fn page_html(page: &Page) -> String {
    page.with_dom(|dom| {
        if let Ok(Some(html_node)) = dom.query_selector("html") {
            let html = dom.outer_html(html_node);
            format!("<!DOCTYPE html>\n{html}")
        } else {
            let doc = dom.document();
            dom.inner_html(doc)
        }
    })
    .unwrap_or_default()
}

fn flare_cookies(cookies: &[CookieInfo]) -> Vec<FlareCookie> {
    cookies
        .iter()
        .map(|c| FlareCookie {
            name: c.name.clone(),
            value: c.value.clone(),
            domain: Some(c.domain.clone()),
            path: Some(c.path.clone()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::flare_cookies;
    use obscura_net::CookieInfo;

    #[test]
    fn maps_cookies_for_api() {
        let out = flare_cookies(&[CookieInfo {
            name: "cf_clearance".into(),
            value: "x".into(),
            domain: ".nowsecure.nl".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            same_site: String::new(),
            expires: None,
        }]);
        assert_eq!(out[0].name, "cf_clearance");
    }
}
