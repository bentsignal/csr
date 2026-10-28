//! Bounded cosmetic PCM bridge. Empty queues produce silence, never block the
//! audio device. Simulation state and snapshot hashes contain no audio samples.
use bevy::prelude::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
#[derive(Asset, TypePath, Clone, Default)]
pub(crate) struct Stream(pub Arc<Mutex<VecDeque<f32>>>);
pub(crate) struct Decoder {
    stream: Stream,
    ready: bool,
    last: f32,
}
impl Iterator for Decoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Ok(mut q) = self.stream.0.lock() {
            if !self.ready && q.len() >= 1536 {
                self.ready = true;
            }
            if self.ready {
                if let Some(sample) = q.pop_front() {
                    self.last = sample;
                    return Some(sample);
                }
                self.ready = false;
            }
        }
        self.last *= 0.98;
        Some(self.last)
    }
}
impl rodio::Source for Decoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> std::num::NonZeroU16 {
        std::num::NonZeroU16::new(1).unwrap()
    }
    fn sample_rate(&self) -> std::num::NonZeroU32 {
        std::num::NonZeroU32::new(48000).unwrap()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}
impl bevy::audio::Decodable for Stream {
    type Decoder = Decoder;
    fn decoder(&self) -> Decoder {
        Decoder {
            stream: self.clone(),
            ready: false,
            last: 0.,
        }
    }
}
#[derive(Component)]
pub(crate) struct Playing;
impl Stream {
    pub fn push(&self, samples: Vec<f32>) {
        if let Ok(mut q) = self.0.lock() {
            q.extend(samples);
            while q.len() > 12000 {
                q.pop_front();
            }
        }
    }
    pub fn clear(&self) {
        if let Ok(mut q) = self.0.lock() {
            q.clear();
        }
    }
}
