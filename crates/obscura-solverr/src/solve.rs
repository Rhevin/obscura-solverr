use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use obscura_browser::lifecycle::WaitUntil;
use obscura_browser::{BrowserContext, Page};
use obscura_net::CookieInfo;
use tokio::time::{timeout, Instant};

use crate::api::{FlareCookie, Solution};
use crate::challenge::{has_cf_clearance, is_cloudflare_challenge};

/// First navigation often hits a CF inline script that blocks until the watchdog
/// fires; the successful path is usually an immediate re-run with a full budget.
const DEFAULT_INITIAL_SCRIPT_MS: u64 = 45_000;

/// Align Obscura engine timeouts with the FlareSolverr `maxTimeout` for this request.
pub fn apply_request_timeouts(max_timeout_ms: u64, retry_scripts: bool) {
    let initial_ms = std::env::var("OBSCURA_SOLVERR_INITIAL_SCRIPT_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_INITIAL_SCRIPT_MS);
    let full_script_ms = max_timeout_ms.saturating_sub(5_000).max(30_000);
    let script_ms = if retry_scripts {
        full_script_ms
    } else {
        initial_ms.min(full_script_ms)
    };
    // Navigation includes fetch + script execution + load events; give a little headroom.
    let nav_ms = max_timeout_ms.saturating_add(10_000);
    std::env::set_var("OBSCURA_SCRIPT_DEADLINE_MS", script_ms.to_string());
    std::env::set_var("OBSCURA_NAV_TIMEOUT_MS", nav_ms.to_string());
    std::env::set_var("OBSCURA_FETCH_TIMEOUT_MS", max_timeout_ms.to_string());
}

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
    apply_request_timeouts(max_timeout_ms, false);
    let overall = Instant::now() + Duration::from_millis(max_timeout_ms);

    // Load (not networkidle0): CF challenge pages keep background requests open.
    timeout(
        Duration::from_millis(max_timeout_ms),
        page.navigate_with_wait(url, WaitUntil::Load),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Navigation timed out after {max_timeout_ms}ms"))?
    .with_context(|| format!("Failed to navigate to {url}"))?;

    page.cancel_v8_termination();

    let mut saw_challenge = false;
    let mut challenge_retry_done = false;
    let mut last_script_rerun = Instant::now();

    loop {
        page.cancel_v8_termination();

        let html = page_html(page);
        let cookies = context.cookie_jar.get_all_cookies();
        let challenged = is_cloudflare_challenge(&html);

        if challenged {
            saw_challenge = true;
            if !has_cf_clearance(&cookies) {
                // Fail-fast nav leaves scripts unfinished; one immediate full-budget re-run
                // usually completes Turnstile in seconds (see nowsecure.nl traces).
                let should_rerun = if !challenge_retry_done {
                    challenge_retry_done = true;
                    true
                } else {
                    last_script_rerun.elapsed() >= Duration::from_secs(30)
                };
                if should_rerun {
                    apply_request_timeouts(max_timeout_ms, true);
                    page.execute_page_scripts().await;
                    last_script_rerun = Instant::now();
                    page.settle(2_000).await;
                } else {
                    page.settle(500).await;
                }
            } else {
                page.settle(500).await;
            }
        } else {
            page.settle(500).await;
        }

        let html = page_html(page);
        let cookies = context.cookie_jar.get_all_cookies();

        if has_cf_clearance(&cookies) {
            break;
        }

        if !challenged && !html.is_empty() {
            break;
        }

        if Instant::now() >= overall {
            anyhow::bail!("Cloudflare challenge not cleared within {max_timeout_ms}ms");
        }

        tokio::time::sleep(Duration::from_millis(1_000)).await;
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
    use super::apply_request_timeouts;
    use obscura_net::CookieInfo;

    #[test]
    fn request_timeouts_follow_max_timeout() {
        std::env::remove_var("OBSCURA_SOLVERR_INITIAL_SCRIPT_MS");
        apply_request_timeouts(120_000, false);
        assert_eq!(std::env::var("OBSCURA_SCRIPT_DEADLINE_MS").unwrap(), "45000");
        assert_eq!(std::env::var("OBSCURA_NAV_TIMEOUT_MS").unwrap(), "130000");
        apply_request_timeouts(120_000, true);
        assert_eq!(std::env::var("OBSCURA_SCRIPT_DEADLINE_MS").unwrap(), "115000");
    }

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
