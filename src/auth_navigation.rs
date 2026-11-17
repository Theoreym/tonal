//! Exact Google-owned domains, with regional domains limited to session transfer.
fn is_google_domain(domain: &str) -> bool {
    include_str!("../assets/auth/google-domains.txt")
        .lines()
        .any(|entry| entry.strip_prefix('.') == Some(domain))
}

pub fn is_google_account_host(host: &str) -> bool {
    let domain = host
        .strip_prefix("accounts.")
        .or_else(|| host.strip_prefix("www."))
        .unwrap_or(host);
    is_google_domain(domain)
}

pub fn is_session_transfer(url: &url::Url) -> bool {
    matches!(
        url.path(),
        "/accounts/SetSID" | "/SetSID" | "/accounts/CheckCookie" | "/CheckCookie"
    )
}

pub fn allowed(text: &str) -> bool {
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    matches!(
        host,
        "accounts.google.com"
            | "accounts.youtube.com"
            | "music.youtube.com"
            | "www.youtube.com"
            | "consent.youtube.com"
            | "consent.google.com"
            | "myaccount.google.com"
            | "gds.google.com"
    ) || (is_google_account_host(host) && is_session_transfer(&url))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_google_youtube_and_regional_session_transfer_chain() {
        for url in [
            "https://accounts.google.com/CheckCookie?continue=test",
            "https://accounts.youtube.com/accounts/SetSID?token=test",
            "https://accounts.google.fr/accounts/SetSID?token=test",
            "https://accounts.google.be/accounts/SetSID?token=test",
            "https://accounts.google.co.uk/accounts/SetSID?token=test",
            "https://www.google.fr/accounts/CheckCookie?continue=test",
            "https://music.youtube.com/",
        ] {
            assert!(allowed(url), "{url}");
        }
    }

    #[test]
    fn regional_permission_is_limited_to_exact_google_domains_and_transfer_paths() {
        for url in [
            "https://accounts.google.fr.evil.test/accounts/SetSID",
            "https://accounts.google.invalid/accounts/SetSID",
            "https://accounts.google.zz/accounts/SetSID",
            "https://evil.test/accounts/SetSID?continue=https://accounts.google.fr/",
            "https://sites.google.fr/accounts/SetSID",
            "https://accounts.google.fr/arbitrary/SetSID",
            "https://www.google.fr/search?q=test",
            "http://accounts.google.fr/accounts/SetSID",
            "https://accounts.google.fr:8443/accounts/SetSID",
            "https://user:password@accounts.google.fr/accounts/SetSID",
        ] {
            assert!(!allowed(url), "{url}");
        }
    }
}
