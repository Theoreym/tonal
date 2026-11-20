//! Public radio endpoints only. Never attach account headers; pin checked DNS results.
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || a == 0
        || a >= 240
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19)))
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => ip.to_ipv4_mapped().map(public_v4).unwrap_or_else(|| {
            let s = ip.segments();
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
                && (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && s[1] == 0xdb8)
        }),
    }
}
pub fn allowed(text: &str) -> bool {
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    if text.len() > 2048
        || !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port() == Some(0)
    {
        return false;
    }
    match url.host() {
        Some(url::Host::Ipv4(ip)) => public_ip(ip.into()),
        Some(url::Host::Ipv6(ip)) => public_ip(ip.into()),
        Some(url::Host::Domain(host)) => {
            let host = host.trim_end_matches('.');
            host.contains('.')
                && ![
                    "localhost",
                    "local",
                    "internal",
                    "lan",
                    "home",
                    "invalid",
                    "test",
                ]
                .iter()
                .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
        }
        None => false,
    }
}
pub async fn get(text: &str) -> Result<reqwest::Response, String> {
    request(text, false).await
}
pub async fn stream(text: &str) -> Result<reqwest::Response, String> {
    request(text, true).await
}
async fn request(text: &str, metadata: bool) -> Result<reqwest::Response, String> {
    let mut url = url::Url::parse(text).map_err(|_| "Adresse de radio invalide.")?;
    for _ in 0..6 {
        if !allowed(url.as_str()) {
            return Err("Adresse de radio non publique.".into());
        }
        let host = url.host_str().ok_or("Adresse de radio invalide.")?;
        let port = url.port_or_known_default().ok_or("Port invalide.")?;
        let addresses: Vec<SocketAddr> = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::lookup_host((host.trim_matches(['[', ']']), port)),
        )
        .await
        .map_err(|_| "La radio ne répond pas.")?
        .map_err(|_| "Adresse de radio introuvable.")?
        .collect();
        if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
            return Err("Adresse de radio non publique.".into());
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .resolve_to_addrs(host, &addresses)
            .connect_timeout(Duration::from_secs(8))
            .read_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("Tonal/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| "Connexion radio indisponible.")?;
        let response = client
            .get(url.clone())
            .header("Icy-MetaData", if metadata { "1" } else { "0" })
            .send()
            .await
            .map_err(|_| "La radio ne répond pas.")?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or("Redirection radio invalide.")?;
            let target = url
                .join(location)
                .map_err(|_| "Redirection radio invalide.")?;
            if url.scheme() == "https" && target.scheme() != "https" {
                return Err("Redirection radio non sécurisée.".into());
            }
            url = target;
        } else {
            return response
                .error_for_status()
                .map_err(|_| "Cette radio est indisponible.".into());
        }
    }
    Err("Trop de redirections pour cette radio.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_streams_allow_radio_ports_but_reject_local_targets_and_credentials() {
        for url in [
            "https://stream.radiofrance.fr/fip.mp3",
            "http://radio.example.com:8000/live",
            "https://8.8.8.8/live",
        ] {
            assert!(allowed(url), "{url}");
        }
        for url in [
            "file:///music",
            "http://localhost/",
            "http://192.168.1.2/",
            "http://127.1/",
            "http://[::ffff:127.0.0.1]/",
            "https://a.local/",
            "https://a.local./",
            "https://user:password@example.com/",
            "http://169.254.169.254/",
            "http://100.64.0.1/",
            "http://[fc00::1]/",
            "http://[2001:db8::1]/",
        ] {
            assert!(!allowed(url), "{url}");
        }
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.2",
            "::1",
            "fe80::1",
            "::ffff:10.0.0.1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()));
        }
    }
}
