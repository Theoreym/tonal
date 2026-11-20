//! Keep the same sink/decoder while replacing the physical output stream.
use rodio::{
    mixer::{Mixer, MixerSource},
    Source,
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

pub struct Route {
    pub mixer: Mixer,
    source: Arc<Mutex<MixerSource>>,
    generation: Arc<AtomicU64>,
    channels: u16,
    rate: u32,
}
impl Route {
    #[cfg(test)]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub fn new(channels: u16, rate: u32) -> Self {
        let (mixer, source) = rodio::mixer::mixer(channels, rate);
        Self {
            mixer,
            source: Arc::new(Mutex::new(source)),
            generation: Arc::new(AtomicU64::new(0)),
            channels,
            rate,
        }
    }
    pub fn attach(&self, output: &Mixer) {
        output.add(self.reader());
    }
    fn reader(&self) -> RoutedSource {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        RoutedSource {
            source: self.source.clone(),
            generation: self.generation.clone(),
            current: generation,
            channels: self.channels,
            rate: self.rate,
            samples: vec![0.0; usize::from(self.channels) * 256],
            at: usize::from(self.channels) * 256,
        }
    }
}
struct RoutedSource {
    source: Arc<Mutex<MixerSource>>,
    generation: Arc<AtomicU64>,
    current: u64,
    channels: u16,
    rate: u32,
    samples: Vec<f32>,
    at: usize,
}
impl Iterator for RoutedSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.generation.load(Ordering::Acquire) != self.current {
            return None;
        }
        if self.at == self.samples.len() {
            // One nonblocking lock per 256 frames, never a lock per sample or a network wait.
            if let Ok(mut source) = self.source.try_lock() {
                if self.generation.load(Ordering::Acquire) != self.current {
                    return None;
                }
                for sample in &mut self.samples {
                    *sample = source.next().unwrap_or(0.0);
                }
            } else {
                self.samples.fill(0.0);
            }
            self.at = 0;
        }
        let sample = self.samples[self.at];
        self.at += 1;
        Some(sample)
    }
}
impl Source for RoutedSource {
    fn current_span_len(&self) -> Option<usize> {
        None
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_retires_the_old_reader_and_preserves_the_same_input_queue() {
        let route = Route::new(2, 48000);
        let sink = rodio::Sink::connect_new(&route.mixer);
        sink.append(rodio::buffer::SamplesBuffer::new(
            2,
            48000,
            vec![0.25; 96000],
        ));
        let mut first = route.reader();
        assert!(first.by_ref().take(128).any(|sample| sample > 0.0));
        let mut second = route.reader();
        assert_eq!(first.next(), None);
        assert!(second.by_ref().take(128).any(|sample| sample > 0.0));
        assert!(!sink.empty());
        sink.pause();
        let mut third = route.reader();
        assert_eq!(second.next(), None);
        assert!(sink.is_paused());
        // Rodio applies sink controls periodically; allow its short control interval to drain.
        third.by_ref().take(1024).for_each(drop);
        assert!(third.by_ref().take(1024).all(|sample| sample == 0.0));
    }
    #[test]
    fn a_new_device_format_adapts_the_same_playback_queue() {
        let route = Route::new(2, 48000);
        let sink = rodio::Sink::connect_new(&route.mixer);
        sink.append(rodio::buffer::SamplesBuffer::new(
            2,
            48000,
            vec![0.25; 96000],
        ));
        let (stereo, mut first) = rodio::mixer::mixer(2, 48000);
        route.attach(&stereo);
        assert!(first.by_ref().take(128).any(|sample| sample > 0.0));
        let (mono, mut second) = rodio::mixer::mixer(1, 44100);
        route.attach(&mono);
        assert_eq!(second.channels(), 1);
        assert_eq!(second.sample_rate(), 44100);
        assert!(second.by_ref().take(1024).any(|sample| sample > 0.0));
        assert!(!sink.empty());
    }
    #[test]
    fn output_callback_never_waits_for_a_busy_mixer() {
        let route = Route::new(2, 48000);
        let mut reader = route.reader();
        let _locked = route.source.lock().unwrap();
        let start = std::time::Instant::now();
        assert!(reader.by_ref().take(1024).all(|sample| sample == 0.0));
        assert!(start.elapsed() < Duration::from_millis(50));
    }
}
