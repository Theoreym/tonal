//! Native metadata provider. Innertube is unofficial and may change without notice.
use crate::session::Session;
use reqwest::{
    blocking::Client,
    header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE},
    StatusCode,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    collections::HashSet,
    io::Read,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const ORIGIN: &str = "https://music.youtube.com";
const MAX_RESPONSE: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub cover_url: Option<String>,
    pub track_count: Option<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistChoice {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub contains: Option<bool>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaylistPrivacy {
    Public,
    #[default]
    Private,
    Unlisted,
}
impl PlaylistPrivacy {
    fn api(self) -> &'static str {
        match self {
            Self::Public => "PUBLIC",
            Self::Private => "PRIVATE",
            Self::Unlisted => "UNLISTED",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        match value {
            "PUBLIC" => Some(Self::Public),
            "PRIVATE" => Some(Self::Private),
            "UNLISTED" => Some(Self::Unlisted),
            _ => None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct PlaylistDetails {
    pub editable: bool,
    pub title: String,
    pub privacy: PlaylistPrivacy,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: Option<u32>,
    pub cover_url: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub given_name: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub premium: Option<bool>,
}
#[derive(Default)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub continuation: Option<String>,
}

#[derive(Debug)]
pub enum ProviderError {
    SessionExpired,
    Network,
    Schema,
    Service(u16),
    InvalidId,
    EditRejected,
    PlaylistReadOnly,
    PlaylistTitle,
    PlaylistMutation,
}
impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaylistReadOnly => {
                f.write_str("Cette playlist ne peut pas être modifiée avec ce compte.")
            }
            Self::PlaylistTitle => {
                f.write_str("Choisis un titre de 1 à 150 caractères, sans < ni >.")
            }
            Self::PlaylistMutation => f.write_str(
                "L’action n’a pas été confirmée. Actualise la bibliothèque avant de réessayer.",
            ),
            Self::SessionExpired => {
                f.write_str("Session expirée ou révoquée. Reconnecte-toi à Google.")
            }
            Self::Network => {
                f.write_str("Impossible de joindre YouTube Music. Vérifie le réseau et réessaie.")
            }
            Self::Schema => f.write_str(
                "Réponse YouTube Music non reconnue. L'API non officielle a peut-être changé.",
            ),
            Self::Service(code) => write!(f, "YouTube Music a refusé la requête (HTTP {code})."),
            Self::InvalidId => f.write_str("Identifiant de playlist invalide."),
            Self::EditRejected => f.write_str(
                "YouTube Music n’a pas confirmé l’ajout. Vérifie la playlist avant de réessayer.",
            ),
        }
    }
}

pub trait Provider {
    fn validate_session(&self, session: &Session) -> Result<Account, ProviderError>;
    fn library_playlists(
        &self,
        session: &Session,
        continuation: Option<&str>,
    ) -> Result<Page<Playlist>, ProviderError>;
    fn playlist_tracks(
        &self,
        session: &Session,
        id: &str,
        continuation: Option<&str>,
    ) -> Result<Page<Track>, ProviderError>;
    fn suggestions(&self, session: &Session, video_id: &str) -> Result<Vec<Track>, ProviderError>;
    fn playlist_choices(
        &self,
        session: &Session,
        video_id: &str,
    ) -> Result<Vec<PlaylistChoice>, ProviderError>;
    fn add_playlist_track(
        &self,
        session: &Session,
        playlist_id: &str,
        video_id: &str,
    ) -> Result<(), ProviderError>;
}

pub trait StreamResolver {
    fn prepare(
        &self,
        url: String,
        session: Option<Session>,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        prefetch: bool,
    ) -> Result<crate::native_stream::NativeStream, String>;
}
pub struct YouTubeStreamResolver;
impl StreamResolver for YouTubeStreamResolver {
    fn prepare(
        &self,
        url: String,
        session: Option<Session>,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        prefetch: bool,
    ) -> Result<crate::native_stream::NativeStream, String> {
        self.prepare_with_level(url, session, cancelled, prefetch, false)
    }
}
impl YouTubeStreamResolver {
    pub fn prepare_with_level(
        &self,
        url: String,
        session: Option<Session>,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        prefetch: bool,
        normalize: bool,
    ) -> Result<crate::native_stream::NativeStream, String> {
        let info = crate::youtube::resolve_audio_for_playback(
            &url,
            session.as_ref(),
            cancelled.clone(),
            normalize,
        )?;
        let stream = crate::native_stream::NativeStream::open(info, session, cancelled)?;
        if prefetch {
            stream.prebuffer()?;
        }
        Ok(stream)
    }
}

pub struct YouTubeMusic {
    client: Client,
    client_version: String,
    premium: Option<bool>,
}
impl YouTubeMusic {
    pub fn new(session: &Session) -> Result<Self, ProviderError> {
        let client = Client::builder().connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(25)).https_only(true)
            // Never forward session headers to a redirected origin.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/143.0.0.0 Safari/537.36")
            .build().map_err(|_| ProviderError::Network)?;
        let response = client
            .get(ORIGIN)
            .headers(auth_headers(session, now()?)?)
            .send()
            .map_err(|_| ProviderError::Network)?;
        check_status(response.status())?;
        let mut html = String::new();
        response
            .take(MAX_RESPONSE + 1)
            .read_to_string(&mut html)
            .map_err(|_| ProviderError::Network)?;
        if html.len() as u64 > MAX_RESPONSE {
            return Err(ProviderError::Schema);
        }
        // Read the current web client's version instead of pinning an expiring version.
        let client_version =
            config_string(&html, "INNERTUBE_CLIENT_VERSION").ok_or(ProviderError::Schema)?;
        if client_version.len() > 80 {
            return Err(ProviderError::Schema);
        }
        crate::direct_audio::prime(session, &html);
        Ok(Self {
            client,
            client_version,
            premium: config_bool(&html, "IS_SUBSCRIBER"),
        })
    }

    fn request(
        &self,
        session: &Session,
        endpoint: &str,
        mut body: Value,
        continuation: Option<&str>,
    ) -> Result<Value, ProviderError> {
        body["context"] = json!({"client": {"clientName": "WEB_REMIX", "clientVersion": self.client_version,
            "hl": "fr", "gl": "FR"}, "user": {}});
        let mut request = self
            .client
            .post(format!("{ORIGIN}/youtubei/v1/{endpoint}"))
            .query(&[("prettyPrint", "false")])
            .headers(auth_headers(session, now()?)?)
            .json(&body);
        if let Some(token) = continuation {
            if token.len() > 16 * 1024 {
                return Err(ProviderError::Schema);
            }
            request =
                request.query(&[("continuation", token), ("ctoken", token), ("type", "next")]);
        }
        let response = request.send().map_err(|_| ProviderError::Network)?;
        check_status(response.status())?;
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ProviderError::Network)?;
        if bytes.len() as u64 > MAX_RESPONSE {
            return Err(ProviderError::Schema);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| ProviderError::Schema)?;
        if value
            .pointer("/responseContext/mainAppWebResponseContext/loggedOut")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err(ProviderError::SessionExpired);
        }
        if let Some(error) = value.get("error") {
            let code = error.get("code").and_then(Value::as_u64).unwrap_or(500) as u16;
            return Err(if code == 401 || code == 403 {
                ProviderError::SessionExpired
            } else {
                ProviderError::Service(code)
            });
        }
        Ok(value)
    }
}

impl YouTubeMusic {
    pub fn playlist_details(
        &self,
        session: &Session,
        id: &str,
    ) -> Result<PlaylistDetails, ProviderError> {
        playlist_target(id)?;
        let response = self.request(
            session,
            "browse",
            json!({"browseId":format!("VL{id}")}),
            None,
        )?;
        parse_playlist_details(&response, id)
    }
    pub fn create_playlist(
        &self,
        session: &Session,
        title: &str,
        privacy: PlaylistPrivacy,
    ) -> Result<Playlist, ProviderError> {
        let title = playlist_title(title)?;
        let response = self.request(
            session,
            "playlist/create",
            json!({"title":title,"description":"","privacyStatus":privacy.api()}),
            None,
        )?;
        let id = confirmed_created_playlist(&response)?;
        Ok(Playlist {
            id,
            title: title.into(),
            subtitle: "0 titre".into(),
            cover_url: None,
            track_count: Some(0),
        })
    }
    #[cfg(test)]
    pub fn update_playlist(
        &self,
        session: &Session,
        id: &str,
        title: Option<&str>,
        privacy: Option<PlaylistPrivacy>,
    ) -> Result<PlaylistDetails, ProviderError> {
        self.update_playlist_guarded(
            session,
            id,
            title,
            privacy,
            &std::sync::atomic::AtomicBool::new(false),
        )
    }
    pub(crate) fn update_playlist_guarded(
        &self,
        session: &Session,
        id: &str,
        title: Option<&str>,
        privacy: Option<PlaylistPrivacy>,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<PlaylistDetails, ProviderError> {
        let body = update_playlist_body(id, title, privacy)?;
        let mut details = self.require_editable_playlist(session, id, cancelled)?;
        check_playlist_cancel(cancelled)?;
        let response = self.request(session, "browse/edit_playlist", body, None)?;
        confirm_playlist_mutation(&response)?;
        if let Some(title) = title {
            details.title = playlist_title(title)?.into();
        }
        if let Some(privacy) = privacy {
            details.privacy = privacy;
        }
        Ok(details)
    }
    #[cfg(test)]
    pub fn delete_playlist(&self, session: &Session, id: &str) -> Result<(), ProviderError> {
        self.delete_playlist_guarded(session, id, &std::sync::atomic::AtomicBool::new(false))
    }
    pub(crate) fn delete_playlist_guarded(
        &self,
        session: &Session,
        id: &str,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(), ProviderError> {
        playlist_target(id)?;
        self.require_editable_playlist(session, id, cancelled)?;
        check_playlist_cancel(cancelled)?;
        let response = self.request(session, "playlist/delete", json!({"playlistId":id}), None)?;
        if response.get("status").is_some() {
            return confirm_playlist_mutation(&response);
        }
        // This endpoint can acknowledge with responseContext only. Confirm disappearance,
        // never repeat a deletion because its response lacked a status field.
        for delay in [0, 150, 350] {
            check_playlist_cancel(cancelled)?;
            if delay > 0 {
                std::thread::sleep(Duration::from_millis(delay));
            }
            if !self.playlist_in_library(session, id, cancelled)? {
                return Ok(());
            }
        }
        Err(ProviderError::PlaylistMutation)
    }
    fn playlist_in_library(
        &self,
        session: &Session,
        id: &str,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<bool, ProviderError> {
        let started = std::time::Instant::now();
        let mut next: Option<String> = None;
        let mut seen = HashSet::new();
        for _ in 0..50 {
            check_playlist_cancel(cancelled)?;
            if started.elapsed() > Duration::from_secs(5) {
                return Err(ProviderError::PlaylistMutation);
            }
            let page = self.library_playlists(session, next.as_deref())?;
            if page.items.iter().any(|playlist| playlist.id == id) {
                return Ok(true);
            }
            let Some(token) = page.continuation else {
                return Ok(false);
            };
            if !seen.insert(token.clone()) {
                return Err(ProviderError::Schema);
            }
            next = Some(token);
        }
        Err(ProviderError::PlaylistMutation)
    }
    fn require_editable_playlist(
        &self,
        session: &Session,
        id: &str,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<PlaylistDetails, ProviderError> {
        // A newly created playlist can briefly lack its editable header. Only metadata reads repeat.
        for delay in [0, 150, 350] {
            check_playlist_cancel(cancelled)?;
            if delay > 0 {
                std::thread::sleep(Duration::from_millis(delay));
            }
            check_playlist_cancel(cancelled)?;
            let details = self.playlist_details(session, id)?;
            if details.editable {
                return Ok(details);
            }
        }
        Err(ProviderError::PlaylistReadOnly)
    }
}
fn check_playlist_cancel(cancelled: &std::sync::atomic::AtomicBool) -> Result<(), ProviderError> {
    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        Err(ProviderError::PlaylistMutation)
    } else {
        Ok(())
    }
}
pub(crate) fn playlist_title(title: &str) -> Result<&str, ProviderError> {
    let title = title.trim();
    if title.is_empty()
        || title.chars().count() > 150
        || title
            .chars()
            .any(|c| c.is_control() || matches!(c, '<' | '>'))
    {
        return Err(ProviderError::PlaylistTitle);
    }
    Ok(title)
}
fn playlist_target(id: &str) -> Result<(), ProviderError> {
    if valid_id(id) && id.starts_with("PL") && !automatic_playlist(id) {
        Ok(())
    } else {
        Err(ProviderError::InvalidId)
    }
}
fn update_playlist_body(
    id: &str,
    title: Option<&str>,
    privacy: Option<PlaylistPrivacy>,
) -> Result<Value, ProviderError> {
    playlist_target(id)?;
    let mut actions = Vec::new();
    if let Some(title) = title {
        actions.push(
            json!({"action":"ACTION_SET_PLAYLIST_NAME","playlistName":playlist_title(title)?}),
        );
    }
    if let Some(privacy) = privacy {
        actions
            .push(json!({"action":"ACTION_SET_PLAYLIST_PRIVACY","playlistPrivacy":privacy.api()}));
    }
    if actions.is_empty() {
        return Err(ProviderError::PlaylistMutation);
    }
    Ok(json!({"playlistId":id,"actions":actions}))
}
fn confirm_playlist_mutation(response: &Value) -> Result<(), ProviderError> {
    if response["status"].as_str() == Some("STATUS_SUCCEEDED") {
        Ok(())
    } else {
        Err(ProviderError::PlaylistMutation)
    }
}
fn confirmed_created_playlist(response: &Value) -> Result<String, ProviderError> {
    if response
        .get("status")
        .is_some_and(|s| s.as_str() != Some("STATUS_SUCCEEDED"))
    {
        return Err(ProviderError::PlaylistMutation);
    }
    let id = response["playlistId"]
        .as_str()
        .ok_or(ProviderError::PlaylistMutation)?;
    playlist_target(id).map_err(|_| ProviderError::PlaylistMutation)?;
    Ok(id.into())
}
fn parse_playlist_details(response: &Value, id: &str) -> Result<PlaylistDetails, ProviderError> {
    let Some(header) = find_key(response, "musicEditablePlaylistDetailHeaderRenderer") else {
        // Never infer ownership from a playlist being present in the library.
        return Ok(PlaylistDetails {
            editable: false,
            title: String::new(),
            privacy: PlaylistPrivacy::Private,
        });
    };
    let returned = header["playlistId"].as_str().ok_or(ProviderError::Schema)?;
    if returned.strip_prefix("VL").unwrap_or(returned) != id {
        return Err(ProviderError::Schema);
    }
    let privacy = header
        .pointer("/editHeader/musicPlaylistEditHeaderRenderer/privacy")
        .and_then(Value::as_str)
        .and_then(PlaylistPrivacy::parse)
        .ok_or(ProviderError::Schema)?;
    let title = header
        .pointer("/editHeader/musicPlaylistEditHeaderRenderer/title")
        .map(text)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            find_key(&header["header"], "musicResponsiveHeaderRenderer").map(|h| text(&h["title"]))
        })
        .unwrap_or_default();
    Ok(PlaylistDetails {
        editable: true,
        title,
        privacy,
    })
}

impl Provider for YouTubeMusic {
    fn validate_session(&self, session: &Session) -> Result<Account, ProviderError> {
        let response = self.request(session, "account/account_menu", json!({}), None)?;
        // A 200 response or the presence of cookies alone does not prove authentication.
        let mut account = parse_account(&response)?;
        account.premium = self.premium;
        Ok(account)
    }

    fn library_playlists(
        &self,
        session: &Session,
        continuation: Option<&str>,
    ) -> Result<Page<Playlist>, ProviderError> {
        parse_playlists(&self.request(
            session,
            "browse",
            json!({"browseId": "FEmusic_liked_playlists"}),
            continuation,
        )?)
    }

    fn playlist_tracks(
        &self,
        session: &Session,
        id: &str,
        continuation: Option<&str>,
    ) -> Result<Page<Track>, ProviderError> {
        if !valid_id(id) {
            return Err(ProviderError::InvalidId);
        }
        parse_tracks(&self.request(
            session,
            "browse",
            json!({"browseId": format!("VL{id}")}),
            continuation,
        )?)
    }

    fn playlist_choices(
        &self,
        session: &Session,
        video_id: &str,
    ) -> Result<Vec<PlaylistChoice>, ProviderError> {
        if !valid_id(video_id) {
            return Err(ProviderError::InvalidId);
        }
        parse_playlist_choices(&self.request(
            session,
            "playlist/get_add_to_playlist",
            json!({"videoIds":[video_id]}),
            None,
        )?)
    }

    fn add_playlist_track(
        &self,
        session: &Session,
        playlist_id: &str,
        video_id: &str,
    ) -> Result<(), ProviderError> {
        let body = add_playlist_body(playlist_id, video_id)?;
        let response = self.request(session, "browse/edit_playlist", body, None)?;
        confirm_playlist_add(&response, video_id)
    }

    fn suggestions(&self, session: &Session, video_id: &str) -> Result<Vec<Track>, ProviderError> {
        if !valid_id(video_id) {
            return Err(ProviderError::InvalidId);
        }
        let response = self.request(
            session,
            "next",
            json!({
                "videoId": video_id,
                "playlistId": format!("RDAMVM{video_id}"),
                "enablePersistentPlaylistPanel": true,
                "isAudioOnly": true,
                "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
                "params": "wAEB"
            }),
            None,
        )?;
        parse_suggestions(&response, video_id)
    }
}

fn now() -> Result<u64, ProviderError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ProviderError::Network)?
        .as_secs())
}
pub(crate) fn auth_headers(session: &Session, timestamp: u64) -> Result<HeaderMap, ProviderError> {
    let mut headers = HeaderMap::new();
    let mut cookie = HeaderValue::from_str(session.cookie_header())
        .map_err(|_| ProviderError::SessionExpired)?;
    cookie.set_sensitive(true);
    headers.insert(COOKIE, cookie);
    let mut authorization = HeaderValue::from_str(&authorization(session.sapisid(), timestamp))
        .map_err(|_| ProviderError::SessionExpired)?;
    authorization.set_sensitive(true);
    headers.insert(AUTHORIZATION, authorization);
    headers.insert("origin", HeaderValue::from_static(ORIGIN));
    headers.insert("x-origin", HeaderValue::from_static(ORIGIN));
    headers.insert("x-goog-authuser", HeaderValue::from_static("0"));
    Ok(headers)
}
fn authorization(sapisid: &str, timestamp: u64) -> String {
    let mut hash = Sha1::new();
    hash.update(timestamp.to_string());
    hash.update(b" ");
    hash.update(sapisid);
    hash.update(b" ");
    hash.update(ORIGIN);
    format!("SAPISIDHASH {timestamp}_{:x}", hash.finalize())
}
fn check_status(status: StatusCode) -> Result<(), ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ProviderError::SessionExpired),
        code if code.is_redirection() => Err(ProviderError::SessionExpired),
        code if !code.is_success() => Err(ProviderError::Service(code.as_u16())),
        _ => Ok(()),
    }
}

pub(crate) fn config_string(html: &str, key: &str) -> Option<String> {
    let (_, after) = html.split_once(&format!("\"{key}\""))?;
    let after = after.trim_start().strip_prefix(':')?.trim_start();
    String::deserialize(&mut serde_json::Deserializer::from_str(after)).ok()
}
fn config_bool(html: &str, key: &str) -> Option<bool> {
    let (_, after) = html.split_once(&format!("\"{key}\""))?;
    let after = after.trim_start().strip_prefix(':')?.trim_start();
    bool::deserialize(&mut serde_json::Deserializer::from_str(after)).ok()
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 160
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn text(value: &Value) -> String {
    if let Some(s) = value.get("simpleText").and_then(Value::as_str) {
        return s.to_owned();
    }
    value
        .get("runs")
        .and_then(Value::as_array)
        .map(|runs| {
            runs.iter()
                .filter_map(|r| r.get("text").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default()
}
fn thumbnail(value: &Value) -> Option<String> {
    find_key(value, "thumbnails")?
        .as_array()?
        .iter()
        .filter_map(|image| {
            let url = image.get("url")?.as_str()?;
            crate::artwork::allowed_url(url).then_some((
                image
                    .get("width")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .min(640),
                url,
            ))
        })
        .max_by_key(|(width, _)| *width)
        .map(|(_, url)| url.to_owned())
}
fn parse_account(value: &Value) -> Result<Account, ProviderError> {
    let header =
        find_key(value, "activeAccountHeaderRenderer").ok_or(ProviderError::SessionExpired)?;
    let name = text(&header["accountName"]);
    Ok(Account {
        given_name: name
            .split_whitespace()
            .next()
            .unwrap_or("Compte")
            .to_owned(),
        display_name: name,
        avatar_url: thumbnail(&header["accountPhoto"]),
        premium: None,
    })
}
fn automatic_playlist(id: &str) -> bool {
    matches!(id, "LM" | "SE" | "HL" | "WL") || id.starts_with("RD")
}
fn duration_seconds(text: &str) -> Option<u32> {
    let mut parts = text.trim().split(':');
    let first = parts.next()?.parse::<u32>().ok()?;
    let rest: Vec<_> = parts.collect();
    if rest.is_empty() || rest.len() > 2 {
        return None;
    }
    rest.into_iter().try_fold(first, |total, part| {
        let value = part.parse::<u32>().ok().filter(|n| *n < 60)?;
        total.checked_mul(60)?.checked_add(value)
    })
}

fn playlist_track_count(subtitle: &str) -> Option<u32> {
    subtitle.split(['•', '·']).find_map(|part| {
        let part = part.trim();
        if !part.starts_with(|ch: char| ch.is_ascii_digit()) {
            return None;
        }
        let end = part
            .char_indices()
            .find(|(_, ch)| !ch.is_ascii_digit() && !ch.is_whitespace() && !matches!(ch, ',' | '.'))
            .map(|(index, _)| index)?;
        let (number, unit) = part.split_at(end);
        if !matches!(
            unit.trim().to_lowercase().as_str(),
            "titre" | "titres" | "morceau" | "morceaux" | "song" | "songs" | "track" | "tracks"
        ) {
            return None;
        }
        number
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()
    })
}
fn walk(value: &Value, depth: usize, visitor: &mut impl FnMut(&serde_json::Map<String, Value>)) {
    if depth > 64 {
        return;
    }
    match value {
        Value::Object(map) => {
            visitor(map);
            for value in map.values() {
                walk(value, depth + 1, visitor);
            }
        }
        Value::Array(values) => {
            for value in values {
                walk(value, depth + 1, visitor);
            }
        }
        _ => {}
    }
}
fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    fn inner<'a>(v: &'a Value, key: &str, depth: usize) -> Option<&'a Value> {
        if depth > 64 {
            return None;
        }
        match v {
            Value::Object(map) => map
                .get(key)
                .or_else(|| map.values().find_map(|v| inner(v, key, depth + 1))),
            Value::Array(values) => values.iter().find_map(|v| inner(v, key, depth + 1)),
            _ => None,
        }
    }
    inner(value, key, 0)
}
fn page_root(value: &Value) -> Result<&Value, ProviderError> {
    value
        .get("contents")
        .or_else(|| value.get("continuationContents"))
        .or_else(|| value.get("onResponseReceivedActions"))
        .or_else(|| value.get("onResponseReceivedEndpoints"))
        .ok_or(ProviderError::Schema)
}
fn continuation(root: &Value) -> Option<String> {
    let mut token = None;
    walk(root, 0, &mut |map| {
        if token.is_some() {
            return;
        }
        for key in ["nextContinuationData", "continuationCommand"] {
            if let Some(value) = map
                .get(key)
                .and_then(|v| {
                    v.get(if key == "continuationCommand" {
                        "token"
                    } else {
                        "continuation"
                    })
                })
                .and_then(Value::as_str)
            {
                token = Some(value.to_owned());
            }
        }
    });
    token
}
fn parse_playlists(value: &Value) -> Result<Page<Playlist>, ProviderError> {
    let root = page_root(value)?;
    if ![
        "gridRenderer",
        "gridContinuation",
        "musicShelfRenderer",
        "appendContinuationItemsAction",
        "musicCarouselShelfRenderer",
    ]
    .iter()
    .any(|key| find_key(root, key).is_some())
    {
        return Err(ProviderError::Schema);
    }
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    walk(root, 0, &mut |map| {
        let Some(row) = map.get("musicTwoRowItemRenderer") else {
            return;
        };
        let Some(id) = row
            .pointer("/navigationEndpoint/browseEndpoint/browseId")
            .and_then(Value::as_str)
            .and_then(|s| s.strip_prefix("VL"))
        else {
            return;
        };
        let title = text(&row["title"]);
        if valid_id(id)
            && !automatic_playlist(id)
            && !title.is_empty()
            && seen.insert(id.to_owned())
        {
            items.push(Playlist {
                id: id.to_owned(),
                title,
                subtitle: text(&row["subtitle"]),
                cover_url: thumbnail(&row["thumbnailRenderer"])
                    .or_else(|| thumbnail(&row["thumbnail"])),
                track_count: playlist_track_count(&text(&row["subtitle"])),
            });
        }
    });
    Ok(Page {
        items,
        continuation: continuation(root),
    })
}
fn parse_tracks(value: &Value) -> Result<Page<Track>, ProviderError> {
    let root = page_root(value)?;
    // A browse response also contains recommendations and secondary shelves. Neither
    // their rows nor their continuation tokens belong to the selected playlist.
    let shelf = [
        "musicPlaylistShelfContinuation",
        "musicShelfContinuation",
        "musicPlaylistShelfRenderer",
    ]
    .iter()
    .find_map(|key| find_key(root, key))
    .or_else(|| root.get("musicShelfRenderer"))
    .or_else(|| {
        let shelves: Vec<_> = root
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(std::slice::from_ref(root))
            .iter()
            .filter_map(|action| action.get("appendContinuationItemsAction"))
            .collect();
        shelves
            .iter()
            .copied()
            .find(|shelf| {
                shelf
                    .get("targetId")
                    .and_then(Value::as_str)
                    .is_some_and(|target| target.contains("playlist"))
            })
            // YouTube also uses opaque target IDs. A single append is the
            // response to the playlist's own scoped continuation token.
            .or_else(|| (shelves.len() == 1).then(|| shelves[0]))
    })
    .ok_or(ProviderError::Schema)?;
    let rows = shelf
        .get("contents")
        .or_else(|| shelf.get("continuationItems"))
        .and_then(Value::as_array);
    let mut items = Vec::new();
    for entry in rows.into_iter().flatten() {
        let Some(row) = entry.get("musicResponsiveListItemRenderer") else {
            continue;
        };
        let Some(id) = row
            .pointer("/playlistItemData/videoId")
            .or_else(|| row.pointer("/navigationEndpoint/watchEndpoint/videoId"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let title =
            text(&row["flexColumns"][0]["musicResponsiveListItemFlexColumnRenderer"]["text"]);
        if valid_id(id) && !title.is_empty() {
            items.push(Track {
                video_id: id.to_owned(),
                title,
                artist: text(
                    &row["flexColumns"][1]["musicResponsiveListItemFlexColumnRenderer"]["text"],
                ),
                album: text(
                    &row["flexColumns"][2]["musicResponsiveListItemFlexColumnRenderer"]["text"],
                ),
                duration_secs: duration_seconds(&text(
                    &row["fixedColumns"][0]["musicResponsiveListItemFixedColumnRenderer"]["text"],
                )),
                cover_url: thumbnail(&row["thumbnail"]),
            });
        }
    }
    let next = shelf
        .get("continuations")
        .and_then(continuation)
        .or_else(|| {
            rows.into_iter()
                .flatten()
                .filter_map(|entry| entry.get("continuationItemRenderer"))
                .find_map(continuation)
        });
    Ok(Page {
        items,
        continuation: next,
    })
}

fn add_playlist_body(playlist_id: &str, video_id: &str) -> Result<Value, ProviderError> {
    if !valid_id(playlist_id) || automatic_playlist(playlist_id) || !valid_id(video_id) {
        return Err(ProviderError::InvalidId);
    }
    // No dedupe override: YouTube must reject an item already present.
    Ok(
        json!({"playlistId":playlist_id,"actions":[{"action":"ACTION_ADD_VIDEO","addedVideoId":video_id}]}),
    )
}

fn confirm_playlist_add(value: &Value, video_id: &str) -> Result<(), ProviderError> {
    if value.get("status").and_then(Value::as_str) == Some("STATUS_SUCCEEDED")
        && value
            .get("playlistEditResults")
            .and_then(Value::as_array)
            .is_some_and(|results| {
                results.iter().any(|result| {
                    let data = &result["playlistEditVideoAddedResultData"];
                    data.get("videoId").and_then(Value::as_str) == Some(video_id)
                        && data
                            .get("setVideoId")
                            .and_then(Value::as_str)
                            .is_some_and(|id| !id.is_empty())
                })
            })
    {
        Ok(())
    } else {
        Err(ProviderError::EditRejected)
    }
}

fn parse_playlist_choices(value: &Value) -> Result<Vec<PlaylistChoice>, ProviderError> {
    let root = find_key(value, "addToPlaylistRenderer").ok_or(ProviderError::Schema)?;
    let mut choices = Vec::new();
    let mut seen = HashSet::new();
    walk(root, 0, &mut |map| {
        let Some(row) = map.get("playlistAddToOptionRenderer") else {
            return;
        };
        let Some(id) = row
            .get("playlistId")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id) && !automatic_playlist(id))
        else {
            return;
        };
        let title = text(&row["title"]);
        let contains = match row.get("containsSelectedVideos").and_then(Value::as_str) {
            Some("ALL") => Some(true),
            Some("NONE") => Some(false),
            _ => None,
        };
        let endpoint = &row["addToPlaylistServiceEndpoint"]["playlistEditEndpoint"];
        let editable = endpoint.get("playlistId").and_then(Value::as_str) == Some(id)
            && endpoint
                .get("actions")
                .and_then(Value::as_array)
                .is_some_and(|actions| {
                    actions.iter().any(|action| {
                        action.get("action").and_then(Value::as_str) == Some("ACTION_ADD_VIDEO")
                    })
                });
        if (contains == Some(true) || editable) && !title.is_empty() && seen.insert(id.to_owned()) {
            choices.push(PlaylistChoice {
                id: id.to_owned(),
                title,
                cover_url: thumbnail(&row["thumbnailRenderer"])
                    .or_else(|| thumbnail(&row["thumbnail"])),
                contains,
            });
        }
    });
    Ok(choices)
}

fn parse_suggestions(value: &Value, seed: &str) -> Result<Vec<Track>, ProviderError> {
    let panel = find_key(page_root(value)?, "playlistPanelRenderer")
        .and_then(|panel| panel.get("contents"))
        .and_then(Value::as_array)
        .ok_or(ProviderError::Schema)?;
    let mut seen = HashSet::from([seed.to_owned()]);
    let mut tracks = Vec::new();
    for entry in panel {
        // Only the primary recording belongs in this list, not video counterparts or menus.
        let row = entry.get("playlistPanelVideoRenderer").or_else(|| {
            entry.pointer(
                "/playlistPanelVideoWrapperRenderer/primaryRenderer/playlistPanelVideoRenderer",
            )
        });
        let Some(row) = row else { continue };
        if row.get("unplayableText").is_some()
            || row.get("isPlayable").and_then(Value::as_bool) == Some(false)
        {
            continue;
        }
        let Some(id) = row
            .get("videoId")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
        else {
            continue;
        };
        let title = text(&row["title"]);
        if title.is_empty() || !seen.insert(id.to_owned()) {
            continue;
        }
        let mut artists = Vec::new();
        let mut album = String::new();
        if let Some(runs) = row
            .pointer("/longBylineText/runs")
            .and_then(Value::as_array)
        {
            for run in runs {
                let Some(name) = run.get("text").and_then(Value::as_str) else {
                    continue;
                };
                let Some(browse_id) = run
                    .pointer("/navigationEndpoint/browseEndpoint/browseId")
                    .and_then(Value::as_str)
                else {
                    continue;
                };
                if browse_id.starts_with("UC") {
                    artists.push(name);
                } else if browse_id.starts_with("MPRE") {
                    album = name.to_owned();
                }
            }
        }
        tracks.push(Track {
            video_id: id.to_owned(),
            title,
            artist: if artists.is_empty() {
                text(&row["shortBylineText"])
            } else {
                artists.join(", ")
            },
            album,
            duration_secs: duration_seconds(&text(&row["lengthText"])),
            cover_url: thumbnail(&row["thumbnail"]),
        });
        if tracks.len() == 48 {
            break;
        }
    }
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playlist_management_validates_titles_permissions_privacy_and_server_acknowledgements() {
        assert!(check_playlist_cancel(&std::sync::atomic::AtomicBool::new(true)).is_err());
        assert!(playlist_title("   ").is_err());
        assert!(playlist_title("<script>").is_err());
        assert!(playlist_title(&"a".repeat(151)).is_err());
        assert_eq!(playlist_title("  Été 2026  ").unwrap(), "Été 2026");
        let body = update_playlist_body("PL_test", Some("Nouveau"), None).unwrap();
        assert_eq!(body["actions"].as_array().unwrap().len(), 1);
        assert_eq!(body["actions"][0]["action"], "ACTION_SET_PLAYLIST_NAME");
        let body = update_playlist_body("PL_test", None, Some(PlaylistPrivacy::Public)).unwrap();
        assert_eq!(body["actions"][0]["playlistPrivacy"], "PUBLIC");
        assert!(update_playlist_body("LM", Some("Titre"), None).is_err());
        assert!(update_playlist_body("PL_test", None, None).is_err());
        assert!(confirm_playlist_mutation(&json!({})).is_err());
        assert!(confirm_playlist_mutation(&json!({"status":"STATUS_FAILED"})).is_err());
        assert!(confirm_playlist_mutation(&json!({"status":"STATUS_SUCCEEDED"})).is_ok());
        assert_eq!(
            confirmed_created_playlist(&json!({"playlistId":"PL_created"})).unwrap(),
            "PL_created"
        );
        assert!(confirmed_created_playlist(&json!({"playlistId":"LM"})).is_err());
        let owned = json!({"header":{"musicEditablePlaylistDetailHeaderRenderer":{"playlistId":"PL_test","header":{"musicResponsiveHeaderRenderer":{"title":{"runs":[{"text":"Titre"}]}}},"editHeader":{"musicPlaylistEditHeaderRenderer":{"privacy":"UNLISTED"}}}}});
        let info = parse_playlist_details(&owned, "PL_test").unwrap();
        assert!(info.editable);
        assert_eq!(info.title, "Titre");
        assert_eq!(info.privacy, PlaylistPrivacy::Unlisted);
        assert!(parse_playlist_details(&owned, "PL_other").is_err());
        assert!(
            !parse_playlist_details(
                &json!({"header":{"musicResponsiveHeaderRenderer":{}}}),
                "PL_test"
            )
            .unwrap()
            .editable
        );
    }
    #[test]
    #[ignore = "Session DPAPI et réseau ; lecture seule des droits et de la confidentialité, sans noms ni identifiants affichés."]
    fn playlist_management_metadata_live_read_only() {
        let session = crate::session::load().unwrap().unwrap();
        let provider = YouTubeMusic::new(&session).unwrap();
        let playlists = provider.library_playlists(&session, None).unwrap();
        for playlist in playlists.items.iter().take(3) {
            let details = provider.playlist_details(&session, &playlist.id).unwrap();
            eprintln!(
                "Playlist: editable={}, privacy={:?}, title_present={}",
                details.editable,
                details.privacy,
                !details.title.is_empty()
            );
            if details.editable {
                assert!(!details.title.is_empty());
            }
        }
    }
    #[test]
    #[ignore = "Session DPAPI et réseau ; crée puis supprime uniquement sa propre playlist privée vide de test."]
    fn creates_renames_and_deletes_its_own_empty_private_playlist() {
        let session = crate::session::load().unwrap().unwrap();
        let provider = YouTubeMusic::new(&session).unwrap();
        let title = format!("Tonal test technique {}", fastrand::u64(..));
        let created = provider
            .create_playlist(&session, &title, PlaylistPrivacy::Private)
            .unwrap();
        struct Cleanup<'a> {
            provider: &'a YouTubeMusic,
            session: &'a Session,
            id: String,
            done: bool,
        }
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                if !self.done {
                    let _ = self.provider.delete_playlist(self.session, &self.id);
                }
            }
        }
        let mut cleanup = Cleanup {
            provider: &provider,
            session: &session,
            id: created.id.clone(),
            done: false,
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        let details = loop {
            let details = provider.playlist_details(&session, &created.id).unwrap();
            if details.editable || std::time::Instant::now() >= deadline {
                break details;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        assert!(details.editable);
        assert_eq!(details.privacy, PlaylistPrivacy::Private);
        assert_eq!(details.title, title);
        let renamed = format!("{title} modifiée");
        provider
            .update_playlist(&session, &created.id, Some(&renamed), None)
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        let confirmed = loop {
            let details = provider.playlist_details(&session, &created.id).unwrap();
            if details.title == renamed || std::time::Instant::now() >= deadline {
                break details;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(confirmed.title, renamed);
        assert_eq!(confirmed.privacy, PlaylistPrivacy::Private);
        assert!(provider
            .playlist_tracks(&session, &created.id, None)
            .unwrap()
            .items
            .is_empty());
        provider.delete_playlist(&session, &created.id).unwrap();
        cleanup.done = true;
        let library = provider.library_playlists(&session, None).unwrap();
        assert!(!library.items.iter().any(|p| p.id == created.id));
        eprintln!("Cycle réel réussi : création, renommage, confidentialité privée conservée, suppression de la seule playlist de test vide.");
    }
    #[test]
    fn playlist_add_requires_a_personal_target_and_an_explicit_matching_success() {
        let body = add_playlist_body("PLmine", "track1").unwrap();
        assert_eq!(body["actions"].as_array().unwrap().len(), 1);
        assert_eq!(body["actions"][0]["addedVideoId"], "track1");
        assert!(body["actions"][0].get("dedupeOption").is_none());
        assert!(add_playlist_body("LM", "track1").is_err());
        assert!(add_playlist_body("PLmine", "https://invalid").is_err());
        let mut response = json!({"status":"STATUS_SUCCEEDED","playlistEditResults":[{
            "playlistEditVideoAddedResultData":{"videoId":"track1","setVideoId":"entry1"}}]});
        assert!(confirm_playlist_add(&response, "track1").is_ok());
        assert!(confirm_playlist_add(&response, "other").is_err());
        response["status"] = json!("STATUS_FAILED");
        assert!(confirm_playlist_add(&response, "track1").is_err());
        assert!(confirm_playlist_add(&json!({}), "track1").is_err());
    }

    #[test]
    fn playlist_choices_use_server_membership_and_only_offer_editable_targets() {
        let option = |id: &str, membership: &str, editable: bool| {
            json!({"playlistAddToOptionRenderer":{
                "playlistId":id,"title":{"simpleText":"Playlist"},"containsSelectedVideos":membership,
                "addToPlaylistServiceEndpoint":{"playlistEditEndpoint":{"playlistId":id,
                    "actions":if editable { json!([{"action":"ACTION_ADD_VIDEO","addedVideoId":"track1"}]) } else { json!([]) } }}
            }})
        };
        let response = json!({"contents":[{"addToPlaylistRenderer":{"playlists":[
            option("PLmine","NONE",true), option("PLpresent","ALL",false),
            option("PLreadonly","NONE",false), option("LM","ALL",true),
            option("PLunknown","UNKNOWN",true), option("PLmine","NONE",true)
        ]}}]});
        let choices = parse_playlist_choices(&response).unwrap();
        assert_eq!(choices.len(), 3);
        assert_eq!(choices[0].contains, Some(false));
        assert_eq!(choices[1].contains, Some(true));
        assert_eq!(choices[2].contains, None);
        assert!(parse_playlist_choices(&json!({"contents":{}})).is_err());
    }

    #[test]
    #[ignore = "Session DPAPI et réseau ; consulte les playlists et la présence d’un titre, aucune écriture."]
    fn playlist_choices_live_read_only() {
        let session = crate::session::load()
            .unwrap()
            .expect("Session enregistrée");
        let provider = YouTubeMusic::new(&session).unwrap();
        let playlists = provider.library_playlists(&session, None).unwrap();
        let seed = provider
            .playlist_tracks(&session, &playlists.items[0].id, None)
            .unwrap()
            .items
            .remove(0);
        let choices = provider.playlist_choices(&session, &seed.video_id).unwrap();
        assert!(!choices.is_empty());
        assert!(choices
            .iter()
            .any(|choice| choice.id == playlists.items[0].id));
        eprintln!(
            "Playlists : {} destinations modifiables ; aucune écriture.",
            choices.len()
        );
    }

    #[test]
    #[ignore = "Session DPAPI et réseau ; contrôle la structure des playlists, sans écriture."]
    fn playlist_content_scopes_live_read_only() {
        let session = crate::session::load().unwrap().unwrap();
        let provider = YouTubeMusic::new(&session).unwrap();
        let library = provider.library_playlists(&session, None).unwrap();
        for (index, playlist) in library.items.iter().take(6).enumerate() {
            let response = provider
                .request(
                    &session,
                    "browse",
                    json!({"browseId":format!("VL{}", playlist.id)}),
                    None,
                )
                .unwrap();
            let page = parse_tracks(&response).unwrap();
            let shelf = find_key(&response, "musicPlaylistShelfRenderer").unwrap();
            let expected = shelf["contents"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|entry| entry.get("musicResponsiveListItemRenderer").is_some())
                .count();
            assert_eq!(page.items.len(), expected);
            if playlist.track_count == Some(0) {
                assert!(page.items.is_empty());
                assert!(page.continuation.is_none());
            }
            eprintln!(
                "Playlist {index}: {} vrais titres sur la première page",
                page.items.len()
            );
            if let Some(token) = &page.continuation {
                let next = provider
                    .playlist_tracks(&session, &playlist.id, Some(token))
                    .unwrap();
                assert!(!next.items.is_empty());
                assert_ne!(next.continuation.as_ref(), Some(token));
                eprintln!("  Pagination réelle : {} titres", next.items.len());
            }
        }
    }

    #[test]
    fn suggestions_keep_primary_playable_recordings_and_exclude_seed_and_duplicates() {
        let row = json!({"playlistPanelVideoRenderer": {
            "videoId":"discovery_1", "title":{"simpleText":"Découverte"},
            "lengthText":{"runs":[{"text":"3:26"}]},
            "longBylineText":{"runs":[
                {"text":"Artiste", "navigationEndpoint":{"browseEndpoint":{"browseId":"UCartist"}}},
                {"text":" · "},
                {"text":"Album", "navigationEndpoint":{"browseEndpoint":{"browseId":"MPREalbum"}}}
            ]},
            "thumbnail":{"thumbnails":[{"url":"https://i.ytimg.com/vi/discovery_1/hqdefault.jpg","width":480}]}
        }});
        let mut seed = row.clone();
        seed["playlistPanelVideoRenderer"]["videoId"] = json!("seed");
        let mut unavailable = row.clone();
        unavailable["playlistPanelVideoRenderer"]["videoId"] = json!("unavailable");
        unavailable["playlistPanelVideoRenderer"]["unplayableText"] =
            json!({"simpleText":"Indisponible"});
        let response = json!({"contents":{"playlistPanelRenderer":{"contents":[
            seed, unavailable,
            {"playlistPanelVideoWrapperRenderer":{"primaryRenderer":row.clone(),"counterpart":[
                {"counterpartRenderer":{"playlistPanelVideoRenderer":{"videoId":"counterpart","title":{"simpleText":"Clip"}}}}
            ]}}, row,
            {"playlistPanelVideoRenderer":{"videoId":"https://invalid","title":{"simpleText":"Invalide"}}}
        ]}}});
        let tracks = parse_suggestions(&response, "seed").unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].video_id, "discovery_1");
        assert_eq!(tracks[0].artist, "Artiste");
        assert_eq!(tracks[0].album, "Album");
        assert_eq!(tracks[0].duration_secs, Some(206));
        assert!(tracks[0].cover_url.is_some());
        assert!(parse_suggestions(&json!({"contents":{}}), "seed").is_err());
        assert!(parse_suggestions(
            &json!({"contents":{"playlistPanelRenderer":{"contents":[]}}}),
            "seed"
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    #[ignore = "Session DPAPI et réseau ; suggestions réelles, métadonnées uniquement, sans lecture audio."]
    fn suggestions_live() {
        let session = crate::session::load()
            .unwrap()
            .expect("Session enregistrée");
        let provider = YouTubeMusic::new(&session).unwrap();
        let playlists = provider.library_playlists(&session, None).unwrap();
        let seed = provider
            .playlist_tracks(&session, &playlists.items[0].id, None)
            .unwrap()
            .items
            .remove(0);
        let tracks = provider.suggestions(&session, &seed.video_id).unwrap();
        assert!(!tracks.is_empty());
        assert!(tracks.len() <= 48);
        let mut ids = HashSet::from([seed.video_id]);
        for track in &tracks {
            assert!(ids.insert(track.video_id.clone()));
            assert!(!track.title.is_empty());
            assert!(!track.artist.is_empty());
            assert!(track.cover_url.is_some());
        }
        eprintln!(
            "Suggestions : {} titres distincts, métadonnées vérifiées, sans audio.",
            tracks.len()
        );
    }

    #[test]
    fn subscription_status_uses_the_explicit_boolean_instead_of_promotional_text() {
        assert_eq!(
            config_bool(
                r#"ytcfg.set({"IS_SUBSCRIBER":true,"OTHER":1});"#,
                "IS_SUBSCRIBER"
            ),
            Some(true)
        );
        assert_eq!(
            config_bool(r#"ytcfg.set({"IS_SUBSCRIBER":false});"#, "IS_SUBSCRIBER"),
            Some(false)
        );
        assert_eq!(
            config_bool(r#"{"PROMOTION":"MUSIC_PREMIUM"}"#, "IS_SUBSCRIBER"),
            None
        );
        assert_eq!(
            config_bool(r#"{"IS_SUBSCRIBER":"true"}"#, "IS_SUBSCRIBER"),
            None
        );
    }

    #[test]
    #[ignore = "Vérifie uniquement la présence du statut d'abonnement avec la session enregistrée, sans imprimer le compte."]
    fn subscription_status_live() {
        let session = crate::session::load().unwrap().unwrap();
        let provider = YouTubeMusic::new(&session).unwrap();
        let account = provider.validate_session(&session).unwrap();
        assert!(account.premium.is_some());
        eprintln!(
            "YouTube Music subscription premium={}",
            account.premium.unwrap()
        );
    }
    #[test]
    fn playlist_total_comes_from_metadata_instead_of_loaded_page_length() {
        assert_eq!(playlist_track_count("Theo • 611 titres"), Some(611));
        assert_eq!(playlist_track_count("1\u{202f}234 titres"), Some(1234));
        assert_eq!(playlist_track_count("Owner · 1,234 songs"), Some(1234));
        assert_eq!(playlist_track_count("1 titre"), Some(1));
        assert_eq!(playlist_track_count("0 titres"), Some(0));
        assert_eq!(playlist_track_count("2026 • Album"), None);
        assert_eq!(playlist_track_count("1,2 k titres"), None);
        assert_eq!(playlist_track_count("Owner"), None);
        let value = json!({"contents":{"gridRenderer":{"items":[
            {"musicTwoRowItemRenderer":{"title":{"simpleText":"Playlist"},
                "subtitle":{"runs":[{"text":"Owner"},{"text":" • "},{"text":"611 titres"}]},
                "navigationEndpoint":{"browseEndpoint":{"browseId":"VLPL_total"}}}}
        ]}}});
        assert_eq!(
            parse_playlists(&value).unwrap().items[0].track_count,
            Some(611)
        );
    }
    #[test]
    fn parses_artwork_account_and_hides_automatic_playlists() {
        let value = json!({"contents":{"gridRenderer":{"items":[
            {"musicTwoRowItemRenderer":{"title":{"simpleText":"Ma playlist"},
                "navigationEndpoint":{"browseEndpoint":{"browseId":"VLPL_real"}},
                "thumbnailRenderer":{"musicThumbnailRenderer":{"thumbnail":{"thumbnails":[
                    {"url":"https://lh3.googleusercontent.com/small","width":120},
                    {"url":"https://lh3.googleusercontent.com/large","width":544}
                ]}}}}},
            {"musicTwoRowItemRenderer":{"title":{"simpleText":"J'aime"},"navigationEndpoint":{"browseEndpoint":{"browseId":"VLLM"}}}},
            {"musicTwoRowItemRenderer":{"title":{"simpleText":"Épisodes"},"navigationEndpoint":{"browseEndpoint":{"browseId":"VLSE"}}}},
            {"musicTwoRowItemRenderer":{"title":{"simpleText":"Mix"},"navigationEndpoint":{"browseEndpoint":{"browseId":"VLRDmix"}}}}
        ]}}});
        let page = parse_playlists(&value).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].cover_url.as_deref(),
            Some("https://lh3.googleusercontent.com/large")
        );
        let account = parse_account(&json!({"activeAccountHeaderRenderer":{
            "accountName":{"simpleText":"Camille Exemple"},
            "accountPhoto":{"thumbnails":[{"url":"https://yt3.ggpht.com/avatar","width":88}]}
        }}))
        .unwrap();
        assert_eq!(account.given_name, "Camille");
        assert_eq!(
            account.avatar_url.as_deref(),
            Some("https://yt3.ggpht.com/avatar")
        );
        assert!(parse_account(&json!({"loggedOut":true})).is_err());
    }

    #[test]
    fn parses_track_cover_album_and_duration_without_inventing_missing_values() {
        let value = json!({"contents":{"musicPlaylistShelfRenderer":{"contents":[
            {"musicResponsiveListItemRenderer":{
                "playlistItemData":{"videoId":"test_video1"},
                "flexColumns":[
                    {"musicResponsiveListItemFlexColumnRenderer":{"text":{"simpleText":"Titre"}}},
                    {"musicResponsiveListItemFlexColumnRenderer":{"text":{"simpleText":"Artiste"}}},
                    {"musicResponsiveListItemFlexColumnRenderer":{"text":{"simpleText":"Album"}}}
                ],
                "fixedColumns":[{"musicResponsiveListItemFixedColumnRenderer":{"text":{"simpleText":"3:26"}}}],
                "thumbnail":{"musicThumbnailRenderer":{"thumbnail":{"thumbnails":[{"url":"https://i.ytimg.com/cover.jpg","width":120}]}}}
            }}
        ]}}});
        let track = parse_tracks(&value).unwrap().items.remove(0);
        assert_eq!(track.album, "Album");
        assert_eq!(track.duration_secs, Some(206));
        assert_eq!(
            track.cover_url.as_deref(),
            Some("https://i.ytimg.com/cover.jpg")
        );
        assert_eq!(duration_seconds("1:02:03"), Some(3723));
        assert_eq!(duration_seconds("En direct"), None);
        assert_eq!(duration_seconds("3:99"), None);
        assert!(thumbnail(&json!({"thumbnails":[{"url":"https://evil.test/cover"}]})).is_none());
    }
    #[test]
    fn library_parses_titles_ids_continuations_and_ignores_other_cards() {
        let value = json!({"contents": {"gridRenderer": {"items": [
            {"musicTwoRowItemRenderer": {"title": {"runs": [{"text": "Ma playlist"}]}, "navigationEndpoint": {"browseEndpoint": {"browseId": "VLPL_test"}}}},
            {"musicTwoRowItemRenderer": {"title": {"simpleText": "Un album"}, "navigationEndpoint": {"browseEndpoint": {"browseId": "MPRE_album"}}}}
        ], "continuations": [{"nextContinuationData": {"continuation": "page2"}}]}}});
        let page = parse_playlists(&value).unwrap();
        assert_eq!(
            page.items,
            vec![Playlist {
                id: "PL_test".into(),
                title: "Ma playlist".into(),
                subtitle: "".into(),
                ..Default::default()
            }]
        );
        assert_eq!(page.continuation.as_deref(), Some("page2"));
        assert!(parse_playlists(&json!({"contents": {"unknownRenderer": {}}})).is_err());
        assert!(
            parse_playlists(&json!({"contents": {"gridRenderer": {"items": []}}}))
                .unwrap()
                .items
                .is_empty()
        );
    }
    #[test]
    fn continuation_page_and_tracks() {
        let value = json!({"continuationContents": {"musicPlaylistShelfContinuation": {"contents": [
            {"musicResponsiveListItemRenderer": {"playlistItemData": {"videoId": "test_video1"}, "flexColumns": [
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Titre"}]}}},
                {"musicResponsiveListItemFlexColumnRenderer": {"text": {"simpleText": "Artiste"}}}
            ]}}, {"musicResponsiveListItemRenderer": {"flexColumns": []}}
        ]}}});
        let page = parse_tracks(&value).unwrap();
        assert_eq!(
            page.items,
            vec![Track {
                video_id: "test_video1".into(),
                title: "Titre".into(),
                artist: "Artiste".into(),
                ..Default::default()
            }]
        );
        assert!(page.continuation.is_none());
    }
    #[test]
    fn playlist_shelf_excludes_recommendations_and_keeps_real_duplicate_entries() {
        let row = |id: &str| {
            json!({"musicResponsiveListItemRenderer": {
                "playlistItemData":{"videoId":id},
                "flexColumns":[{"musicResponsiveListItemFlexColumnRenderer":{"text":{"simpleText":id}}}]
            }})
        };
        let recommendation = json!({"musicShelfRenderer":{
            "contents":[row("unknown_song"),row("unknown_song")],
            "continuations":[{"nextContinuationData":{"continuation":"unrelated"}}]
        }});
        let mut value = json!({"contents":{"sectionListRenderer":{"contents":[
            recommendation,
            {"musicPlaylistShelfRenderer":{}}
        ]}}});
        let empty = parse_tracks(&value).unwrap();
        assert!(empty.items.is_empty());
        assert!(empty.continuation.is_none());
        value["contents"]["sectionListRenderer"]["contents"][1]["musicPlaylistShelfRenderer"] = json!({
            "contents":[row("actual_song"),row("actual_song"),
                {"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"actual_next"}}}}]
        });
        let page = parse_tracks(&value).unwrap();
        assert_eq!(
            page.items
                .iter()
                .map(|t| t.video_id.as_str())
                .collect::<Vec<_>>(),
            vec!["actual_song", "actual_song"]
        );
        assert_eq!(page.continuation.as_deref(), Some("actual_next"));
    }
    #[test]
    fn playlist_append_continuation_ignores_other_targets() {
        let value = json!({"onResponseReceivedActions":[
            {"appendContinuationItemsAction":{"targetId":"recommendations", "continuationItems":[
                {"musicResponsiveListItemRenderer":{"playlistItemData":{"videoId":"unknown_song"},"flexColumns":[{"musicResponsiveListItemFlexColumnRenderer":{"text":{"simpleText":"Suggestion"}}}]}}
            ]}},
            {"appendContinuationItemsAction":{"targetId":"playlist-shelf", "continuationItems":[
                {"musicResponsiveListItemRenderer":{"playlistItemData":{"videoId":"actual_song"},"flexColumns":[{"musicResponsiveListItemFlexColumnRenderer":{"text":{"simpleText":"Titre"}}}]}},
                {"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"next_page"}}}}
            ]}}
        ]});
        let page = parse_tracks(&value).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].video_id, "actual_song");
        assert_eq!(page.continuation.as_deref(), Some("next_page"));
        let opaque = json!({"onResponseReceivedActions":[
            {"appendContinuationItemsAction":{"targetId":"opaque_target_123", "continuationItems":[
                {"continuationItemRenderer":{"continuationEndpoint":{"continuationCommand":{"token":"opaque_next"}}}}
            ]}}
        ]});
        assert_eq!(
            parse_tracks(&opaque).unwrap().continuation.as_deref(),
            Some("opaque_next")
        );
        assert!(
            parse_tracks(&json!({"contents":{"sectionListRenderer":{"contents":[
                {"musicShelfRenderer":{"contents":[]}}
            ]}}}))
            .is_err()
        );
    }
    #[test]
    fn client_version_is_read_from_live_config() {
        assert_eq!(
            config_string(
                r#"ytcfg.set({"INNERTUBE_CLIENT_VERSION": "1.test", "OTHER": 1});"#,
                "INNERTUBE_CLIENT_VERSION"
            )
            .as_deref(),
            Some("1.test")
        );
    }
    #[test]
    fn authentication_hash_is_bound_to_origin_and_timestamp() {
        assert_eq!(
            authorization("test", 1),
            "SAPISIDHASH 1_3e818942372dd3b77f9cd5debf20a5b353a9ba02"
        );
    }

    #[test]
    #[ignore = "Nécessite Internet ; vérifie le rejet d'une session factice, sans compte réel."]
    fn invalid_session_is_rejected_live() {
        let session = Session::from_cookies([
            ("SAPISID".into(), "tonal-invalid-test".into()),
            ("SID".into(), "tonal-invalid-test".into()),
        ])
        .unwrap();
        // Google may reject the session on the initial HTML request or the account API.
        let result =
            YouTubeMusic::new(&session).and_then(|provider| provider.validate_session(&session));
        assert!(matches!(result, Err(ProviderError::SessionExpired)));
    }
}
