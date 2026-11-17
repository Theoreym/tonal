//! Public YouTube player preparation and signature solving, isolated from account secrets.
//! Only the official EJS assets and public player source enter QuickJS. No host I/O bindings.
use crate::http_audio::AudioHttp;
use reqwest::Client;
use rquickjs::{Context, Runtime};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const LIB: &str = include_str!("../assets/ejs/lib.min.js");
const CORE: &str = include_str!("../assets/ejs/core.min.js");
const EJS: &str = "0.8.0";
const SOURCE_LIMIT: usize = 8 * 1024 * 1024;
const SCRIPT_LIMIT: usize = 12 * 1024 * 1024;
const CACHE_LIMIT: u64 = 24 * 1024 * 1024;
const PLAYER_TTL: Duration = Duration::from_secs(3600);
const IDLE: Duration = Duration::from_secs(90);

#[derive(Clone)]
pub(crate) struct Player {
    id: String,
    pub(crate) timestamp: u32,
}
pub(crate) struct Solution {
    pub(crate) n: Option<String>,
    pub(crate) signature: Option<String>,
}
enum Task {
    Prepare,
    Solve {
        player: Player,
        n: Option<String>,
        signature: Option<String>,
    },
}
enum Answer {
    Prepared(Player),
    Solved(Solution),
}
struct Job {
    task: Task,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    reply: SyncSender<Result<Answer, ()>>,
}
static WORKER: Mutex<Option<SyncSender<Job>>> = Mutex::new(None);
static WARMING: AtomicBool = AtomicBool::new(false);

/// Public preparation only: warming does not start any music or register a listening session.
pub(crate) fn warm() {
    if WARMING.swap(true, Ordering::AcqRel) {
        return;
    }
    if std::thread::Builder::new()
        .name("tonal-player-warm".into())
        .spawn(|| {
            let _ = request(
                Task::Prepare,
                &AtomicBool::new(false),
                None,
                Duration::from_secs(25),
            );
            WARMING.store(false, Ordering::Release);
        })
        .is_err()
    {
        WARMING.store(false, Ordering::Release);
    }
}
pub(crate) fn prepare(
    cancelled: &AtomicBool,
    interrupted: Option<&AtomicBool>,
) -> Result<Player, ()> {
    match request(
        Task::Prepare,
        cancelled,
        interrupted,
        Duration::from_secs(6),
    )? {
        Answer::Prepared(player) => Ok(player),
        _ => Err(()),
    }
}
#[cfg(test)]
pub(crate) fn prepare_cold_for_test() -> Result<Player, ()> {
    match request(
        Task::Prepare,
        &AtomicBool::new(false),
        None,
        Duration::from_secs(30),
    )? {
        Answer::Prepared(player) => Ok(player),
        _ => Err(()),
    }
}
pub(crate) fn solve(
    player: &Player,
    n: Option<String>,
    signature: Option<String>,
    cancelled: &AtomicBool,
    interrupted: Option<&AtomicBool>,
) -> Result<Solution, ()> {
    if n.as_ref().is_some_and(|s| s.len() > 4096)
        || signature.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return Err(());
    }
    match request(
        Task::Solve {
            player: player.clone(),
            n,
            signature,
        },
        cancelled,
        interrupted,
        Duration::from_secs(4),
    )? {
        Answer::Solved(solution) => Ok(solution),
        _ => Err(()),
    }
}
fn stopped(cancelled: &AtomicBool, interrupted: Option<&AtomicBool>) -> bool {
    cancelled.load(Ordering::Acquire) || interrupted.is_some_and(|a| a.load(Ordering::Acquire))
}
fn request(
    task: Task,
    cancelled: &AtomicBool,
    interrupted: Option<&AtomicBool>,
    limit: Duration,
) -> Result<Answer, ()> {
    if stopped(cancelled, interrupted) {
        return Err(());
    }
    let deadline = Instant::now() + limit;
    let abort = Arc::new(AtomicBool::new(false));
    let (reply, response) = mpsc::sync_channel(1);
    let mut job = Job {
        task,
        cancelled: abort.clone(),
        deadline,
        reply,
    };
    {
        let mut worker = WORKER.lock().map_err(|_| ())?;
        if let Some(tx) = worker.as_ref() {
            match tx.try_send(job) {
                Ok(()) => {
                    drop(worker);
                    return receive(response, abort, deadline, cancelled, interrupted);
                }
                Err(TrySendError::Disconnected(returned)) => job = returned,
                Err(TrySendError::Full(_)) => return Err(()),
            }
        }
        let (tx, rx) = mpsc::sync_channel(4);
        std::thread::Builder::new()
            .name("tonal-quickjs".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || work(rx))
            .map_err(|_| ())?;
        tx.try_send(job).map_err(|_| ())?;
        *worker = Some(tx);
    }
    receive(response, abort, deadline, cancelled, interrupted)
}
fn receive(
    response: Receiver<Result<Answer, ()>>,
    abort: Arc<AtomicBool>,
    deadline: Instant,
    cancelled: &AtomicBool,
    interrupted: Option<&AtomicBool>,
) -> Result<Answer, ()> {
    loop {
        if stopped(cancelled, interrupted) || Instant::now() >= deadline {
            abort.store(true, Ordering::Release);
            return Err(());
        }
        match response.recv_timeout(Duration::from_millis(20)) {
            Ok(answer) => return answer,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(()),
        }
    }
}

struct Loaded {
    player: Player,
    checked: Instant,
    context: Context,
    runtime: Runtime,
}
fn work(rx: Receiver<Job>) {
    let mut loaded: Option<Loaded> = None;
    let mut failed: Option<Instant> = None;
    while let Ok(job) = rx.recv_timeout(IDLE) {
        if job.cancelled.load(Ordering::Acquire) || Instant::now() >= job.deadline {
            continue;
        }
        let answer = match job.task {
            Task::Prepare => {
                if loaded
                    .as_ref()
                    .is_none_or(|p| p.checked.elapsed() >= PLAYER_TTL)
                {
                    if failed.is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
                        let _ = job.reply.send(Err(()));
                        continue;
                    }
                    match load(&job.cancelled, job.deadline) {
                        Ok(player) => {
                            loaded = Some(player);
                            failed = None;
                        }
                        Err(()) => {
                            if !job.cancelled.load(Ordering::Acquire) {
                                failed = Some(Instant::now());
                            }
                            let _ = job.reply.send(Err(()));
                            continue;
                        }
                    }
                }
                Ok(Answer::Prepared(loaded.as_ref().unwrap().player.clone()))
            }
            Task::Solve {
                player,
                n,
                signature,
            } => match loaded.as_ref().filter(|p| p.player.id == player.id) {
                Some(engine) => {
                    watchdog(&engine.runtime, &job.cancelled, job.deadline);
                    run(&engine.context, n, signature).map(Answer::Solved)
                }
                None => Err(()),
            },
        };
        let _ = job.reply.send(answer);
    }
}
fn watchdog(runtime: &Runtime, cancelled: &Arc<AtomicBool>, deadline: Instant) {
    let cancelled = cancelled.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        cancelled.load(Ordering::Acquire) || Instant::now() >= deadline
    })));
}
fn engine(
    cancelled: &Arc<AtomicBool>,
    deadline: Instant,
    memory: usize,
) -> Result<(Runtime, Context), ()> {
    let runtime = Runtime::new().map_err(|_| ())?;
    runtime.set_memory_limit(memory);
    runtime.set_max_stack_size(8 * 1024 * 1024);
    watchdog(&runtime, cancelled, deadline);
    let context = Context::full(&runtime).map_err(|_| ())?;
    Ok((runtime, context))
}
#[derive(Serialize, Deserialize)]
struct Cached {
    ejs: String,
    id: String,
    timestamp: u32,
    script: String,
}
fn cache_path() -> Option<PathBuf> {
    Some(
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
            .join("Tonal")
            .join("player-ejs.json"),
    )
}
fn cached(id: &str) -> Option<Cached> {
    let file = std::fs::File::open(cache_path()?).ok()?;
    if file.metadata().ok()?.len() > CACHE_LIMIT {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(CACHE_LIMIT + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > CACHE_LIMIT {
        return None;
    }
    let cache: Cached = serde_json::from_slice(&bytes).ok()?;
    (cache.ejs == EJS
        && cache.id == id
        && (10000..=99999).contains(&cache.timestamp)
        && !cache.script.is_empty()
        && cache.script.len() <= SCRIPT_LIMIT)
        .then_some(cache)
}
fn save(cache: &Cached) {
    let Some(path) = cache_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    // Atomic replacement; a partial or stale cache is ignored and fetched again.
    if let Ok(mut file) = tempfile::NamedTempFile::new_in(parent) {
        if serde_json::to_writer(&mut file, cache).is_ok() {
            let _ = file.persist(path);
        }
    }
}
fn fetch(
    client: &Client,
    http: &AudioHttp,
    url: &str,
    limit: usize,
    cancelled: &AtomicBool,
) -> Result<String, ()> {
    let never = AtomicBool::new(false);
    let bytes = http
        .execute(cancelled, &never, async {
            let mut response = client.get(url).send().await?.error_for_status()?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                let remaining = (limit + 1).saturating_sub(bytes.len());
                bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                if bytes.len() > limit {
                    break;
                }
            }
            Ok(bytes)
        })
        .map_err(|_| ())?;
    if bytes.len() > limit {
        return Err(());
    }
    String::from_utf8(bytes).map_err(|_| ())
}
fn load(cancelled: &Arc<AtomicBool>, deadline: Instant) -> Result<Loaded, ()> {
    #[cfg(test)]
    let started = Instant::now();
    let client = Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(6))
        .build()
        .map_err(|_| ())?;
    let http = AudioHttp::new();
    // These public requests never use the authenticated Music client or its headers.
    let iframe = fetch(
        &client,
        &http,
        "https://www.youtube.com/iframe_api",
        256 * 1024,
        cancelled,
    )?;
    let id = player_id(&iframe).ok_or(())?;
    #[cfg(test)]
    eprintln!("Public player version fetched: {:?}", started.elapsed());
    let mut from_disk = true;
    let cache = if let Some(cache) = cached(&id) {
        cache
    } else {
        from_disk = false;
        let source = fetch(
            &client,
            &http,
            &format!("https://www.youtube.com/s/player/{id}/player_ias.vflset/en_US/base.js"),
            SOURCE_LIMIT,
            cancelled,
        )?;
        let timestamp = timestamp(&source).ok_or(())?;
        #[cfg(test)]
        eprintln!(
            "Public player source fetched ({} bytes): {:?}",
            source.len(),
            started.elapsed()
        );
        let script = preprocess(&source, cancelled, deadline)?;
        #[cfg(test)]
        eprintln!(
            "Public player preprocessing complete: {:?}",
            started.elapsed()
        );
        Cached {
            ejs: EJS.into(),
            id: id.clone(),
            timestamp,
            script,
        }
    };
    let (runtime, context) = engine(cancelled, deadline, 96 * 1024 * 1024)?;
    let initialized = context.with(|ctx| {
        ctx.globals().set("__tonal_source", cache.script.as_str()).map_err(|_| ())?;
        ctx.eval::<(), _>("var __tonal_result = {n:null,sig:null}; Function('_result', __tonal_source)(__tonal_result); delete globalThis.__tonal_source;").map_err(|_| ())
    });
    if initialized.is_err()
        && from_disk
        && !cancelled.load(Ordering::Acquire)
        && Instant::now() < deadline
    {
        // A corrupt/incompatible source cache must not permanently disable native playback.
        if let Some(path) = cache_path() {
            let _ = std::fs::remove_file(path);
        }
    }
    initialized?;
    #[cfg(test)]
    eprintln!(
        "Public player loaded, disk cache={from_disk}: {:?}",
        started.elapsed()
    );
    if !from_disk {
        save(&cache);
    }
    Ok(Loaded {
        player: Player {
            id,
            timestamp: cache.timestamp,
        },
        checked: Instant::now(),
        context,
        runtime,
    })
}
fn preprocess(source: &str, cancelled: &Arc<AtomicBool>, deadline: Instant) -> Result<String, ()> {
    let (_runtime, context) = engine(cancelled, deadline, 256 * 1024 * 1024)?;
    context.with(|ctx| {
        ctx.eval::<(), _>(LIB).map_err(|_| ())?;
        ctx.eval::<(), _>("Object.assign(globalThis, lib)").map_err(|_| ())?;
        ctx.eval::<(), _>(CORE).map_err(|_| ())?;
        ctx.globals().set("__tonal_player", source).map_err(|_| ())?;
        let script: String = ctx.eval("jsc({type:'player',player:__tonal_player,output_preprocessed:true,requests:[]}).preprocessed_player").map_err(|_| ())?;
        if script.is_empty() || script.len() > SCRIPT_LIMIT { return Err(()); }
        Ok(script)
    })
}
fn run(context: &Context, n: Option<String>, signature: Option<String>) -> Result<Solution, ()> {
    context.with(|ctx| {
        let challenge = |kind: &str, input: Option<String>| -> Result<Option<String>, ()> {
            let Some(input) = input else {
                return Ok(None);
            };
            ctx.globals()
                .set("__tonal_input", input.as_str())
                .map_err(|_| ())?;
            let output: Result<String, _> =
                ctx.eval(format!("__tonal_result.{kind}(__tonal_input)"));
            let _ = ctx.globals().remove("__tonal_input");
            let output = output.map_err(|_| ())?;
            if output.is_empty() || output.len() > 4096 || output.starts_with("enhanced_except_") {
                return Err(());
            }
            Ok(Some(output))
        };
        Ok(Solution {
            n: challenge("n", n)?,
            signature: challenge("sig", signature)?,
        })
    })
}
fn player_id(source: &str) -> Option<String> {
    source
        .match_indices("/s/player/")
        .find_map(|(i, _)| {
            let rest = &source[i + 10..];
            let id = rest.get(..8)?;
            (id.bytes().all(|b| b.is_ascii_hexdigit()) && rest.as_bytes().get(8) == Some(&b'/'))
                .then(|| id.to_owned())
        })
        .or_else(|| {
            // iframe_api escapes its literal URL's slashes.
            source
                .contains("\\/")
                .then(|| player_id(&source.replace("\\/", "/")))
                .flatten()
        })
}
fn timestamp(source: &str) -> Option<u32> {
    ["signatureTimestamp", "sts"].iter().find_map(|key| {
        source.match_indices(key).find_map(|(i, _)| {
            let digits = source[i + key.len()..]
                .trim_start()
                .strip_prefix(':')?
                .trim_start();
            let end = digits
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(digits.len());
            (end == 5).then(|| digits[..end].parse().ok()).flatten()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_player_identifiers_are_bounded_and_cannot_change_the_fetch_origin() {
        assert_eq!(
            player_id(r"https:\/\/www.youtube.com\/s\/player\/5203c085\/base.js").as_deref(),
            Some("5203c085")
        );
        assert!(player_id("/s/player/../../xx/base.js").is_none());
        assert!(player_id("/s/player/123456789/base.js").is_none());
        assert_eq!(
            timestamp("sts:123,signatureTimestamp:20733,x:1"),
            Some(20733)
        );
    }
    #[test]
    fn watchdog_interrupts_untrusted_script_and_solver_validates_results() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (runtime, context) = engine(
            &cancelled,
            Instant::now() + Duration::from_millis(100),
            16 * 1024 * 1024,
        )
        .unwrap();
        context
            .with(|ctx| {
                ctx.eval::<(), _>(
                    "var __tonal_result={n:x=>'n-'+x,sig:x=>x.split('').reverse().join('')};",
                )
            })
            .unwrap();
        let solution = run(&context, Some("token".into()), Some("abc".into())).unwrap();
        assert_eq!(solution.n.as_deref(), Some("n-token"));
        assert_eq!(solution.signature.as_deref(), Some("cba"));
        watchdog(
            &runtime,
            &cancelled,
            Instant::now() + Duration::from_millis(10),
        );
        assert!(context
            .with(|ctx| ctx.eval::<(), _>("while(true) {};"))
            .is_err());
    }
    #[test]
    fn cancelled_jobs_never_start_a_worker() {
        assert!(prepare(&AtomicBool::new(true), None).is_err());
    }
}
