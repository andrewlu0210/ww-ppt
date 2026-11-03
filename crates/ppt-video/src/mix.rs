//! Mixes the clips' sound into one track on the video's timeline.

use std::sync::Arc;

/// Samples per second of the mixed track, which is stereo.
pub const RATE: u32 = 44_100;

/// One clip's sound placed on the timeline.
pub struct Track {
    /// Interleaved stereo samples at [`RATE`].
    pub pcm: Arc<Vec<f32>>,
    /// The part of `pcm` that plays (after trimming), in milliseconds.
    pub from_ms: f64,
    pub to_ms: f64,
    /// When it starts in the video, and the latest it may play until.
    pub at_ms: f64,
    pub until_ms: f64,
    pub volume: f32,
    pub repeat: bool,
}

fn frame_of(ms: f64) -> usize {
    (ms.max(0.0) * RATE as f64 / 1000.0).round() as usize
}

/// `total_ms` of interleaved stereo samples with every track added in.
pub fn mix(tracks: &[Track], total_ms: f64) -> Vec<f32> {
    let frames = frame_of(total_ms);
    let mut out = vec![0.0f32; frames * 2];
    for t in tracks {
        let total_src = t.pcm.len() / 2;
        let from = frame_of(t.from_ms).min(total_src);
        let to = frame_of(t.to_ms).clamp(from, total_src);
        if to == from {
            continue;
        }
        let start = frame_of(t.at_ms);
        let end = frame_of(t.until_ms).min(frames);
        let mut src = from;
        for f in start..end {
            if src >= to {
                if !t.repeat {
                    break;
                }
                src = from;
            }
            out[f * 2] += t.pcm[src * 2] * t.volume;
            out[f * 2 + 1] += t.pcm[src * 2 + 1] * t.volume;
            src += 1;
        }
    }
    for s in &mut out {
        *s = s.clamp(-1.0, 1.0);
    }
    out
}

/// A 16-bit stereo WAV file of `samples`, which the encoder reads back.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data as u32).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(ms: f64, v: f32) -> Arc<Vec<f32>> {
        Arc::new(vec![v; frame_of(ms) * 2])
    }

    fn track(pcm: Arc<Vec<f32>>, at_ms: f64, until_ms: f64, repeat: bool) -> Track {
        Track { from_ms: 0.0, to_ms: 1e9, pcm, at_ms, until_ms, volume: 1.0, repeat }
    }

    #[test]
    fn plays_once_from_its_start_time() {
        let out = mix(&[track(constant(100.0, 0.5), 200.0, 1000.0, false)], 1000.0);
        assert_eq!(out.len(), frame_of(1000.0) * 2);
        assert_eq!(out[frame_of(100.0) * 2], 0.0);
        assert_eq!(out[frame_of(250.0) * 2], 0.5);
        assert_eq!(out[frame_of(350.0) * 2], 0.0);
    }

    #[test]
    fn loops_until_its_limit_and_clips_the_sum() {
        let loud = track(constant(50.0, 0.8), 0.0, 120.0, true);
        let also = track(constant(50.0, 0.8), 0.0, 120.0, false);
        let out = mix(&[loud, also], 200.0);
        assert_eq!(out[frame_of(10.0) * 2], 1.0);
        // The loop is still going at 110 ms, the single play is over.
        assert_eq!(out[frame_of(110.0) * 2], 0.8);
        assert_eq!(out[frame_of(150.0) * 2], 0.0);
    }

    #[test]
    fn trims_and_scales() {
        let mut pcm = vec![0.0f32; frame_of(100.0) * 2];
        pcm[frame_of(60.0) * 2..].fill(1.0);
        let t = Track { pcm: Arc::new(pcm), from_ms: 50.0, to_ms: 100.0, at_ms: 0.0, until_ms: 100.0, volume: 0.5, repeat: false };
        let out = mix(&[t], 100.0);
        assert_eq!(out[0], 0.0);
        assert_eq!(out[frame_of(20.0) * 2], 0.5);
        assert_eq!(out[frame_of(60.0) * 2], 0.0);
    }

    #[test]
    fn wav_header_is_sized() {
        let w = wav(&[0.0, 1.0, -1.0, 0.0]);
        assert_eq!(w.len(), 44 + 8);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(i16::from_le_bytes([w[46], w[47]]), 32767);
    }
}
