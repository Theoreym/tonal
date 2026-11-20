//! Lazy, bounded native artwork loading. No account headers are sent to image hosts.
use eframe::egui::{self, pos2, Rect, TextureHandle};
use reqwest::blocking::Client;
use std::{
    collections::HashMap,
    io::{Cursor, Read},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

pub fn allowed_url(text: &str) -> bool {
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            ["ytimg.com", "googleusercontent.com", "ggpht.com"]
                .iter()
                .any(|base| host == *base || host.ends_with(&format!(".{base}")))
        })
}

enum ImageState {
    Loading,
    Ready(TextureHandle),
    Failed { retry_at: Instant },
}
struct Entry {
    state: ImageState,
    used: u64,
    failures: u8,
}
type Download = (String, Option<egui::ColorImage>);
pub struct ArtworkCache {
    client: Option<Client>,
    entries: HashMap<String, Entry>,
    sender: Sender<Download>,
    receiver: Receiver<Download>,
    active: usize,
    clock: u64,
}

impl ArtworkCache {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            client: Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(12))
                .build()
                .ok(),
            entries: HashMap::new(),
            sender,
            receiver,
            active: 0,
            clock: 0,
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        for (url, image) in self.receiver.try_iter() {
            self.active = self.active.saturating_sub(1);
            if let Some(entry) = self.entries.get_mut(&url) {
                entry.state = image
                    .map(|image| {
                        ImageState::Ready(ctx.load_texture(
                            "artwork",
                            image,
                            egui::TextureOptions::LINEAR,
                        ))
                    })
                    .unwrap_or_else(|| {
                        entry.failures = entry.failures.saturating_add(1);
                        let delay = retry_delay(entry.failures);
                        ctx.request_repaint_after(delay);
                        ImageState::Failed {
                            retry_at: Instant::now() + delay,
                        }
                    });
            }
        }
    }

    pub fn texture(&mut self, ctx: &egui::Context, url: Option<&str>) -> Option<TextureHandle> {
        let url = url.filter(|url| allowed_url(url))?;
        self.load(ctx, url)
    }

    pub fn radio_texture(&mut self, ctx: &egui::Context, url: &str) -> Option<TextureHandle> {
        crate::radio_net::allowed(url).then_some(())?;
        self.load_kind(ctx, url, true)
    }

    fn load(&mut self, ctx: &egui::Context, url: &str) -> Option<TextureHandle> {
        self.load_kind(ctx, url, false)
    }

    fn load_kind(&mut self, ctx: &egui::Context, url: &str, radio: bool) -> Option<TextureHandle> {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(url) {
            entry.used = self.clock;
            match &entry.state {
                ImageState::Ready(texture) => return Some(texture.clone()),
                ImageState::Loading => return None,
                ImageState::Failed { retry_at } if Instant::now() < *retry_at => {
                    ctx.request_repaint_after(retry_at.saturating_duration_since(Instant::now()));
                    return None;
                }
                ImageState::Failed { .. } => {}
            }
        }
        if self.active >= 4 {
            return None;
        }
        let client = self.client.clone()?;
        if !self.entries.contains_key(url) && self.entries.len() >= 64 {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, e)| !matches!(e.state, ImageState::Loading))
                .min_by_key(|(_, e)| e.used)
                .map(|(url, _)| url.clone());
            if let Some(oldest) = oldest {
                self.entries.remove(&oldest);
            }
        }
        self.entries
            .entry(url.to_owned())
            .and_modify(|entry| entry.state = ImageState::Loading)
            .or_insert(Entry {
                state: ImageState::Loading,
                used: self.clock,
                failures: 0,
            });
        self.active += 1;
        let url = url.to_owned();
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let image = if radio {
                download_radio(&url)
            } else {
                download(&client, &url)
            };
            let _ = sender.send((url, image));
            ctx.request_repaint();
        });
        None
    }

    pub fn paint(&mut self, ui: &egui::Ui, rect: Rect, url: Option<&str>, radius: u8) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        if let Some(texture) = self.texture(ui.ctx(), url) {
            paint_texture(ui, rect, &texture, radius);
        } else {
            crate::ui::cover(ui.painter(), rect, 0);
        }
    }
}

fn retry_delay(failures: u8) -> Duration {
    Duration::from_secs((2_u64 << failures.saturating_sub(1).min(6)).min(120))
}

pub fn paint_texture(ui: &egui::Ui, rect: Rect, texture: &TextureHandle, radius: u8) {
    let size = texture.size_vec2();
    let side = size.x.min(size.y);
    let uv = Rect::from_center_size(pos2(0.5, 0.5), egui::vec2(side / size.x, side / size.y));
    egui::Image::from_texture(texture)
        .uv(uv)
        .corner_radius(radius)
        .paint_at(ui, rect);
}

fn download(client: &Client, url: &str) -> Option<egui::ColorImage> {
    let response = client.get(url).send().ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|size| size > 4 * 1024 * 1024)
    {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4 * 1024 * 1024 {
        return None;
    }
    decode(&bytes)
}

fn download_radio(url: &str) -> Option<egui::ColorImage> {
    crate::http_audio::network_runtime().ok()?.block_on(async {
        tokio::time::timeout(Duration::from_secs(15), async {
            let mut response = crate::radio_net::get(url).await.ok()?;
            if response
                .content_length()
                .is_some_and(|size| size > 4 * 1024 * 1024)
            {
                return None;
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.ok()? {
                if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                    return None;
                }
                bytes.extend_from_slice(&chunk);
            }
            decode(&bytes)
        })
        .await
        .ok()
        .flatten()
    })
}

fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let image = if image.width() > 512 || image.height() > 512 {
        image.thumbnail(512, 512)
    } else {
        image
    }
    .to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_transient_failure_is_retried_without_dropping_ready_artwork() {
        use std::{io::Write, net::TcpListener, time::Instant};
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/cover", server.local_addr().unwrap());
        let worker = thread::spawn(move || {
            for attempt in 0..2 {
                let (mut socket, _) = server.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") {
                    assert!(header.len() < 8192);
                    socket.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                if attempt == 0 {
                    socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    let bytes = include_bytes!("../assets/tonal.png");
                    write!(
                        socket,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .unwrap();
                    socket.write_all(bytes).unwrap();
                }
            }
        });
        let ctx = egui::Context::default();
        let mut cache = ArtworkCache::new();
        // Only this private test path allows a local HTTP fixture. Public texture() still validates HTTPS hosts.
        cache.client = Some(Client::new());
        let stable = ctx.load_texture(
            "stable",
            decode(include_bytes!("../assets/tonal.png")).unwrap(),
            egui::TextureOptions::LINEAR,
        );
        let stable_id = stable.id();
        cache.entries.insert(
            "stable".into(),
            Entry {
                state: ImageState::Ready(stable),
                used: 0,
                failures: 0,
            },
        );
        assert!(cache.load(&ctx, &url).is_none());
        let deadline = Instant::now();
        while cache.active != 0 && deadline.elapsed() < Duration::from_secs(3) {
            cache.poll(&ctx);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            cache.entries[&url].state,
            ImageState::Failed { .. }
        ));
        assert!(cache.load(&ctx, &url).is_none());
        assert_eq!(cache.active, 0, "No retry before its backoff expires");
        cache.entries.get_mut(&url).unwrap().state = ImageState::Failed {
            retry_at: Instant::now() - Duration::from_secs(1),
        };
        assert!(cache.load(&ctx, &url).is_none());
        assert_eq!(cache.active, 1);
        let deadline = Instant::now();
        while cache.active != 0 && deadline.elapsed() < Duration::from_secs(3) {
            cache.poll(&ctx);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(cache.load(&ctx, &url).is_some());
        assert_eq!(cache.load(&ctx, "stable").unwrap().id(), stable_id);
        assert_eq!(retry_delay(1), Duration::from_secs(2));
        assert_eq!(retry_delay(u8::MAX), Duration::from_secs(120));
        worker.join().unwrap();
        ctx.run_ui(Default::default(), |_| {})
            .textures_delta
            .clear();
    }
    #[test]
    fn artwork_is_limited_to_public_https_image_hosts() {
        assert!(allowed_url(
            "https://lh3.googleusercontent.com/cover=w544-h544"
        ));
        assert!(allowed_url("https://i.ytimg.com/vi/test/hqdefault.jpg"));
        for url in [
            "http://i.ytimg.com/cover",
            "https://i.ytimg.com.evil.test/cover",
            "https://user:secret@lh3.ggpht.com/cover",
            "https://127.0.0.1/cover",
        ] {
            assert!(!allowed_url(url));
        }
        let pixels = decode(include_bytes!("../assets/tonal.png")).unwrap();
        assert_eq!(pixels.size, [256, 256]);
        assert!(decode(b"not an image").is_none());
    }
}
