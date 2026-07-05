use obscura_net::CookieInfo;

/// Heuristic: page HTML still looks like an active Cloudflare interstitial.
pub fn is_cloudflare_challenge(html: &str) -> bool {
    let lower = html.to_lowercase();
    lower.contains("checking your browser")
        || lower.contains("just a moment")
        || lower.contains("cf-browser-verification")
        || lower.contains("challenges.cloudflare.com/turnstile")
        || (lower.contains("challenge-platform") && lower.contains("__cf$cv$params"))
}

pub fn has_cf_clearance(cookies: &[CookieInfo]) -> bool {
    cookies
        .iter()
        .any(|c| c.name == "cf_clearance" && !c.value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_interstitial_title() {
        assert!(is_cloudflare_challenge(
            "<html><title>Just a moment...</title></html>"
        ));
    }

    #[test]
    fn cf_clearance_present() {
        assert!(has_cf_clearance(&[CookieInfo {
            name: "cf_clearance".into(),
            value: "abc".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            same_site: String::new(),
            expires: None,
        }]));
    }
}
