//! Native Innertube resolution, including signature/n solving. yt-dlp remains the fallback.
use crate::{
    http_audio::AudioHttp,
    player_solver, provider,
    session::Session,
    youtube::{self, AudioQuality, MediaInfo},
};
use reqwest::{header::HeaderMap, Client};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/143.0.0.0 Safari/537.36";
const LIMIT: usize = 4 * 1024 * 1024;
#[derive(Clone)]
struct Config {
    owner: String,
    version: String,
    visitor: Option<String>,
    at: Instant,
}
static CONFIG: Mutex<Option<Config>> = Mutex::new(None);
static LEVELS: Mutex<Vec<(String, Option<f32>)>> = Mutex::new(Vec::new());
fn video_key(source: &str) -> Option<String> {
    url::Url::parse(source)
        .ok()?
        .query_pairs()
        .find(|(key, _)| key == "v")
        .map(|(_, value)| value.into_owned())
}
pub(crate) fn cached_loudness(source: &str) -> Option<Option<f32>> {
    let key = video_key(source)?;
    LEVELS
        .lock()
        .ok()?
        .iter()
        .rev()
        .find(|(id, _)| id == &key)
        .map(|(_, level)| *level)
}
pub(crate) fn ensure_loudness(
    source: &str,
    session: Option<&Session>,
    cancelled: &AtomicBool,
) -> Option<f32> {
    if let Some(level) = cached_loudness(source) {
        return level;
    }
    let _ = attempt_player(source, session, cancelled, None, true);
    cached_loudness(source).flatten()
}
static NOT_MUSIC: Mutex<Vec<(String, Instant)>> = Mutex::new(Vec::new());
pub(crate) fn music_unavailable(source: &str) -> bool {
    NOT_MUSIC.lock().ok().is_some_and(|entries| {
        entries
            .iter()
            .any(|(url, at)| url == source && at.elapsed() < Duration::from_secs(300))
    })
}
fn key(session: &Session) -> String {
    use sha1::{Digest, Sha1};
    format!("{:x}", Sha1::digest(session.cookie_header().as_bytes()))
}
pub(crate) fn prime(session: &Session, html: &str) {
    if let Some(version) =
        provider::config_string(html, "INNERTUBE_CLIENT_VERSION").filter(|v| v.len() <= 80)
    {
        if let Ok(mut config) = CONFIG.lock() {
            *config = Some(Config {
                owner: key(session),
                version,
                visitor: provider::config_string(html, "VISITOR_DATA").filter(|v| v.len() <= 4096),
                at: Instant::now(),
            });
        }
    }
}
fn primed(session: &Session) -> Option<Config> {
    CONFIG
        .lock()
        .ok()?
        .as_ref()
        .filter(|config| {
            config.owner == key(session) && config.at.elapsed() < Duration::from_secs(300)
        })
        .cloned()
}

pub(crate) fn resolve(
    source: &str,
    session: Option<&Session>,
    cancelled: &AtomicBool,
    interrupted: Option<&AtomicBool>,
) -> Result<MediaInfo, ()> {
    static COOLDOWN: Mutex<[Option<Instant>; 2]> = Mutex::new([None, None]);
    let mode = usize::from(session.is_some());
    if COOLDOWN.lock().map_err(|_| ())?[mode].is_some_and(|until| Instant::now() < until) {
        return Err(());
    }
    let result = attempt(source, session, cancelled, interrupted);
    if result.is_err()
        && !cancelled.load(Ordering::Acquire)
        && !interrupted.is_some_and(|a| a.load(Ordering::Acquire))
    {
        COOLDOWN.lock().map_err(|_| ())?[mode] = Some(Instant::now() + Duration::from_secs(30));
    }
    result
}

fn attempt(
    source: &str,
    session: Option<&Session>,
    cancelled: &AtomicBool,
    interrupted: Option<&AtomicBool>,
) -> Result<MediaInfo, ()> {
    attempt_player(source, session, cancelled, interrupted, false)
}

fn attempt_player(
    source: &str,
    session: Option<&Session>,
    cancelled: &AtomicBool,
    interrupted: Option<&AtomicBool>,
    metadata_only: bool,
) -> Result<MediaInfo, ()> {
    let http = AudioHttp::new();
    let never = AtomicBool::new(false);
    let abort = interrupted.unwrap_or(&never);
    let client = Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2))
        .user_agent(UA)
        .build()
        .map_err(|_| ())?;
    let video = url::Url::parse(source)
        .map_err(|_| ())?
        .query_pairs()
        .find(|(key, _)| key == "v")
        .map(|(_, value)| value.into_owned())
        .ok_or(())?;
    let (endpoint, mut headers, context) = if let Some(session) = session {
        static VERSION: OnceLock<Mutex<Option<(String, Instant)>>> = OnceLock::new();
        let cache = VERSION.get_or_init(|| Mutex::new(None));
        let cached = primed(session).map(|config| config.version).or_else(|| {
            cache
                .lock()
                .ok()?
                .as_ref()
                .filter(|(_, at)| at.elapsed() < Duration::from_secs(300))
                .map(|(v, _)| v.clone())
        });
        let version = if let Some(version) = cached {
            version
        } else {
            let headers = auth(session)?;
            let bytes = http
                .execute(cancelled, abort, async {
                    let response = client
                        .get("https://music.youtube.com/")
                        .headers(headers)
                        .send()
                        .await?
                        .error_for_status()?;
                    bounded(response).await
                })
                .map_err(|_| ())?;
            let html = Zeroizing::new(String::from_utf8(bytes).map_err(|_| ())?);
            prime(session, &html);
            let version = provider::config_string(&html, "INNERTUBE_CLIENT_VERSION")
                .filter(|v| v.len() <= 80)
                .ok_or(())?;
            *cache.lock().map_err(|_| ())? = Some((version.clone(), Instant::now()));
            version
        };
        let mut headers = auth(session)?;
        headers.insert("x-youtube-client-name", "67".parse().map_err(|_| ())?);
        headers.insert("x-youtube-client-version", version.parse().map_err(|_| ())?);
        (
            "https://music.youtube.com/youtubei/v1/player?prettyPrint=false",
            headers,
            json!({"clientName":"WEB_REMIX","clientVersion":version,"hl":"fr","gl":"FR"}),
        )
    } else {
        // Public client configuration; no account cookies or subscription claims.
        let mut headers = HeaderMap::new();
        headers.insert("x-youtube-client-name", "101".parse().map_err(|_| ())?);
        headers.insert("x-youtube-client-version", "1.02".parse().map_err(|_| ())?);
        headers.insert("user-agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15".parse().map_err(|_|())?);
        (
            "https://www.youtube.com/youtubei/v1/player?prettyPrint=false",
            headers,
            json!({"clientName":"VISIONOS","clientVersion":"1.02","deviceMake":"Apple","deviceModel":"RealityDevice17,1","osName":"visionOS","osVersion":"26.5.23O471","hl":"fr"}),
        )
    };
    let cached_visitor = session.and_then(primed).and_then(|config| config.visitor);
    let visitor = if cached_visitor.is_some() {
        cached_visitor
    } else {
        let bootstrap = http
            .execute(cancelled, abort, async {
                let mut request = client.get("https://music.youtube.com/");
                if let Some(session) = session {
                    if let Ok(headers) = auth(session) {
                        request = request.headers(headers);
                    }
                }
                bounded(request.send().await?.error_for_status()?).await
            })
            .map_err(|_| ())?;
        let html = Zeroizing::new(String::from_utf8(bootstrap).map_err(|_| ())?);
        if let Some(session) = session {
            prime(session, &html);
        }
        provider::config_string(&html, "VISITOR_DATA").filter(|v| v.len() < 4096)
    };
    let mut context = context;
    if let Some(visitor) = visitor {
        headers.insert("x-goog-visitor-id", visitor.parse().map_err(|_| ())?);
        context["visitorData"] = json!(visitor);
    }
    headers.insert("content-type", "application/json".parse().map_err(|_| ())?);
    let player = if session.is_some() && !metadata_only {
        player_solver::prepare(cancelled, interrupted).ok()
    } else {
        None
    };
    let mut body = json!({"context":{"client":context},"videoId":video,"contentCheckOk":true,"racyCheckOk":true});
    if let Some(player) = &player {
        body["playbackContext"] = json!({"contentPlaybackContext":{"html5Preference":"HTML5_PREF_WANTS","signatureTimestamp":player.timestamp}});
    }
    let bytes = http
        .execute(cancelled, abort, async {
            let response = client
                .post(endpoint)
                .headers(headers)
                .json(&body)
                .send()
                .await?;
            #[cfg(test)]
            eprintln!("Direct player HTTP={}", response.status().as_u16());
            let response = response.error_for_status()?;
            bounded(response).await
        })
        .map_err(|_| ())?;
    if bytes.len() > LIMIT {
        return Err(());
    }
    let mut response: Value = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if response["playabilityStatus"]["status"].as_str() == Some("OK") {
        if let (Some(key), Ok(mut levels)) = (video_key(source), LEVELS.lock()) {
            let level = response["playerConfig"]["audioConfig"]["loudnessDb"]
                .as_f64()
                .filter(|value| value.is_finite() && (-60.0..=60.0).contains(value))
                .map(|value| value as f32);
            if levels.len() >= 128 {
                levels.remove(0);
            }
            levels.push((key, level));
        }
    }
    if metadata_only {
        return Err(());
    }
    if session.is_some() && response["playabilityStatus"]["status"].as_str() != Some("OK") {
        if let Ok(mut entries) = NOT_MUSIC.lock() {
            if entries.len() >= 64 {
                entries.remove(0);
            }
            entries.push((source.into(), Instant::now()));
        }
    }
    decipher(
        &mut response,
        session.is_some(),
        player.as_ref(),
        cancelled,
        interrupted,
    )?;
    let info = parse(&response, source, session.is_some()).ok_or(())?;
    // A usable URL is required before selecting this fast path. CDN requests carry no account headers.
    let mut probe = url::Url::parse(&info.url).map_err(|_| ())?;
    probe.query_pairs_mut().append_pair("range", "0-31");
    let bytes = http
        .execute(cancelled, abort, async {
            let response = client
                .get(probe)
                .header("User-Agent", &info.user_agent)
                .send()
                .await?
                .error_for_status()?;
            bounded_to(response, 32).await
        })
        .map_err(|_| ())?;
    if bytes.len() != 32 {
        return Err(());
    }
    Ok(info)
}

fn auth(session: &Session) -> Result<HeaderMap, ()> {
    provider::auth_headers(
        session,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ())?
            .as_secs(),
    )
    .map_err(|_| ())
}

async fn bounded(response: reqwest::Response) -> Result<Vec<u8>, reqwest::Error> {
    bounded_to(response, LIMIT).await
}

async fn bounded_to(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, reqwest::Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = (limit + 1).saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if bytes.len() > limit {
            break;
        }
    }
    Ok(bytes)
}

fn parse(response: &Value, source: &str, authenticated: bool) -> Option<MediaInfo> {
    if response["playabilityStatus"]["status"].as_str()? != "OK" {
        return None;
    }
    let formats = response["streamingData"]["adaptiveFormats"].as_array()?;
    let order = format_order(formats, authenticated);
    for itag in order {
        let Some(format) = formats
            .iter()
            .find(|format| format["itag"].as_u64() == Some(*itag))
        else {
            continue;
        };
        if format
            .get("drmFamilies")
            .is_some_and(|drm| !drm.as_array().is_some_and(Vec::is_empty))
        {
            continue;
        }
        let Some(url) = format["url"]
            .as_str()
            .filter(|url| youtube::media_url_allowed(url))
        else {
            continue;
        };
        let mime = format["mimeType"].as_str().unwrap_or_default();
        let codec = if mime.starts_with("audio/webm") && mime.contains("opus") {
            "opus"
        } else if mime.starts_with("audio/mp4") && mime.contains("mp4a") {
            "mp4a.40.2"
        } else {
            continue;
        };
        let bytes = format["contentLength"]
            .as_str()
            .and_then(|value| value.parse().ok())
            .filter(|size: &u64| *size > 0 && *size <= 2 * 1024 * 1024 * 1024)?;
        let duration = response["videoDetails"]["lengthSeconds"]
            .as_str()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0 && *v < 604800)
            .map(Duration::from_secs);
        return Some(MediaInfo {
            url: Zeroizing::new(url.into()),
            source_url: source.into(),
            bytes,
            duration,
            quality: AudioQuality {
                format_id: itag.to_string(),
                codec: codec.into(),
                bitrate_kbps: format["averageBitrate"]
                    .as_f64()
                    .or_else(|| format["bitrate"].as_f64())
                    .map(|v| v / 1000.0),
            },
            user_agent: UA.into(),
        });
    }
    None
}

fn decipher(
    response: &mut Value,
    authenticated: bool,
    player: Option<&player_solver::Player>,
    cancelled: &AtomicBool,
    interrupted: Option<&AtomicBool>,
) -> Result<(), ()> {
    let formats = response["streamingData"]["adaptiveFormats"]
        .as_array_mut()
        .ok_or(())?;
    let order = format_order(formats, authenticated);
    for itag in order {
        let Some(format) = formats
            .iter_mut()
            .find(|f| f["itag"].as_u64() == Some(*itag))
        else {
            continue;
        };
        if format
            .get("drmFamilies")
            .is_some_and(|d| !d.as_array().is_some_and(Vec::is_empty))
        {
            continue;
        }
        let (mut url, signature, parameter) = challenges(format)?;
        let n = url
            .query_pairs()
            .find(|(key, _)| key == "n")
            .map(|(_, value)| value.into_owned());
        if n.is_some() || signature.is_some() {
            let prepared;
            let player = if let Some(player) = player {
                player
            } else if !authenticated {
                prepared = player_solver::prepare(cancelled, interrupted)?;
                &prepared
            } else {
                return Err(());
            };
            let solved = player_solver::solve(player, n, signature, cancelled, interrupted)?;
            apply_solution(&mut url, solved, parameter.as_deref())?;
        }
        format["url"] = json!(url.as_str());
        return Ok(());
    }
    Err(())
}
fn format_order(formats: &[Value], authenticated: bool) -> &'static [u64] {
    if authenticated
        && formats
            .iter()
            .any(|format| matches!(format["itag"].as_u64(), Some(774 | 141)))
    {
        &[774, 141]
    } else {
        &[251, 140]
    }
}
fn challenges(format: &Value) -> Result<(url::Url, Option<String>, Option<String>), ()> {
    let (raw, signature, parameter) = if let Some(url) = format["url"].as_str() {
        (url.to_owned(), None, None)
    } else {
        let cipher = format["signatureCipher"]
            .as_str()
            .filter(|s| s.len() <= 96 * 1024)
            .ok_or(())?;
        let pairs: Vec<_> = url::form_urlencoded::parse(cipher.as_bytes()).collect();
        let get = |key: &str| -> Result<Option<String>, ()> {
            let mut values = pairs.iter().filter(|(k, _)| k == key);
            let value = values.next().map(|(_, v)| v.to_string());
            if values.next().is_some() {
                return Err(());
            }
            Ok(value)
        };
        let raw = get("url")?.ok_or(())?;
        let signature = get("s")?
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .ok_or(())?;
        let parameter = get("sp")?.unwrap_or_else(|| "signature".into());
        if !matches!(parameter.as_str(), "sig" | "signature" | "lsig") {
            return Err(());
        }
        (raw, Some(signature), Some(parameter))
    };
    if raw.len() > 64 * 1024 || !youtube::media_url_allowed(&raw) {
        return Err(());
    }
    Ok((url::Url::parse(&raw).map_err(|_| ())?, signature, parameter))
}
fn apply_solution(
    url: &mut url::Url,
    solution: player_solver::Solution,
    parameter: Option<&str>,
) -> Result<(), ()> {
    let mut pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    if let Some(n) = solution.n {
        for (key, value) in &mut pairs {
            if key == "n" {
                *value = n.clone();
            }
        }
    }
    if let Some(signature) = solution.signature {
        let parameter = parameter.ok_or(())?;
        pairs.retain(|(key, _)| key != parameter);
        pairs.push((parameter.into(), signature));
    }
    url.query_pairs_mut().clear().extend_pairs(pairs);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cipher_decoding_preserves_cdn_parameters_and_rejects_invalid_targets() {
        let cipher = url::form_urlencoded::Serializer::new(String::new())
            .append_pair(
                "url",
                "https://test.googlevideo.com/videoplayback?expire=9999999999&n=before&x=a%2Bb",
            )
            .append_pair("s", "challenge")
            .append_pair("sp", "sig")
            .finish();
        let (mut url, sig, parameter) = challenges(&json!({"signatureCipher":cipher})).unwrap();
        assert_eq!(sig.as_deref(), Some("challenge"));
        apply_solution(
            &mut url,
            player_solver::Solution {
                n: Some("after+value".into()),
                signature: Some("signed/value".into()),
            },
            parameter.as_deref(),
        )
        .unwrap();
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(pairs.get("n").unwrap(), "after+value");
        assert_eq!(pairs.get("sig").unwrap(), "signed/value");
        assert_eq!(pairs.get("x").unwrap(), "a+b");
        assert_eq!(pairs.get("expire").unwrap(), "9999999999");
        assert!(challenges(
            &json!({"signatureCipher": "s=a&sp=host&url=https%3A%2F%2Ftest.googlevideo.com%2Fa"})
        )
        .is_err());
        assert!(
            challenges(&json!({"signatureCipher": "s=a&url=https%3A%2F%2Fevil.test%2Fa"})).is_err()
        );
        assert!(challenges(
            &json!({"signatureCipher": "s=a&s=b&url=https%3A%2F%2Ftest.googlevideo.com%2Fa"})
        )
        .is_err());
    }
    #[test]
    #[ignore = "Session DPAPI et Internet ; prépare le moteur puis trois nouveaux titres, sans yt-dlp ni sortie sonore."]
    fn embedded_new_tracks_preserve_premium_and_measure_latency() {
        use crate::provider::Provider;
        use std::sync::Arc;
        let start = Instant::now();
        // Longer preparation allowance for a completely cold public player.
        let prepared = player_solver::prepare_cold_for_test();
        assert!(prepared.is_ok(), "Embedded player preparation failed");
        eprintln!("Embedded engine ready: {:?}", start.elapsed());
        let session = crate::session::load()
            .unwrap()
            .expect("Session enregistrée");
        let provider = provider::YouTubeMusic::new(&session).unwrap();
        let playlist = provider
            .library_playlists(&session, None)
            .unwrap()
            .items
            .remove(0);
        let tracks = provider
            .playlist_tracks(&session, &playlist.id, None)
            .unwrap()
            .items;
        for (index, track) in tracks.iter().take(3).enumerate() {
            let cancelled = Arc::new(AtomicBool::new(false));
            let source = format!("https://youtube.com/watch?v={}", track.video_id);
            let start = Instant::now();
            // Direct call proves this measurement cannot hide a yt-dlp fallback or URL-cache hit.
            let info = attempt(&source, Some(&session), &cancelled, None)
                .expect("Native authenticated resolution failed");
            let resolved = start.elapsed();
            assert!(matches!(info.quality.format_id.as_str(), "774" | "141"));
            let quality = info.quality.label();
            let stream = crate::native_stream::NativeStream::open(
                info,
                Some(session.clone()),
                cancelled.clone(),
            )
            .unwrap();
            // Manual selection starts decoding immediately; it does not wait for next-track prebuffering.
            let (control, mut pcm) = crate::native_stream::start(stream, false);
            let pcm_deadline = Instant::now() + Duration::from_secs(5);
            while control.buffering() && Instant::now() < pcm_deadline {
                for _ in 0..1024 {
                    assert!(pcm.next().is_some());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(control.take_error().is_none());
            assert!(!control.buffering(), "Initial PCM failed to arrive");
            assert!(pcm.next().is_some());
            control.set_paused(true);
            eprintln!(
                "New song {}: resolution={resolved:?}, first PCM={:?}, {quality}",
                index + 1,
                start.elapsed()
            );
        }
    }
    #[test]
    fn native_selection_preserves_quality_and_rejects_unusable_or_restricted_formats() {
        let source = "https://youtube.com/watch?v=test";
        let format = |itag: u64, url: &str| json!({"itag":itag,"url":url,"mimeType":"audio/mp4; codecs=\"mp4a.40.2\"","contentLength":"123456","averageBitrate":256000});
        let mut response = json!({"playabilityStatus":{"status":"OK"},"streamingData":{"adaptiveFormats":[format(140,"https://test.googlevideo.com/audio"),format(141,"https://test.googlevideo.com/premium")]}});
        assert_eq!(
            parse(&response, source, true).unwrap().quality.format_id,
            "141"
        );
        assert_eq!(
            parse(&response, source, false).unwrap().quality.format_id,
            "140"
        );
        response["streamingData"]["adaptiveFormats"][1]["url"] = json!("https://evil.test/audio");
        assert!(
            parse(&response, source, true).is_none(),
            "An account fast path must not silently fall back to standard"
        );
        response["playabilityStatus"]["status"] = json!("LOGIN_REQUIRED");
        assert!(parse(&response, source, false).is_none());
        response["playabilityStatus"]["status"] = json!("OK");
        response["streamingData"]["adaptiveFormats"] =
            json!([format(140, "https://test.googlevideo.com/standard")]);
        assert_eq!(
            parse(&response, source, true).unwrap().quality.format_id,
            "140"
        );
    }

    #[test]
    #[ignore = "Internet ; mesure la résolution directe anonyme et authentifiée sans jouer d'audio."]
    fn direct_resolution_smoke() {
        for session in [None, crate::session::load().unwrap()] {
            let start = Instant::now();
            let info = attempt(
                "https://youtube.com/watch?v=jNQXAC9IVRw",
                session.as_ref(),
                &AtomicBool::new(false),
                None,
            );
            match info {
                Ok(info) => eprintln!(
                    "Direct authenticated={}: {}, elapsed={:?}",
                    session.is_some(),
                    info.quality.label(),
                    start.elapsed()
                ),
                Err(()) => eprintln!(
                    "Direct authenticated={}: fallback required, elapsed={:?}",
                    session.is_some(),
                    start.elapsed()
                ),
            }
        }
        if let Some(session) = crate::session::load().unwrap() {
            use crate::provider::Provider;
            let provider = provider::YouTubeMusic::new(&session).unwrap();
            let playlist = provider
                .library_playlists(&session, None)
                .unwrap()
                .items
                .remove(0);
            let tracks = provider
                .playlist_tracks(&session, &playlist.id, None)
                .unwrap()
                .items;
            for track in tracks.iter().take(2) {
                let start = Instant::now();
                let source = format!("https://youtube.com/watch?v={}", track.video_id);
                match attempt(&source, Some(&session), &AtomicBool::new(false), None) {
                    Ok(info) => eprintln!(
                        "Direct library song: {}, elapsed={:?}",
                        info.quality.label(),
                        start.elapsed()
                    ),
                    Err(()) => eprintln!(
                        "Direct library song: fallback required, elapsed={:?}",
                        start.elapsed()
                    ),
                }
            }
        }
    }
}
