//! Bounded cosmetic PCM bridge. Empty queues produce silence, never block the
//! audio device. Simulation state and snapshot hashes contain no audio samples.
//! The provider renders interleaved stereo (left, right) at 48 kHz.
use bevy::prelude::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
/// Queued interleaved samples: at most 250 ms of stereo.
const CAPACITY: usize = 24000;
/// Start (or resume) playback once 32 ms of stereo is queued.
const PRIME: usize = 3072;
#[derive(Asset, TypePath, Clone, Default)]
pub(crate) struct Stream(pub Arc<Mutex<VecDeque<f32>>>);
pub(crate) struct Decoder {
    stream: Stream,
    ready: bool,
    /// Samples emitted so far (its parity is the channel of the next one).
    at: usize,
    last: [f32; 2],
}
impl Iterator for Decoder {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let channel = self.at & 1;
        self.at = self.at.wrapping_add(1);
        if let Ok(mut q) = self.stream.0.lock() {
            // Resume only on a frame boundary so left/right never swap.
            if !self.ready && channel == 0 && q.len() >= PRIME {
                self.ready = true;
            }
            if self.ready {
                if let Some(sample) = q.pop_front() {
                    self.last[channel] = sample;
                    return Some(sample);
                }
                self.ready = false;
            }
        }
        self.last[channel] *= 0.98;
        Some(self.last[channel])
    }
}
impl rodio::Source for Decoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> std::num::NonZeroU16 {
        std::num::NonZeroU16::new(2).unwrap()
    }
    fn sample_rate(&self) -> std::num::NonZeroU32 {
        std::num::NonZeroU32::new(mechanics::link::AUDIO_RATE).unwrap()
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
            at: 0,
            last: [0.; 2],
        }
    }
}
#[derive(Component)]
pub(crate) struct Playing;
impl Stream {
    /// Append interleaved stereo; an odd trailing sample is dropped.
    pub fn push(&self, mut samples: Vec<f32>) {
        samples.truncate(samples.len() & !1);
        if let Ok(mut q) = self.0.lock() {
            q.extend(samples);
            let excess = q.len().saturating_sub(CAPACITY) & !1;
            q.drain(..excess);
        }
    }
    pub fn clear(&self) {
        if let Ok(mut q) = self.0.lock() {
            q.clear();
        }
    }
}
