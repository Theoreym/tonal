//! Progressive compressed audio: bounded HTTP blocks, native decoding and nonblocking PCM output.
use crate::{
    session::Session,
    youtube::{self, AudioQuality, MediaInfo},
};
use rodio::Source;
use std::{
    collections::HashMap,
    io::{self, Read, Seek, SeekFrom},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{Decoder, DecoderOptions, CODEC_TYPE_OPUS},
    errors::Error as DecodeError,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::{MediaSource, MediaSourceStream},
    meta::MetadataOptions,
    probe::Hint,
    units::{Time, TimeBase},
};
use zeroize::Zeroizing;

const BLOCK: u64 = 256 * 1024;
const CACHE_BLOCKS: usize = 32; // 8 MiB compressed audio, independent of track length.
const PCM_QUEUE: usize = 24;
type BlockMap = HashMap<u64, (Arc<[u8]>, u64)>;

struct RemoteData {
    http: crate::http_audio::AudioHttp,
    url: Mutex<Zeroizing<String>>,
    origin: String,
    session: Option<Session>,
    bytes: u64,
    user_agent: String,
    format: String,
    cancelled: Arc<AtomicBool>,
    cache: Mutex<(BlockMap, u64)>,
}
pub struct NativeStream {
    remote: Arc<RemoteData>,
    pub quality: AudioQuality,
    pub duration: Option<Duration>,
    pub loudness_db: Option<f32>,
}
impl NativeStream {
    pub fn open(
        info: MediaInfo,
        session: Option<Session>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        if !youtube::media_url_allowed(info.url.as_str()) {
            return Err("Adresse de flux audio refusée.".into());
        }
        let http = crate::http_audio::AudioHttp::new();
        let bytes = if info.bytes > 0 {
            info.bytes
        } else {
            http.length(info.url.as_str(), &info.user_agent, &cancelled)
                .unwrap_or(0)
        };
        if bytes == 0 || bytes > 2 * 1024 * 1024 * 1024 {
            return Err("La taille du flux audio est indisponible ou non prise en charge.".into());
        }
        Ok(Self {
            loudness_db: crate::direct_audio::cached_loudness(&info.source_url).flatten(),
            remote: Arc::new(RemoteData {
                http,
                url: Mutex::new(info.url),
                origin: info.source_url,
                session,
                bytes,
                user_agent: info.user_agent,
                format: info.quality.format_id.clone(),
                cancelled,
                cache: Mutex::new((HashMap::new(), 0)),
            }),
            quality: info.quality,
            duration: info.duration,
        })
    }
    pub fn prebuffer(&self) -> Result<(), String> {
        for index in 0..2.min(self.remote.bytes.div_ceil(BLOCK)) {
            self.remote
                .block(index)
                .map_err(|_| "Le préchargement audio a échoué.")?;
        }
        Ok(())
    }
    pub fn cancel(&self) {
        self.remote.cancelled.store(true, Ordering::Relaxed);
        self.remote.http.set_paused(true);
    }
}
impl Drop for NativeStream {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl RemoteData {
    fn block(&self, index: u64) -> io::Result<Arc<[u8]>> {
        let abort = self.http.token();
        if self.cancelled.load(Ordering::Relaxed) || self.http.paused() {
            return Err(io::Error::other("stream cancelled"));
        }
        {
            let mut cache = self.cache.lock().unwrap();
            cache.1 += 1;
            let stamp = cache.1;
            if let Some((bytes, used)) = cache.0.get_mut(&index) {
                *used = stamp;
                return Ok(bytes.clone());
            }
        }
        let start = index * BLOCK;
        let end = (start + BLOCK).min(self.bytes).saturating_sub(1);
        let expected = (end - start + 1) as usize;
        let mut attempts = 0;
        // Limit renewal per read, rather than once for the entire lifetime of a track.
        let mut renewed = false;
        let data = loop {
            if abort.load(Ordering::SeqCst)
                || self.http.paused()
                || self.cancelled.load(Ordering::Relaxed)
            {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "stream interrupted",
                ));
            }
            let mut url = reqwest::Url::parse(self.url.lock().unwrap().as_str())
                .map_err(|_| io::Error::other("invalid stream"))?;
            if !renewed && expires_soon(&url, SystemTime::now()) {
                renewed = true;
                self.renew(&abort)?;
                continue;
            }
            let pairs: Vec<_> = url
                .query_pairs()
                .filter(|(key, _)| key != "range")
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            url.query_pairs_mut()
                .clear()
                .extend_pairs(pairs)
                .append_pair("range", &format!("{start}-{end}"));
            let response =
                self.http
                    .range(url, &self.user_agent, expected, &self.cancelled, &abort);
            if let Ok((status, _)) = &response {
                if matches!(
                    *status,
                    reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::GONE
                ) && !renewed
                {
                    renewed = true;
                    self.renew(&abort)?;
                    continue;
                }
            }
            let result = response.ok().and_then(|(status, bytes)| {
                (status.is_success() && bytes.len() == expected).then_some(bytes)
            });
            if let Some(bytes) = result {
                break bytes;
            }
            if self.cancelled.load(Ordering::Relaxed) || attempts >= 1 {
                return Err(io::Error::other("audio range unavailable"));
            }
            attempts += 1;
        };
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::other("stream cancelled"));
        }
        let data: Arc<[u8]> = data.into();
        let mut cache = self.cache.lock().unwrap();
        if cache.0.len() >= CACHE_BLOCKS {
            let oldest = cache
                .0
                .iter()
                .filter(|(index, _)| **index >= 2)
                .min_by_key(|(_, (_, used))| *used)
                .map(|(index, _)| *index);
            if let Some(oldest) = oldest {
                cache.0.remove(&oldest);
            }
        }
        let stamp = cache.1;
        cache.0.insert(index, (data.clone(), stamp));
        Ok(data)
    }

    fn renew(&self, abort: &AtomicBool) -> io::Result<()> {
        let renewed = youtube::resolve_audio_fresh(
            &self.origin,
            self.session.as_ref(),
            self.cancelled.clone(),
            abort,
        )
        .map_err(|_| io::Error::other("stream renewal failed"))?;
        if renewed.quality.format_id != self.format
            || (renewed.bytes != 0 && renewed.bytes != self.bytes)
        {
            return Err(io::Error::other("stream format changed"));
        }
        *self.url.lock().unwrap() = renewed.url;
        Ok(())
    }
}

fn expires_soon(url: &reqwest::Url, now: SystemTime) -> bool {
    let Ok(now) = now.duration_since(UNIX_EPOCH) else {
        return false;
    };
    url.query_pairs()
        .find(|(key, _)| key == "expire")
        .and_then(|(_, value)| value.parse::<u64>().ok())
        .is_some_and(|expiry| expiry <= now.as_secs().saturating_add(60))
}

struct NetworkReader {
    remote: Arc<RemoteData>,
    at: u64,
    seekable: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
    generation: u64,
}
impl Read for NetworkReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.at >= self.remote.bytes {
            return Ok(0);
        }
        if self.epoch.load(Ordering::Relaxed) != self.generation {
            return Err(io::Error::other("seek superseded"));
        }
        let block = self.remote.block(self.at / BLOCK)?;
        let offset = (self.at % BLOCK) as usize;
        let count = out.len().min(block.len() - offset);
        out[..count].copy_from_slice(&block[offset..offset + count]);
        self.at += count as u64;
        Ok(count)
    }
}
impl Seek for NetworkReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let target = match position {
            SeekFrom::Start(at) => i128::from(at),
            SeekFrom::Current(delta) => i128::from(self.at) + i128::from(delta),
            SeekFrom::End(delta) => i128::from(self.remote.bytes) + i128::from(delta),
        };
        if target < 0 || target > i128::from(self.remote.bytes) {
            return Err(io::Error::other("seek outside stream"));
        }
        self.at = target as u64;
        Ok(self.at)
    }
}
impl MediaSource for NetworkReader {
    fn is_seekable(&self) -> bool {
        self.seekable.load(Ordering::Relaxed)
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.remote.bytes)
    }
}

enum Codec {
    Standard(Box<dyn Decoder>),
    Opus {
        decoder: Box<opus::Decoder>,
        channels: usize,
        pre_skip: usize,
        gain: f32,
    },
}
struct NativeDecoder {
    format: Box<dyn FormatReader>,
    codec: Codec,
    track: u32,
    base: TimeBase,
    target_ms: u64,
}
struct Pcm {
    samples: Vec<f32>,
    channels: u16,
    rate: u32,
    start_ms: u64,
    epoch: u64,
    end: bool,
}
impl NativeDecoder {
    fn open(
        stream: &NativeStream,
        epoch: Arc<AtomicU64>,
        generation: u64,
        position: Duration,
    ) -> Result<Self, String> {
        let seekable = Arc::new(AtomicBool::new(stream.quality.codec == "opus"));
        let input = NetworkReader {
            remote: stream.remote.clone(),
            at: 0,
            seekable: seekable.clone(),
            epoch,
            generation,
        };
        let mut hint = Hint::new();
        hint.with_extension(if stream.quality.codec == "opus" {
            "webm"
        } else {
            "m4a"
        });
        // Avoid indexing the complete fragmented MP4 at startup.
        let mut format = symphonia::default::get_probe()
            .format(
                &hint,
                MediaSourceStream::new(Box::new(input), Default::default()),
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|_| "Le conteneur audio n'est pas reconnu.")?
            .format;
        seekable.store(true, Ordering::Relaxed);
        let track = format
            .default_track()
            .ok_or("Le flux ne contient pas de piste audio.")?;
        let id = track.id;
        let base = track.codec_params.time_base.unwrap_or(TimeBase::new(
            1,
            track.codec_params.sample_rate.unwrap_or(48000),
        ));
        let codec = if track.codec_params.codec == CODEC_TYPE_OPUS {
            let header = track
                .codec_params
                .extra_data
                .as_deref()
                .filter(|data| data.len() >= 19 && data.starts_with(b"OpusHead"))
                .ok_or("En-tête Opus invalide.")?;
            let channels = usize::from(header[9]);
            if !(1..=2).contains(&channels) || header[18] != 0 {
                return Err("Disposition Opus non prise en charge.".into());
            }
            Codec::Opus {
                decoder: Box::new(
                    opus::Decoder::new(
                        48000,
                        if channels == 1 {
                            opus::Channels::Mono
                        } else {
                            opus::Channels::Stereo
                        },
                    )
                    .map_err(|_| "Décodeur Opus indisponible.")?,
                ),
                channels,
                pre_skip: usize::from(u16::from_le_bytes([header[10], header[11]])),
                gain: 10_f32.powf(f32::from(i16::from_le_bytes([header[16], header[17]])) / 5120.0),
            }
        } else {
            Codec::Standard(
                symphonia::default::get_codecs()
                    .make(&track.codec_params, &DecoderOptions::default())
                    .map_err(|_| "Codec audio non pris en charge.")?,
            )
        };
        if !position.is_zero() {
            let preroll = position.saturating_sub(Duration::from_millis(80));
            format
                .seek(
                    SeekMode::Accurate,
                    SeekTo::Time {
                        time: Time::new(preroll.as_secs(), preroll.subsec_nanos() as f64 / 1e9),
                        track_id: Some(id),
                    },
                )
                .map_err(|_| "Impossible de déplacer la lecture dans ce flux.")?;
        }
        Ok(Self {
            format,
            codec,
            track: id,
            base,
            target_ms: position.as_millis() as u64,
        })
    }
    fn next(&mut self, epoch: u64) -> Result<Option<Pcm>, String> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(DecodeError::IoError(error))
                    if error.kind() == io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None)
                }
                Err(_) => return Err("Le téléchargement du flux a été interrompu.".into()),
            };
            if packet.track_id() != self.track {
                continue;
            }
            let timestamp = self.base.calc_time(packet.ts());
            let mut start_ms = timestamp.seconds * 1000 + (timestamp.frac * 1000.0) as u64;
            let (mut samples, channels, rate) = match &mut self.codec {
                Codec::Standard(decoder) => {
                    let decoded = decoder
                        .decode(&packet)
                        .map_err(|_| "Décodage AAC interrompu.")?;
                    let spec = *decoded.spec();
                    let mut output = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                    output.copy_interleaved_ref(decoded);
                    (
                        output.samples().to_vec(),
                        spec.channels.count() as u16,
                        spec.rate,
                    )
                }
                Codec::Opus {
                    decoder,
                    channels,
                    pre_skip,
                    gain,
                } => {
                    let mut samples = vec![0_f32; 5760 * *channels];
                    let frames = decoder
                        .decode_float(packet.buf(), &mut samples, false)
                        .map_err(|_| "Décodage Opus interrompu.")?;
                    samples.truncate(frames * *channels);
                    let skip = if packet.ts() == 0 { *pre_skip } else { 0 }.min(frames);
                    samples.drain(..skip * *channels);
                    for sample in &mut samples {
                        *sample *= *gain;
                    }
                    (samples, *channels as u16, 48000)
                }
            };
            if start_ms < self.target_ms {
                let trim = (((self.target_ms - start_ms) * u64::from(rate) / 1000) as usize
                    * usize::from(channels))
                .min(samples.len());
                samples.drain(..trim);
                start_ms += trim as u64 * 1000 / u64::from(channels) / u64::from(rate);
            }
            if samples.is_empty() {
                continue;
            }
            return Ok(Some(Pcm {
                samples,
                channels,
                rate,
                start_ms,
                epoch,
                end: false,
            }));
        }
    }
}

struct Shared {
    epoch: Arc<AtomicU64>,
    position: AtomicU64,
    ended: AtomicBool,
    buffering: AtomicBool,
    cancelled: AtomicBool,
    paused: AtomicBool,
    error: Mutex<Option<String>>,
}
pub struct NativeControl {
    shared: Arc<Shared>,
    commands: Sender<Duration>,
    stream: Arc<NativeStream>,
}
impl NativeControl {
    pub fn seek(&self, position: Duration) -> Result<(), String> {
        self.shared.epoch.fetch_add(1, Ordering::Relaxed);
        self.stream.remote.http.interrupt();
        self.shared
            .position
            .store(position.as_millis() as u64, Ordering::Relaxed);
        self.shared.ended.store(false, Ordering::Relaxed);
        self.shared.buffering.store(true, Ordering::Relaxed);
        self.commands
            .send(position)
            .map_err(|_| "Le lecteur audio est arrêté.".into())
    }
    pub fn position(&self) -> Duration {
        Duration::from_millis(self.shared.position.load(Ordering::Relaxed))
    }
    pub fn ended(&self) -> bool {
        self.shared.ended.load(Ordering::Relaxed)
    }
    pub fn buffering(&self) -> bool {
        self.shared.buffering.load(Ordering::Relaxed)
    }
    pub fn take_error(&self) -> Option<String> {
        self.shared.error.lock().unwrap().take()
    }
    pub fn set_paused(&self, paused: bool) {
        if paused {
            self.shared.paused.store(true, Ordering::SeqCst);
            self.stream.remote.http.set_paused(true);
        } else {
            self.stream.remote.http.set_paused(false);
            // Reopen from the frozen position and discard stale, predecoded PCM.
            let _ = self.seek(self.position());
            self.shared.paused.store(false, Ordering::SeqCst);
        }
    }
}
impl Drop for NativeControl {
    fn drop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Relaxed);
        self.stream.cancel();
    }
}

pub struct PcmOutput {
    receiver: Receiver<Pcm>,
    shared: Arc<Shared>,
    chunk: Vec<f32>,
    at: usize,
    channels: u16,
    rate: u32,
    epoch: u64,
    exhausted: bool,
}
impl PcmOutput {
    fn refill(&mut self) {
        self.chunk.clear();
        self.at = 0;
        let epoch = self.shared.epoch.load(Ordering::Relaxed);
        if self.epoch != epoch {
            self.exhausted = false;
            self.epoch = epoch;
        }
        while let Ok(block) = self.receiver.try_recv() {
            if block.epoch != self.shared.epoch.load(Ordering::Relaxed) {
                continue;
            }
            if block.end {
                self.exhausted = true;
                self.shared.ended.store(true, Ordering::Relaxed);
                break;
            }
            self.shared
                .position
                .store(block.start_ms, Ordering::Relaxed);
            self.shared.buffering.store(false, Ordering::Relaxed);
            self.channels = block.channels;
            self.rate = block.rate;
            self.epoch = block.epoch;
            self.chunk = block.samples;
            return;
        }
        self.shared
            .buffering
            .store(!self.exhausted, Ordering::Relaxed);
        self.chunk.resize(512 * usize::from(self.channels), 0.0);
    }
}
impl Iterator for PcmOutput {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.shared.cancelled.load(Ordering::Relaxed) {
            return None;
        }
        if self.shared.paused.load(Ordering::SeqCst) {
            return Some(0.0);
        }
        if self.at >= self.chunk.len() || self.epoch != self.shared.epoch.load(Ordering::Relaxed) {
            self.refill();
        }
        let sample = self.chunk[self.at];
        self.at += 1;
        // rodio queries the next span's rate/channels immediately after this span ends.
        // Publish that metadata before returning the last sample of the previous span.
        if self.at == self.chunk.len() {
            self.refill();
        }
        Some(sample)
    }
}
impl Source for PcmOutput {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.chunk.len().saturating_sub(self.at).max(1))
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub fn start(stream: NativeStream, paused: bool) -> (NativeControl, PcmOutput) {
    if paused {
        stream.remote.http.set_paused(true);
    }
    let stream = Arc::new(stream);
    let shared = Arc::new(Shared {
        epoch: Arc::new(AtomicU64::new(0)),
        position: AtomicU64::new(0),
        ended: AtomicBool::new(false),
        buffering: AtomicBool::new(true),
        cancelled: AtomicBool::new(false),
        paused: AtomicBool::new(paused),
        error: Mutex::new(None),
    });
    let (commands, seeks) = mpsc::channel();
    let (sender, receiver) = mpsc::sync_channel(PCM_QUEUE);
    let worker_stream = stream.clone();
    let worker_shared = shared.clone();
    thread::spawn(move || decode_loop(worker_stream, worker_shared, seeks, sender));
    (
        NativeControl {
            shared: shared.clone(),
            commands,
            stream,
        },
        PcmOutput {
            receiver,
            shared,
            chunk: vec![0.0; 1024],
            at: 0,
            channels: 2,
            rate: 48000,
            epoch: 0,
            exhausted: false,
        },
    )
}

fn decode_loop(
    stream: Arc<NativeStream>,
    shared: Arc<Shared>,
    seeks: Receiver<Duration>,
    output: SyncSender<Pcm>,
) {
    let mut epoch = 0;
    let mut decoder = None;
    let mut reopen = Some(Duration::ZERO);
    let mut pending = None;
    loop {
        if shared.cancelled.load(Ordering::Relaxed) {
            return;
        }
        if let Some(target) = seeks.try_iter().last() {
            epoch = shared.epoch.load(Ordering::Relaxed);
            reopen = Some(target);
            pending = None;
            decoder = None;
        }
        if shared.paused.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        if let Some(target) = reopen.take() {
            match NativeDecoder::open(&stream, shared.epoch.clone(), epoch, target) {
                Ok(opened) => decoder = Some(opened),
                Err(_) if shared.epoch.load(Ordering::Relaxed) != epoch => continue,
                Err(_) if shared.paused.load(Ordering::SeqCst) => {
                    decoder = None;
                    continue;
                }
                Err(error) => {
                    *shared.error.lock().unwrap() = Some(error);
                    return;
                }
            }
        }
        if let Some(chunk) = pending.take() {
            match output.try_send(chunk) {
                Ok(()) => {}
                Err(TrySendError::Full(chunk)) => {
                    pending = Some(chunk);
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(TrySendError::Disconnected(_)) => return,
            }
        }
        let Some(active) = decoder.as_mut() else {
            thread::sleep(Duration::from_millis(10));
            continue;
        };
        match active.next(epoch) {
            Ok(Some(chunk)) => pending = Some(chunk),
            Ok(None) => {
                pending = Some(Pcm {
                    samples: Vec::new(),
                    channels: 2,
                    rate: 48000,
                    start_ms: 0,
                    epoch,
                    end: true,
                });
                // Wait for a seek or cancellation after delivering the end marker.
                decoder = None;
            }
            Err(_) if shared.epoch.load(Ordering::Relaxed) != epoch => {}
            Err(_) if shared.paused.load(Ordering::SeqCst) => {
                decoder = None;
                pending = None;
            }
            Err(error) => {
                *shared.error.lock().unwrap() = Some(error);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_url_expiry_has_a_margin_and_handles_missing_or_invalid_dates() {
        let now = UNIX_EPOCH + Duration::from_secs(1000);
        for (query, expected) in [
            ("expire=1000", true),
            ("expire=1060", true),
            ("expire=1061", false),
            ("expire=bad", false),
            ("other=1", false),
        ] {
            let url = reqwest::Url::parse(&format!(
                "https://test.googlevideo.com/videoplayback?{query}"
            ))
            .unwrap();
            assert_eq!(expires_soon(&url, now), expected);
        }
    }

    #[test]
    #[ignore = "Internet et yt-dlp ; simule deux expirations successives du lien d'un même flux, sans sortie audio."]
    fn the_same_stream_can_renew_after_multiple_expirations() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let info = youtube::resolve_audio(
            "https://music.youtube.com/watch?v=jNQXAC9IVRw",
            None,
            cancelled.clone(),
        )
        .unwrap();
        let stream = NativeStream::open(info, None, cancelled).unwrap();
        for _ in 0..2 {
            let mut url = reqwest::Url::parse(stream.remote.url.lock().unwrap().as_str()).unwrap();
            let pairs: Vec<_> = url
                .query_pairs()
                .filter(|(key, _)| key != "expire")
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect();
            url.query_pairs_mut()
                .clear()
                .extend_pairs(pairs)
                .append_pair("expire", "1");
            *stream.remote.url.lock().unwrap() = Zeroizing::new(url.into());
            stream.remote.cache.lock().unwrap().0.clear();
            stream.prebuffer().unwrap();
            let current = reqwest::Url::parse(stream.remote.url.lock().unwrap().as_str()).unwrap();
            assert!(!expires_soon(&current, SystemTime::now()));
        }
        stream.cancel();
    }
    use std::{
        io::Write,
        net::{TcpListener, TcpStream},
        process::Command,
        time::Instant,
    };

    #[test]
    fn pcm_span_publishes_new_rate_and_channels_at_the_boundary() {
        let shared = Arc::new(Shared {
            epoch: Arc::new(AtomicU64::new(0)),
            position: AtomicU64::new(0),
            ended: AtomicBool::new(false),
            buffering: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            error: Mutex::new(None),
        });
        let (tx, receiver) = mpsc::sync_channel(3);
        tx.send(Pcm {
            samples: vec![1.0, 2.0, 3.0, 4.0],
            channels: 1,
            rate: 44100,
            start_ms: 0,
            epoch: 0,
            end: false,
        })
        .unwrap();
        tx.send(Pcm {
            samples: vec![5.0, 6.0],
            channels: 2,
            rate: 48000,
            start_ms: 1,
            epoch: 0,
            end: false,
        })
        .unwrap();
        let mut source = PcmOutput {
            receiver,
            shared,
            chunk: vec![0.0, 0.0],
            at: 0,
            channels: 2,
            rate: 48000,
            epoch: 0,
            exhausted: false,
        };
        assert_eq!(source.current_span_len(), Some(2));
        assert_eq!(source.next(), Some(0.0));
        assert_eq!(source.next(), Some(0.0));
        assert_eq!(
            (
                source.channels(),
                source.sample_rate(),
                source.current_span_len()
            ),
            (1, 44100, Some(4))
        );
        assert_eq!(
            source.by_ref().take(4).collect::<Vec<_>>(),
            vec![1.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(
            (
                source.channels(),
                source.sample_rate(),
                source.current_span_len()
            ),
            (2, 48000, Some(2))
        );
        assert_eq!(source.next(), Some(5.0));
        assert_eq!(source.next(), Some(6.0));
        assert!(source.current_span_len().unwrap() > 0);
    }

    struct Server {
        url: String,
        stop: Arc<AtomicBool>,
        sent: Arc<AtomicU64>,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl Server {
        fn new(data: Vec<u8>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/audio", listener.local_addr().unwrap());
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let sent = Arc::new(AtomicU64::new(0));
            let worker_stop = stop.clone();
            let worker_sent = sent.clone();
            let worker = thread::spawn(move || {
                while !worker_stop.load(Ordering::Relaxed) {
                    let Ok((mut socket, _)) = listener.accept() else {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    };
                    if worker_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    socket
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut byte = [0];
                    while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
                        if socket.read(&mut byte).unwrap_or(0) == 0 {
                            break;
                        }
                        request.push(byte[0]);
                    }
                    let request = String::from_utf8_lossy(&request);
                    let path = request.split_whitespace().nth(1).unwrap_or("/");
                    let parsed = reqwest::Url::parse(&format!("http://localhost{path}")).unwrap();
                    let Some(range) = parsed
                        .query_pairs()
                        .find(|(key, _)| key == "range")
                        .map(|(_, value)| value.into_owned())
                    else {
                        continue;
                    };
                    let (start, end) = range.split_once('-').unwrap();
                    let start: usize = start.parse().unwrap();
                    let end: usize = end.parse().unwrap();
                    let body = &data[start..=end];
                    let header = format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    if socket.write_all(header.as_bytes()).is_ok() && socket.write_all(body).is_ok()
                    {
                        worker_sent.fetch_add(body.len() as u64, Ordering::Relaxed);
                    }
                }
            });
            Self {
                url,
                stop,
                sent,
                worker: Some(worker),
            }
        }
        fn stream(&self, bytes: u64, codec: &str) -> NativeStream {
            NativeStream {
                remote: Arc::new(RemoteData {
                    http: crate::http_audio::AudioHttp::local_test(),
                    url: Mutex::new(Zeroizing::new(self.url.clone())),
                    origin: String::new(),
                    session: None,
                    bytes,
                    user_agent: "Tonal test".into(),
                    format: "test".into(),
                    cancelled: Arc::new(AtomicBool::new(false)),
                    cache: Mutex::new((HashMap::new(), 0)),
                }),
                quality: AudioQuality {
                    format_id: "test".into(),
                    codec: codec.into(),
                    bitrate_kbps: Some(256.0),
                },
                duration: Some(Duration::from_secs(45)),
                loudness_db: None,
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ = TcpStream::connect(
                self.url
                    .trim_start_matches("http://")
                    .trim_end_matches("/audio"),
            );
            self.worker.take().unwrap().join().unwrap();
        }
    }

    #[test]
    fn compressed_cache_is_bounded_and_preload_is_half_a_megabyte() {
        let data = vec![42; BLOCK as usize * 40];
        let server = Server::new(data.clone());
        let stream = server.stream(data.len() as u64, "opus");
        stream.prebuffer().unwrap();
        assert_eq!(server.sent.load(Ordering::Relaxed), BLOCK * 2);
        for index in 2..40 {
            assert_eq!(stream.remote.block(index).unwrap()[0], 42);
        }
        let cache = stream.remote.cache.lock().unwrap();
        assert_eq!(cache.0.len(), CACHE_BLOCKS);
        assert_eq!(
            cache
                .0
                .values()
                .map(|(bytes, _)| bytes.len())
                .sum::<usize>(),
            8 * 1024 * 1024
        );
        assert!(cache.0.contains_key(&0) && cache.0.contains_key(&1));
        drop(cache);
        stream.cancel();
        assert!(stream.remote.block(0).is_err());
    }

    #[test]
    #[ignore = "Nécessite FFmpeg ; décode Opus et AAC par plages HTTP, puis teste déplacement et reprise après fin."]
    fn progressive_codecs_seek_and_replay() {
        let directory = tempfile::tempdir().unwrap();
        for (extension, codec, encoder) in
            [("m4a", "mp4a.40.2", "aac"), ("webm", "opus", "libopus")]
        {
            let path = directory.path().join(format!("fixture.{extension}"));
            let mut command = Command::new("ffmpeg");
            command.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=45",
                "-ac",
                "2",
                "-c:a",
                encoder,
                "-b:a",
                "256k",
            ]);
            if extension == "m4a" {
                command.args(["-movflags", "+faststart"]);
            }
            assert!(command.arg(&path).status().unwrap().success());
            let data = std::fs::read(path).unwrap();
            let bytes = data.len() as u64;
            let server = Server::new(data);
            let stream = server.stream(bytes, codec);
            let epoch = Arc::new(AtomicU64::new(0));
            let mut decoder =
                NativeDecoder::open(&stream, epoch.clone(), 0, Duration::ZERO).unwrap();
            let first = decoder.next(0).unwrap().unwrap();
            assert!(first.samples.iter().any(|sample| sample.abs() > 0.001));
            assert!(
                server.sent.load(Ordering::Relaxed) < bytes,
                "Must play before the entire file arrives: {extension}"
            );
            for millis in [25000, 4000] {
                let mut decoder =
                    NativeDecoder::open(&stream, epoch.clone(), 0, Duration::from_millis(millis))
                        .unwrap();
                let first = decoder.next(0).unwrap().unwrap();
                assert!(
                    first.start_ms.abs_diff(millis) < 100,
                    "{extension}: {}",
                    first.start_ms
                );
                assert!(first.samples.iter().any(|sample| sample.abs() > 0.001));
            }
            drop(decoder);
            let (control, mut output) = start(stream, false);
            let deadline = Instant::now();
            while control.position() < Duration::from_millis(100)
                && deadline.elapsed() < Duration::from_secs(5)
            {
                for _ in 0..960 {
                    output.next();
                }
                thread::sleep(Duration::from_millis(2));
            }
            assert!(control.position() >= Duration::from_millis(100));
            control.set_paused(true);
            let frozen = control.position();
            thread::sleep(Duration::from_millis(50));
            let sent = server.sent.load(Ordering::Relaxed);
            for _ in 0..5000 {
                assert_eq!(output.next(), Some(0.0));
            }
            thread::sleep(Duration::from_millis(100));
            assert_eq!(control.position(), frozen);
            assert_eq!(server.sent.load(Ordering::Relaxed), sent);
            control.seek(Duration::from_secs(25)).unwrap();
            thread::sleep(Duration::from_millis(100));
            assert_eq!(
                server.sent.load(Ordering::Relaxed),
                sent,
                "A seek while paused must not download audio"
            );
            control.set_paused(false);
            let deadline = Instant::now();
            while control.position() <= Duration::from_secs(25)
                && deadline.elapsed() < Duration::from_secs(5)
            {
                for _ in 0..960 {
                    output.next();
                }
                thread::sleep(Duration::from_millis(2));
            }
            assert!(
                control.position() > Duration::from_secs(25),
                "{extension}: {:?}",
                control.take_error()
            );
            control.seek(Duration::from_secs(44)).unwrap();
            let deadline = Instant::now();
            while !control.ended() && deadline.elapsed() < Duration::from_secs(5) {
                for _ in 0..9600 {
                    output.next();
                }
                thread::sleep(Duration::from_millis(2));
            }
            assert!(
                control.ended(),
                "{extension}, position={:?}, error={:?}",
                control.position(),
                control.take_error()
            );
            control.seek(Duration::ZERO).unwrap();
            let deadline = Instant::now();
            while control.position() < Duration::from_millis(100)
                && deadline.elapsed() < Duration::from_secs(5)
            {
                for _ in 0..960 {
                    assert!(output.next().is_some());
                }
                thread::sleep(Duration::from_millis(2));
            }
            assert!(control.position() >= Duration::from_millis(100));
            assert!(!control.ended());
        }
    }
}
