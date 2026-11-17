//! Cancellable media HTTP. Pausing drops the current request and its connection pool.
use reqwest::{Client, StatusCode, Url};
use std::{
    future::Future,
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};
use tokio::{runtime::Runtime, sync::watch};

pub struct AudioHttp {
    client: Mutex<Option<Client>>,
    paused: AtomicBool,
    abort: Mutex<Arc<AtomicBool>>,
    signal: watch::Sender<u64>,
    https_only: bool,
}
impl AudioHttp {
    pub fn new() -> Self {
        Self::with_https(true)
    }
    fn with_https(https_only: bool) -> Self {
        let (signal, _) = watch::channel(0);
        Self {
            client: Mutex::new(None),
            paused: AtomicBool::new(false),
            abort: Mutex::new(Arc::new(AtomicBool::new(false))),
            signal,
            https_only,
        }
    }
    #[cfg(test)]
    pub fn local_test() -> Self {
        Self::with_https(false)
    }
    pub fn token(&self) -> Arc<AtomicBool> {
        self.abort.lock().unwrap().clone()
    }
    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        self.interrupt();
        if paused {
            self.client.lock().unwrap().take();
        }
    }
    pub fn interrupt(&self) {
        let mut token = self.abort.lock().unwrap();
        token.store(true, Ordering::SeqCst);
        *token = Arc::new(AtomicBool::new(false));
        self.signal
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }
    fn client(&self) -> io::Result<Client> {
        let mut slot = self.client.lock().unwrap();
        if self.paused() {
            return Err(interrupted());
        }
        if let Some(client) = slot.as_ref() {
            return Ok(client.clone());
        }
        let client = Client::builder()
            .https_only(self.https_only)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() < 3
                    && crate::youtube::media_url_allowed(attempt.url().as_str())
                {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|_| io::Error::other("media client unavailable"))?;
        *slot = Some(client.clone());
        Ok(client)
    }
    pub(crate) fn execute<T>(
        &self,
        cancelled: &AtomicBool,
        token: &AtomicBool,
        task: impl Future<Output = Result<T, reqwest::Error>>,
    ) -> io::Result<T> {
        let mut signal = self.signal.subscribe();
        network_runtime()?.block_on(async {
            tokio::select! {
                biased;
                _ = async {
                    loop {
                        if cancelled.load(Ordering::SeqCst) || token.load(Ordering::SeqCst) || self.paused() { return; }
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                } => Err(interrupted()),
                _ = signal.changed() => Err(interrupted()),
                result=task => result.map_err(|_|io::Error::other("media request failed")),
            }
        })
    }
    pub fn length(&self, url: &str, agent: &str, cancelled: &AtomicBool) -> io::Result<u64> {
        let token = self.token();
        let client = self.client()?;
        self.execute(cancelled, &token, async {
            let response = client.head(url).header("User-Agent", agent).send().await?;
            Ok(if response.status().is_success() {
                response.content_length().unwrap_or(0)
            } else {
                0
            })
        })
    }
    pub fn range(
        &self,
        url: Url,
        agent: &str,
        expected: usize,
        cancelled: &AtomicBool,
        token: &AtomicBool,
    ) -> io::Result<(StatusCode, Vec<u8>)> {
        let client = self.client()?;
        self.execute(cancelled, token, async {
            let mut response = client.get(url).header("User-Agent", agent).send().await?;
            let status = response.status();
            let mut bytes = Vec::with_capacity(expected);
            if status.is_success() {
                while let Some(chunk) = response.chunk().await? {
                    let remaining = (expected + 1).saturating_sub(bytes.len());
                    bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                    if bytes.len() > expected {
                        break;
                    }
                }
            }
            Ok((status, bytes))
        })
    }
}
fn interrupted() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "media request interrupted")
}
pub(crate) fn network_runtime() -> io::Result<&'static Runtime> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .map_err(|_| "media runtime unavailable".into())
        })
        .as_ref()
        .map_err(|_| io::Error::other("media runtime unavailable"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Instant,
    };
    fn slow_server() -> (
        Url,
        mpsc::Receiver<()>,
        mpsc::Receiver<bool>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!("http://{}/audio", listener.local_addr().unwrap())).unwrap();
        let (ready_tx, ready) = mpsc::channel();
        let (closed_tx, closed) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                if socket.read(&mut byte).unwrap_or(0) == 0 {
                    return;
                }
                request.push(byte[0]);
            }
            socket.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 262144\r\nConnection: close\r\n\r\npartial").unwrap();
            ready_tx.send(()).unwrap();
            let disconnected = match socket.read(&mut byte) {
                Ok(0) => true,
                Err(error) => matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ),
                _ => false,
            };
            let _ = closed_tx.send(disconnected);
        });
        (url, ready, closed, worker)
    }
    #[test]
    fn pause_closes_an_inflight_audio_request_and_rejects_requests_until_resume() {
        let (url, ready, closed, server) = slow_server();
        let http = Arc::new(AudioHttp::local_test());
        let reader = http.clone();
        let target = url.clone();
        let worker = thread::spawn(move || {
            reader.range(
                target,
                "Tonal test",
                262144,
                &AtomicBool::new(false),
                &reader.token(),
            )
        });
        ready.recv_timeout(Duration::from_secs(3)).unwrap();
        let started = Instant::now();
        http.set_paused(true);
        let result = worker.join().unwrap();
        assert!(matches!(result,Err(error) if error.kind()==io::ErrorKind::Interrupted));
        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(
            closed.recv_timeout(Duration::from_secs(2)).unwrap(),
            "Pausing must close the active TCP request"
        );
        server.join().unwrap();
        assert!(http
            .range(url, "Tonal test", 1, &AtomicBool::new(false), &http.token())
            .is_err());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!("http://{}/audio", listener.local_addr().unwrap())).unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                assert!(request.len() < 8192);
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            socket.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\nConnection: close\r\n\r\n1234").unwrap();
        });
        http.set_paused(false);
        let (status, body) = http
            .range(url, "Tonal test", 4, &AtomicBool::new(false), &http.token())
            .unwrap();
        assert!(status.is_success());
        assert_eq!(body, b"1234");
        server.join().unwrap();
    }
    #[test]
    fn cancellation_closes_inflight_prefetch_even_without_a_pause_notification() {
        let (url, ready, closed, server) = slow_server();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancelled.clone();
        let worker = thread::spawn(move || {
            let http = AudioHttp::local_test();
            http.range(url, "Tonal test", 262144, &worker_cancel, &http.token())
        });
        ready.recv_timeout(Duration::from_secs(3)).unwrap();
        let started = Instant::now();
        cancelled.store(true, Ordering::SeqCst);
        assert!(worker.join().unwrap().is_err());
        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(closed.recv_timeout(Duration::from_secs(2)).unwrap());
        server.join().unwrap();
    }
}
