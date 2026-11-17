//! Small allowlisted diagnostics: never log URLs, queries, forms, cookies or script output.
use std::{
    fs::File,
    io::Write,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone)]
pub struct Diagnostics {
    file: Arc<Mutex<Option<File>>>,
    started: Instant,
}

impl Diagnostics {
    pub fn new() -> Self {
        let file = std::env::current_exe()
            .ok()
            .and_then(|exe| File::create(exe.parent()?.join("tonal-login.log")).ok());
        let diagnostic = Self {
            file: Arc::new(Mutex::new(file)),
            started: Instant::now(),
        };
        diagnostic.event("login-start");
        diagnostic
    }

    pub fn event(&self, event: &'static str) {
        self.write(event);
    }

    pub fn navigation(&self, event: &'static str, url: &str) {
        let site = location(url);
        self.write(&format!("{event} {site}"));
    }

    pub fn script_health(&self, result: &str) {
        // Extract only booleans and a fixed readiness enum from our own probe.
        let Ok(serde_json::Value::Object(value)) = serde_json::from_str(result) else {
            return;
        };
        let ready = match value.get("ready").and_then(|v| v.as_str()) {
            Some("complete") => "complete",
            Some("interactive") => "interactive",
            _ => "loading",
        };
        let boolean = |key| value.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
        self.write(&format!(
            "page-health ready={ready} online={} secure={} webauthn={}",
            boolean("online"),
            boolean("secure"),
            boolean("webauthn")
        ));
    }

    fn write(&self, event: &str) {
        if let Ok(mut file) = self.file.lock() {
            if let Some(file) = file.as_mut() {
                let _ = writeln!(file, "{:>6}s {event}", self.started.elapsed().as_secs());
            }
        }
    }
}

fn location(text: &str) -> String {
    let Ok(url) = url::Url::parse(text) else {
        return "invalid-url".into();
    };
    let host = url.host_str().unwrap_or_default();
    // Log only known infrastructure domains, never arbitrary user-controlled hostnames.
    let site = if [
        "google.com",
        "youtube.com",
        "gstatic.com",
        "googleusercontent.com",
    ]
    .iter()
    .any(|base| host == *base || host.ends_with(&format!(".{base}")))
        || crate::auth_navigation::is_google_account_host(host)
    {
        host
    } else {
        "external-site"
    };
    let stage = if url.path().contains("/challenge/pk") {
        "passkey"
    } else if url.path().contains("/challenge/pwd") {
        "password"
    } else if url.path().contains("/challenge/") {
        "identity-challenge"
    } else if url.path().contains("SetSID") || url.path().contains("CheckCookie") {
        "session-transfer"
    } else if url.path().contains("/signin/rejected")
        || url.path().contains("/deniedsigninrejected")
    {
        "rejected"
    } else {
        "page"
    };
    format!("site={site} stage={stage}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_diagnostics_discard_all_identity_and_token_fields() {
        assert_eq!(location("https://person:password@accounts.google.com/v3/signin/challenge/pk/PRIVATE?token=SECRET#private"),
            "site=accounts.google.com stage=passkey");
        assert_eq!(
            location("https://www.google.com/accounts/SetSID?token=SECRET"),
            "site=www.google.com stage=session-transfer"
        );
        assert_eq!(
            location("https://accounts.google.fr/accounts/SetSID?token=SECRET"),
            "site=accounts.google.fr stage=session-transfer"
        );
        assert_eq!(
            location("https://private-id.example.com/person"),
            "site=external-site stage=page"
        );
        assert_eq!(
            location("https://accounts.google.com.evil.test/"),
            "site=external-site stage=page"
        );
    }
}
