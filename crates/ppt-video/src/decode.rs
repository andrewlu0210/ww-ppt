//! Reads the sound of an audio or video file (AVFoundation) as PCM.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, Ordering};

use objc2::AnyThread;
use objc2_av_foundation::{AVAsset, AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVMediaTypeAudio};
use objc2_foundation::{NSString, NSURL};

use crate::encoder::{dict, number};
use crate::mix::RATE;

/// A file's length and its sound as interleaved stereo samples at [`RATE`].
pub struct Decoded {
    pub duration_ms: f64,
    /// Empty when the file has no sound track.
    pub pcm: Vec<f32>,
}

/// Decodes `bytes`; `part` (the part name) gives the extension AVFoundation goes by.
pub fn decode(bytes: &[u8], part: &str) -> Result<Decoded, String> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let ext = part.rsplit('.').next().filter(|e| e.len() <= 5).unwrap_or("mp4");
    let path = std::env::temp_dir().join(format!("want-point-media-{}-{}.{ext}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let out = decode_file(&path);
    let _ = std::fs::remove_file(&path);
    out
}

fn decode_file(path: &std::path::Path) -> Result<Decoded, String> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    unsafe {
        let asset = AVAsset::assetWithURL(&url);
        let d = asset.duration();
        let duration_ms = if d.timescale > 0 { d.value as f64 * 1000.0 / d.timescale as f64 } else { 0.0 };
        #[allow(deprecated)]
        let tracks = asset.tracksWithMediaType(AVMediaTypeAudio.ok_or("no audio media type")?);
        let Some(track) = tracks.firstObject() else { return Ok(Decoded { duration_ms, pcm: Vec::new() }) };
        let reader = AVAssetReader::initWithAsset_error(AVAssetReader::alloc(), &asset).map_err(|e| format!("cannot read the media: {}", e.localizedDescription()))?;
        // 16-bit stereo linear PCM ('lpcm'), whatever the file holds.
        let key = |k: &str| NSString::from_str(k);
        let (format, rate, channels, bits, no, ) = (key("AVFormatIDKey"), key("AVSampleRateKey"), key("AVNumberOfChannelsKey"), key("AVLinearPCMBitDepthKey"), number(0));
        let (float, big, non_interleaved) = (key("AVLinearPCMIsFloatKey"), key("AVLinearPCMIsBigEndianKey"), key("AVLinearPCMIsNonInterleaved"));
        let settings = dict(&[
            (&format, number(0x6C70_636D)),
            (&rate, number(RATE as i64)),
            (&channels, number(2)),
            (&bits, number(16)),
            (&float, no.clone()),
            (&big, no.clone()),
            (&non_interleaved, no),
        ]);
        let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, Some(&settings));
        if !reader.canAddOutput(&output) {
            return Err("the media cannot be decoded".into());
        }
        reader.addOutput(&output);
        if !reader.startReading() {
            return Err(format!("cannot read the media: {}", reader.error().map(|e| e.localizedDescription().to_string()).unwrap_or_default()));
        }
        let mut pcm = Vec::new();
        while let Some(buffer) = output.copyNextSampleBuffer() {
            let Some(block) = buffer.data_buffer() else { continue };
            let len = block.data_length();
            let mut bytes = vec![0u8; len];
            if block.copy_data_bytes(0, len, NonNull::new_unchecked(bytes.as_mut_ptr().cast())) != 0 {
                return Err("cannot read the media's sound".into());
            }
            pcm.extend(bytes.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0));
        }
        if reader.status() == AVAssetReaderStatus::Failed {
            return Err(format!("cannot decode the media: {}", reader.error().map(|e| e.localizedDescription().to_string()).unwrap_or_default()));
        }
        Ok(Decoded { duration_ms, pcm })
    }
}
