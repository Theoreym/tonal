//! YouTube URL resolution only. Audio bytes are streamed by native_stream, never saved as WAV.
use crate::session::Session;
use serde::Deserialize;
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use url::Url;
use zeroize::Zeroizing;

pub const AUDIO_FORMATS: &str = "774/141/251/140/bestaudio[acodec=opus]/bestaudio[acodec^=mp4a]";

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct AudioQuality {
    pub format_id: String,
    #[serde(rename = "acodec")]
    pub codec: String,
    #[serde(rename = "abr")]
    pub bitrate_kbps: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QualityTier {
    Standard,
    High,
    Master,
}
impl QualityTier {
    pub fn letter(self) -> &'static str {
        match self {
            Self::Standard => "S",
            Self::High => "H",
            Self::Master => "M",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::High => "High",
            Self::Master => "Master",
        }
    }
}
impl AudioQuality {
    pub fn tier(&self) -> Option<QualityTier> {
        let bitrate = self.bitrate()?;
        Some(if bitrate >= 250.0 {
            QualityTier::Master
        } else if bitrate >= 160.0 {
            QualityTier::High
        } else {
            QualityTier::Standard
        })
    }
    fn bitrate(&self) -> Option<f64> {
        self.bitrate_kbps
            .filter(|n| n.is_finite() && *n > 0.0)
            .or_else(|| matches!(self.format_id.as_str(), "774" | "141").then_some(256.0))
    }
    pub fn label(&self) -> String {
        let codec = if self.codec == "opus" {
            "Opus"
        } else if self.codec.starts_with("mp4a") {
            "AAC"
        } else {
            "Audio"
        };
        let bitrate = self.bitrate();
        match bitrate {
            Some(rate) => format!("{codec} · {rate:.0} kbit/s"),
            None => codec.into(),
        }
    }
}

#[derive(Clone)]
pub struct MediaInfo {
    pub url: Zeroizing<String>,
    pub source_url: String,
    pub quality: AudioQuality,
    pub bytes: u64,
    pub duration: Option<Duration>,
    pub user_agent: String,
}

#[derive(Deserialize)]
struct Resolved {
    url: String,
    #[serde(flatten)]
    quality: AudioQuality,
    filesize: Option<u64>,
    duration: Option<f64>,
    #[serde(default)]
    http_headers: HttpHeaders,
}
#[derive(Default, Deserialize)]
struct HttpHeaders {
    #[serde(rename = "User-Agent")]
    user_agent: Option<String>,
}

pub fn check_url(text: &str) -> Result<(), String> {
    let url = Url::parse(text.trim()).map_err(|_| "URL invalide.".to_owned())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err("Utilise une URL YouTube HTTPS sans identifiants ni port personnalisé.".into());
    }
    if !matches!(
        url.host_str(),
        Some(
            "youtube.com" | "www.youtube.com" | "music.youtube.com" | "m.youtube.com" | "youtu.be"
        )
    ) {
        return Err("Utilise une URL YouTube ou YouTube Music.".into());
    }
    Ok(())
}

pub fn media_url_allowed(text: &str) -> bool {
    Url::parse(text).is_ok_and(|url| {
        url.scheme() == "https"
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url
                .host_str()
                .is_some_and(|host| host == "googlevideo.com" || host.ends_with(".googlevideo.com"))
    })
}

fn canonical_url(text: &str) -> Result<String, String> {
    check_url(text)?;
    let url = Url::parse(text.trim()).map_err(|_| "URL invalide.")?;
    let video = if url.host_str() == Some("youtu.be") {
        url.path().trim_matches('/').to_owned()
    } else {
        url.query_pairs()
            .find(|(key, _)| key == "v")
            .map(|(_, value)| value.into_owned())
            .ok_or("Titre YouTube invalide.")?
    };
    if video.is_empty()
        || video.len() > 160
        || !video
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err("Identifiant YouTube invalide.".into());
    }
    // Scope stdin-supplied cookies to .youtube.com, covering Music and the player API.
    Ok(format!("https://youtube.com/watch?v={video}"))
}

pub fn resolve_audio(
    url: &str,
    session: Option<&Session>,
    cancelled: Arc<AtomicBool>,
) -> Result<MediaInfo, String> {
    resolve_audio_interruptible(url, session, cancelled, None)
}
pub fn resolve_audio_interruptible(
    url: &str,
    session: Option<&Session>,
    cancelled: Arc<AtomicBool>,
    interrupted: Option<&AtomicBool>,
) -> Result<MediaInfo, String> {
    resolve_cached(url, session, cancelled, interrupted, false, false)
}

pub(crate) fn resolve_audio_for_playback(
    url: &str,
    session: Option<&Session>,
    cancelled: Arc<AtomicBool>,
    normalize: bool,
) -> Result<MediaInfo, String> {
    if normalize {
        resolve_cached(url, session, cancelled, None, false, true)
    } else {
        resolve_audio(url, session, cancelled)
    }
}

pub(crate) fn resolve_audio_fresh(
    url: &str,
    session: Option<&Session>,
    cancelled: Arc<AtomicBool>,
    interrupted: &AtomicBool,
) -> Result<MediaInfo, String> {
    resolve_cached(url, session, cancelled, Some(interrupted), true, false)
}

fn resolve_cached(
    url: &str,
    session: Option<&Session>,
    cancelled: Arc<AtomicBool>,
    interrupted: Option<&AtomicBool>,
    fresh: bool,
    normalize: bool,
) -> Result<MediaInfo, String> {
    let source_url = canonical_url(url)?;
    let stopped = || {
        cancelled.load(Ordering::Relaxed)
            || interrupted.is_some_and(|token| token.load(Ordering::SeqCst))
    };
    if stopped() {
        return Err("Préparation annulée.".into());
    }
    let (cached, scope, epoch) = crate::resolution_cache::lookup(&source_url, session, fresh);
    if let Some(info) = cached {
        if normalize {
            crate::direct_audio::ensure_loudness(&source_url, session, &cancelled);
        }
        if stopped() {
            return Err("Préparation annulée.".into());
        }
        return Ok(info);
    }
    if let Ok(info) = crate::direct_audio::resolve(&source_url, session, &cancelled, interrupted) {
        if stopped() {
            return Err("Préparation annulée.".into());
        }
        crate::resolution_cache::store(scope, &info, epoch);
        return Ok(info);
    }
    let result = thread::scope(|threads| {
        if normalize && crate::direct_audio::cached_loudness(&source_url).is_none() {
            threads.spawn(|| {
                crate::direct_audio::ensure_loudness(&source_url, session, &cancelled);
            });
        }
        resolve_with_ytdlp(&source_url, session, cancelled.clone(), interrupted)
    });
    let info = result?;
    if stopped() {
        return Err("Préparation annulée.".into());
    }
    crate::resolution_cache::store(scope, &info, epoch);
    Ok(info)
}

pub(crate) fn resolve_with_ytdlp(
    source_url: &str,
    session: Option<&Session>,
    cancelled: Arc<AtomicBool>,
    interrupted: Option<&AtomicBool>,
) -> Result<MediaInfo, String> {
    let started = Instant::now();
    if session.is_some() && crate::direct_audio::music_unavailable(source_url) {
        return resolve_ytdlp_mode(
            source_url,
            session,
            cancelled,
            interrupted,
            false,
            Duration::from_secs(75),
        );
    }
    let result = resolve_ytdlp_mode(
        source_url,
        session,
        cancelled.clone(),
        interrupted,
        true,
        Duration::from_secs(25),
    );
    if result.is_ok()
        || session.is_none()
        || cancelled.load(Ordering::Relaxed)
        || interrupted.is_some_and(|token| token.load(Ordering::SeqCst))
    {
        return result;
    }
    resolve_ytdlp_mode(
        source_url,
        session,
        cancelled,
        interrupted,
        false,
        Duration::from_secs(75).saturating_sub(started.elapsed()),
    )
}

fn resolve_ytdlp_mode(
    source_url: &str,
    session: Option<&Session>,
    cancelled: Arc<AtomicBool>,
    interrupted: Option<&AtomicBool>,
    music_only: bool,
    timeout: Duration,
) -> Result<MediaInfo, String> {
    let mut command = ytdlp_command();
    command.args([
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-cache-dir",
        "--no-playlist",
        "--no-warnings",
        "--quiet",
        "--socket-timeout",
        "12",
        "--retries",
        "1",
        "--skip-download",
        "--dump-single-json",
        "--format",
        AUDIO_FORMATS,
    ]);
    command.arg("--js-runtimes").arg(node_runtime());
    let config = session.map(session_config);
    if config.is_some() {
        // All supported deployments must use a release containing the cookie-scope fix.
        require_safe_ytdlp()?;
        command.args([
            "--config-locations",
            "-",
            "--extractor-args",
            if music_only {
                "youtube:player_client=web_music"
            } else {
                "youtube:player_client=default,web_music"
            },
        ]);
    }
    command.arg("--").arg(source_url);
    let output = run(
        command,
        config.as_deref().map(|s| s.as_str()),
        &cancelled,
        interrupted,
        timeout,
    )?;
    let resolved: Resolved =
        serde_json::from_slice(&output).map_err(|_| "Réponse audio YouTube non reconnue.")?;
    if !media_url_allowed(&resolved.url) {
        return Err("YouTube n’a pas fourni de flux audio direct compatible.".into());
    }
    if !(resolved.quality.codec == "opus" || resolved.quality.codec.starts_with("mp4a")) {
        return Err("Le codec de ce flux n’est pas pris en charge.".into());
    }
    let size = resolved
        .filesize
        .or_else(|| {
            Url::parse(&resolved.url)
                .ok()?
                .query_pairs()
                .find(|(key, _)| key == "clen")?
                .1
                .parse()
                .ok()
        })
        .unwrap_or(0);
    Ok(MediaInfo {
        url: Zeroizing::new(resolved.url),
        source_url: source_url.into(),
        quality: resolved.quality,
        bytes: size,
        duration: resolved
            .duration
            .filter(|n| n.is_finite() && *n > 0.0 && *n < 604800.0)
            .map(Duration::from_secs_f64),
        user_agent: resolved
            .http_headers
            .user_agent
            .unwrap_or_else(|| "Mozilla/5.0".into()),
    })
}

fn session_config(session: &Session) -> Zeroizing<String> {
    let mut config = Zeroizing::new(String::from("--add-headers 'Cookie: "));
    // POSIX quoting for yt-dlp's own config parser, not a shell command.
    for ch in session.cookie_header().chars() {
        if ch == '\'' {
            config.push_str("'\\''");
        } else {
            config.push(ch);
        }
    }
    config.push_str("'\n");
    config
}

fn hidden_command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}
fn bundled_tool(root: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let path = root.join("bin").join(name);
    path.is_file().then_some(path)
}
fn ytdlp_command() -> Command {
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| bundled_tool(p.parent()?, "yt-dlp.exe"));
    let mut command = path.map_or_else(|| hidden_command("yt-dlp"), Command::new);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}
fn node_runtime() -> std::ffi::OsString {
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| bundled_tool(p.parent()?, "node.exe"));
    match path {
        Some(path) => {
            let mut value = std::ffi::OsString::from("node:");
            value.push(path);
            value
        }
        None => "node".into(),
    }
}

fn require_safe_ytdlp() -> Result<(), String> {
    static VERSION: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *VERSION.get_or_init(|| {
        ytdlp_command()
            .arg("--version")
            .output()
            .ok()
            .is_some_and(|output| {
                let version = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<u32> = version
                    .trim()
                    .split('.')
                    .take(3)
                    .filter_map(|v| v.parse().ok())
                    .collect();
                parts.as_slice() >= [2023, 7, 6].as_slice()
            })
    }) {
        Ok(())
    } else {
        Err("Mets yt-dlp à jour pour utiliser la session Google en lecture.".into())
    }
}

fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = hidden_command("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn capture(mut reader: impl Read) -> Zeroizing<Vec<u8>> {
    let mut bytes = Zeroizing::new(Vec::new());
    let mut chunk = [0_u8; 4096];
    while let Ok(size) = reader.read(&mut chunk) {
        if size == 0 {
            break;
        }
        if bytes.len() < 8 * 1024 * 1024 {
            bytes.extend_from_slice(&chunk[..size]);
        }
    }
    bytes
}

fn run(
    mut command: Command,
    config: Option<&str>,
    cancelled: &AtomicBool,
    interrupted: Option<&AtomicBool>,
    timeout: Duration,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let stopped = || {
        cancelled.load(Ordering::Relaxed)
            || interrupted.is_some_and(|token| token.load(Ordering::SeqCst))
    };
    if stopped() {
        return Err("Préparation annulée.".into());
    }
    let mut child = command
        .stdin(if config.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "Impossible de démarrer yt-dlp. Vérifie son installation.")?;
    let stdout = child.stdout.take().ok_or("Sortie yt-dlp indisponible.")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Diagnostic yt-dlp indisponible.")?;
    let output = thread::spawn(move || capture(stdout));
    let errors = thread::spawn(move || capture(stderr));
    if let (Some(config), Some(mut input)) = (config, child.stdin.take()) {
        if input.write_all(config.as_bytes()).is_err() {
            kill_tree(&mut child);
            return Err("Impossible de transmettre la session à yt-dlp.".into());
        }
    }
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(_) => {
                kill_tree(&mut child);
                break Err("Impossible de suivre la préparation audio.");
            }
            Ok(None) => {}
        }
        if stopped() {
            kill_tree(&mut child);
            break Err("Préparation annulée.");
        }
        if started.elapsed() > timeout {
            kill_tree(&mut child);
            break Err("YouTube met trop de temps à fournir le flux. Réessaie.");
        }
        thread::sleep(Duration::from_millis(50));
    };
    let bytes = output.join().map_err(|_| "Sortie audio interrompue.")?;
    let errors = errors.join().map_err(|_| "Diagnostic audio interrompu.")?;
    if !status.map_err(str::to_owned)?.success() {
        // Never echo stderr: authenticated errors can contain cookies or signed media URLs.
        let message = String::from_utf8_lossy(&errors);
        return Err(
            if message.contains("Sign in") || message.contains("cookies") {
                "YouTube n’a pas accepté la lecture avec cette session. Reconnecte-toi ou réessaie."
            } else {
                "Impossible d’obtenir le flux audio de ce titre. Vérifie la connexion et yt-dlp."
            }
            .into(),
        );
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_resolution_does_not_reuse_or_start_a_stream() {
        let cancelled = Arc::new(AtomicBool::new(true));
        assert!(resolve_audio("https://youtube.com/watch?v=cancelled", None, cancelled).is_err());
    }

    #[test]
    #[ignore = "Session DPAPI, réseau et yt-dlp ; compare une préparation froide et une reprise en cache, sans sortie audio."]
    fn cached_preparation_preserves_premium_and_measures_replay_latency() {
        use crate::provider::Provider;
        let session = crate::session::load()
            .unwrap()
            .expect("Session enregistrée");
        let provider = crate::provider::YouTubeMusic::new(&session).unwrap();
        let playlist = provider
            .library_playlists(&session, None)
            .unwrap()
            .items
            .remove(0);
        let track = provider
            .playlist_tracks(&session, &playlist.id, None)
            .unwrap()
            .items
            .remove(0);
        let url = format!("https://music.youtube.com/watch?v={}", track.video_id);
        crate::resolution_cache::clear();
        let resolver = crate::provider::YouTubeStreamResolver;
        let started = Instant::now();
        let first = resolver
            .prepare_with_level(
                url.clone(),
                Some(session.clone()),
                Arc::new(AtomicBool::new(false)),
                true,
                true,
            )
            .unwrap();
        let cold = started.elapsed();
        let quality = first.quality.clone();
        drop(first);
        let canonical = canonical_url(&url).unwrap();
        assert!(
            crate::resolution_cache::lookup(&canonical, Some(&session), false)
                .0
                .is_some()
        );
        let started = Instant::now();
        let replay = resolver
            .prepare_with_level(
                url,
                Some(session),
                Arc::new(AtomicBool::new(false)),
                true,
                true,
            )
            .unwrap();
        let warm = started.elapsed();
        assert_eq!(quality, replay.quality);
        eprintln!("Même qualité {} : préparation froide + 512 Kio={cold:?} ; reprise en cache + 512 Kio={warm:?} ; aucun fichier audio ni sortie sonore.", quality.label());
    }
    #[test]
    #[ignore = "Session DPAPI et réseau ; compare la résolution multi-client et Music seule, puis vérifie le préchargement du meilleur format."]
    fn authenticated_resolver_benchmark() {
        use crate::provider::Provider;
        let session = crate::session::load().unwrap().unwrap();
        let provider = crate::provider::YouTubeMusic::new(&session).unwrap();
        let playlist = provider
            .library_playlists(&session, None)
            .unwrap()
            .items
            .remove(0);
        let track = provider
            .playlist_tracks(&session, &playlist.id, None)
            .unwrap()
            .items
            .remove(0);
        let source = format!("https://youtube.com/watch?v={}", track.video_id);
        for music_only in [false, true] {
            let cancelled = Arc::new(AtomicBool::new(false));
            let started = Instant::now();
            let info = resolve_ytdlp_mode(
                &source,
                Some(&session),
                cancelled.clone(),
                None,
                music_only,
                Duration::from_secs(40),
            )
            .unwrap();
            eprintln!(
                "yt-dlp music_only={music_only}: {}, resolve={:?}",
                info.quality.label(),
                started.elapsed()
            );
            let stream =
                crate::native_stream::NativeStream::open(info, Some(session.clone()), cancelled)
                    .unwrap();
            stream.prebuffer().unwrap();
            eprintln!(
                "yt-dlp music_only={music_only}: resolve + first audio blocks={:?}",
                started.elapsed()
            );
        }
    }
    #[test]
    fn quality_tiers_use_real_bitrate_and_cover_every_boundary() {
        for (rate, tier) in [
            (0.1, QualityTier::Standard),
            (150.0, QualityTier::Standard),
            (155.0, QualityTier::Standard),
            (159.9, QualityTier::Standard),
            (160.0, QualityTier::High),
            (249.9, QualityTier::High),
            (250.0, QualityTier::Master),
            (256.0, QualityTier::Master),
            (258.0, QualityTier::Master),
        ] {
            assert_eq!(
                AudioQuality {
                    bitrate_kbps: Some(rate),
                    ..Default::default()
                }
                .tier(),
                Some(tier)
            );
        }
        for rate in [None, Some(f64::NAN), Some(-1.0), Some(0.0)] {
            assert_eq!(
                AudioQuality {
                    bitrate_kbps: rate,
                    ..Default::default()
                }
                .tier(),
                None
            );
        }
        assert_eq!(
            AudioQuality {
                format_id: "141".into(),
                ..Default::default()
            }
            .tier(),
            Some(QualityTier::Master)
        );
    }
    #[test]
    fn only_trusted_https_urls_are_accepted() {
        assert!(check_url("https://youtu.be/abc").is_ok());
        for bad in [
            "http://youtu.be/abc",
            "https://youtube.com.evil.test/",
            "https://user:secret@youtube.com/",
            "https://youtube.com:1234/",
        ] {
            assert!(check_url(bad).is_err());
        }
        assert_eq!(
            canonical_url("https://music.youtube.com/watch?v=abc").unwrap(),
            "https://youtube.com/watch?v=abc"
        );
        assert!(media_url_allowed(
            "https://rr1---example.googlevideo.com/videoplayback?token=example"
        ));
        assert!(!media_url_allowed(
            "https://googlevideo.com.evil.test/audio"
        ));
    }
    #[test]
    fn session_config_is_private_input_and_escapes_quotes() {
        let session = Session::from_cookies([
            ("SAPISID".into(), "test'quoted".into()),
            ("SID".into(), "test".into()),
        ])
        .unwrap();
        let config = session_config(&session);
        assert!(config.contains("test'\\''quoted"));
        assert_eq!(config.lines().count(), 1);
        assert!(config.starts_with("--add-headers 'Cookie: "));
        assert_eq!(
            AudioQuality {
                format_id: "774".into(),
                codec: "opus".into(),
                bitrate_kbps: None
            }
            .label(),
            "Opus · 256 kbit/s"
        );
    }
}
