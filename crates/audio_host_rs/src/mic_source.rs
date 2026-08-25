//! Microphone Stream Source for host capture emulation (MediaStreamAudioSourceNode).

use std::f32::consts::PI;
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq)]
pub enum MicPattern {
    SineTone(f32), // Frequency in Hz
    VoiceFormant,  // Synthesized vocal vowel formant (~300Hz fundamental + 2kHz resonance)
    WhiteNoise,
    Silence,
}

pub struct MicStreamSource {
    pattern: RwLock<MicPattern>,
    phase: RwLock<f32>,
    amplitude: f32,
}

impl Default for MicStreamSource {
    fn default() -> Self {
        Self {
            pattern: RwLock::new(MicPattern::SineTone(440.0)),
            phase: RwLock::new(0.0),
            amplitude: 0.5,
        }
    }
}

impl MicStreamSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pattern(pattern: MicPattern, amplitude: f32) -> Self {
        Self {
            pattern: RwLock::new(pattern),
            phase: RwLock::new(0.0),
            amplitude: amplitude.clamp(0.0, 1.0),
        }
    }

    pub fn set_pattern(&self, pattern: MicPattern) {
        let mut p = self.pattern.write().unwrap();
        *p = pattern;
    }

    /// Generate 16-bit PCM buffer for specified frames, sample rate, and channel count.
    pub fn generate_pcm_16bit(
        &self,
        frame_count: usize,
        sample_rate: u32,
        channel_count: u32,
    ) -> Vec<u8> {
        let pattern = self.pattern.read().unwrap().clone();
        let mut phase = self.phase.write().unwrap();
        let channels = channel_count.max(1) as usize;
        let mut result = Vec::with_capacity(frame_count * channels * 2);

        for _ in 0..frame_count {
            let sample_f32 = match pattern {
                MicPattern::SineTone(freq) => {
                    let sample = (*phase * 2.0 * PI).sin() * self.amplitude;
                    *phase += freq / (sample_rate as f32);
                    if *phase >= 1.0 {
                        *phase -= 1.0;
                    }
                    sample
                }
                MicPattern::VoiceFormant => {
                    let f0 = 220.0;
                    let f1 = 800.0;
                    let s0 = (*phase * 2.0 * PI).sin();
                    let s1 = (*phase * (f1 / f0) * 2.0 * PI).sin() * 0.4;
                    let sample = (s0 + s1) * 0.5 * self.amplitude;
                    *phase += f0 / (sample_rate as f32);
                    if *phase >= 1.0 {
                        *phase -= 1.0;
                    }
                    sample
                }
                MicPattern::WhiteNoise => {
                    // Simple deterministic LCG for white noise
                    let val = (((*phase * 12345.67).fract()) * 2.0 - 1.0) * self.amplitude;
                    *phase += 0.01;
                    if *phase >= 1.0 {
                        *phase -= 1.0;
                    }
                    val
                }
                MicPattern::Silence => 0.0,
            };

            let sample_i16 = (sample_f32.clamp(-1.0, 1.0) * 32767.0) as i16;
            let bytes = sample_i16.to_le_bytes();

            for _ in 0..channels {
                result.push(bytes[0]);
                result.push(bytes[1]);
            }
        }

        result
    }
}
