//! H.264 `.mp4` writing with AVFoundation (`AVAssetWriter`).

use std::path::Path;
use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAsset, AVAssetReader, AVAssetReaderTrackOutput, AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus, AVFileTypeMPEG4, AVMediaTypeAudio, AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey, AVVideoCodecTypeH264,
    AVVideoCompressionPropertiesKey, AVVideoHeightKey, AVVideoWidthKey,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferPool, CVPixelBufferUnlockBaseAddress,
    kCVPixelBufferHeightKey, kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey, kCVPixelFormatType_32BGRA, kCVReturnSuccess,
};
use objc2_foundation::{NSCopying, NSDictionary, NSNumber, NSString, NSURL};

/// Writes frames of RGBA pixels to an `.mp4` file.
pub struct Encoder {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    width: u32,
    height: u32,
    fps: i32,
    frame: i64,
    audio: Option<Audio>,
}

/// The mixed sound, read back from its WAV file and written as AAC.
struct Audio {
    input: Retained<AVAssetWriterInput>,
    _reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
    /// Read but not yet accepted by the writer.
    pending: Option<Retained<CMSampleBuffer>>,
    done: bool,
}

impl Audio {
    fn new(writer: &AVAssetWriter, wav: &Path) -> Result<Audio, String> {
        unsafe {
            let key = |k: &str| NSString::from_str(k);
            let (format, rate, channels, bitrate) = (key("AVFormatIDKey"), key("AVSampleRateKey"), key("AVNumberOfChannelsKey"), key("AVEncoderBitRateKey"));
            // 'aac ': AAC, 128 kbit/s, stereo.
            let settings = dict(&[(&format, number(0x6161_6320)), (&rate, number(crate::mix::RATE as i64)), (&channels, number(2)), (&bitrate, number(128_000))]);
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(AVMediaTypeAudio.ok_or("no audio media type")?, Some(&settings));
            input.setExpectsMediaDataInRealTime(false);
            if !writer.canAddInput(&input) {
                return Err("the video file cannot take sound".into());
            }
            writer.addInput(&input);
            let url = NSURL::fileURLWithPath(&NSString::from_str(&wav.to_string_lossy()));
            let asset = AVAsset::assetWithURL(&url);
            #[allow(deprecated)]
            let track = asset.tracksWithMediaType(AVMediaTypeAudio.ok_or("no audio media type")?).firstObject().ok_or("the mixed sound has no track")?;
            let reader = AVAssetReader::initWithAsset_error(AVAssetReader::alloc(), &asset).map_err(|e| err("cannot read the sound back", Some(e)))?;
            let (format, rate, channels, bits, float, big, non) = (key("AVFormatIDKey"), key("AVSampleRateKey"), key("AVNumberOfChannelsKey"), key("AVLinearPCMBitDepthKey"), key("AVLinearPCMIsFloatKey"), key("AVLinearPCMIsBigEndianKey"), key("AVLinearPCMIsNonInterleaved"));
            let read = dict(&[
                (&format, number(0x6C70_636D)),
                (&rate, number(crate::mix::RATE as i64)),
                (&channels, number(2)),
                (&bits, number(16)),
                (&float, number(0)),
                (&big, number(0)),
                (&non, number(0)),
            ]);
            let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, Some(&read));
            if !reader.canAddOutput(&output) {
                return Err("cannot read the sound back".into());
            }
            reader.addOutput(&output);
            Ok(Audio { input, _reader: reader, output, pending: None, done: false })
        }
    }

    /// Hands the writer sound up to `until_ms` (as far as it takes it right now).
    fn pump(&mut self, until_ms: f64) -> Result<(), String> {
        unsafe {
            while !self.done && self.input.isReadyForMoreMediaData() {
                let buffer = match self.pending.take() {
                    Some(b) => b,
                    None => match self.output.copyNextSampleBuffer() {
                        Some(b) => b,
                        None => {
                            self.input.markAsFinished();
                            self.done = true;
                            break;
                        }
                    },
                };
                let t = buffer.presentation_time_stamp();
                if t.timescale > 0 && t.value as f64 * 1000.0 / t.timescale as f64 > until_ms {
                    self.pending = Some(buffer);
                    break;
                }
                if !self.input.appendSampleBuffer(&buffer) {
                    return Err("cannot add sound to the video".into());
                }
            }
        }
        Ok(())
    }
}

fn err(what: &str, e: Option<Retained<objc2_foundation::NSError>>) -> String {
    match e {
        Some(e) => format!("{what}: {}", e.localizedDescription()),
        None => what.to_string(),
    }
}

/// `dict` entries as an `NSDictionary<NSString, AnyObject>`.
pub(crate) fn dict(entries: &[(&NSString, Retained<AnyObject>)]) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<&NSString> = entries.iter().map(|e| e.0).collect();
    let values: Vec<&AnyObject> = entries.iter().map(|e| &*e.1).collect();
    NSDictionary::from_slices(&keys, &values)
}

pub(crate) fn number(v: i64) -> Retained<AnyObject> {
    Retained::into_super(Retained::into_super(NSNumber::new_i64(v))).into()
}

impl Encoder {
    /// Starts a file at `path` (replaced if it exists) for `width` × `height` frames.
    /// `sound` is a WAV file (see `mix::wav`) to put on the audio track.
    pub fn new(path: &Path, width: u32, height: u32, fps: u32, sound: Option<&Path>) -> Result<Encoder, String> {
        let _ = std::fs::remove_file(path);
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        unsafe {
            let file_type = AVFileTypeMPEG4.ok_or("no MPEG-4 file type")?;
            let writer = AVAssetWriter::initWithURL_fileType_error(AVAssetWriter::alloc(), &url, file_type).map_err(|e| err("cannot create the video file", Some(e)))?;
            // About 0.1 bit per pixel per frame: crisp text, moderate size.
            let bitrate = (width as i64 * height as i64 * fps as i64) / 10;
            let compression = dict(&[(AVVideoAverageBitRateKey.ok_or("no bitrate key")?, number(bitrate))]);
            let settings = dict(&[
                (AVVideoCodecKey.ok_or("no codec key")?, Retained::into_super(Retained::into_super(AVVideoCodecTypeH264.ok_or("no H.264")?.copy()))),
                (AVVideoWidthKey.ok_or("no width key")?, number(width as i64)),
                (AVVideoHeightKey.ok_or("no height key")?, number(height as i64)),
                (AVVideoCompressionPropertiesKey.ok_or("no compression key")?, Retained::into_super(Retained::into_super(compression))),
            ]);
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(AVMediaTypeVideo.ok_or("no video media type")?, Some(&settings));
            input.setExpectsMediaDataInRealTime(false);
            let cf = |k: &objc2_core_foundation::CFString| -> &NSString { &*(k as *const _ as *const NSString) };
            let attrs = dict(&[
                (cf(kCVPixelBufferPixelFormatTypeKey), number(kCVPixelFormatType_32BGRA as i64)),
                (cf(kCVPixelBufferWidthKey), number(width as i64)),
                (cf(kCVPixelBufferHeightKey), number(height as i64)),
            ]);
            let adaptor = AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(&input, Some(&attrs));
            if !writer.canAddInput(&input) {
                return Err("the video file cannot take this input".into());
            }
            writer.addInput(&input);
            let mut audio = match sound {
                Some(wav) => Some(Audio::new(&writer, wav)?),
                None => None,
            };
            if !writer.startWriting() {
                return Err(err("cannot start writing the video", writer.error()));
            }
            writer.startSessionAtSourceTime(CMTime { value: 0, timescale: fps as i32, flags: CMTimeFlags::Valid, epoch: 0 });
            if let Some(a) = audio.as_mut() {
                // The reader may start only once the writer is going; it does not depend on it.
                if !a._reader.startReading() {
                    return Err("cannot read the sound back".into());
                }
            }
            Ok(Encoder { writer, input, adaptor, width, height, fps: fps as i32, frame: 0, audio })
        }
    }

    /// Appends one frame (`width` × `height` RGBA pixels, rows packed).
    pub fn push(&mut self, rgba: &[u8]) -> Result<(), String> {
        let (w, h) = (self.width as usize, self.height as usize);
        if rgba.len() != w * h * 4 {
            return Err("frame has the wrong size".into());
        }
        unsafe {
            // The encoder works asynchronously; wait until it takes more.
            let now_ms = self.frame as f64 * 1000.0 / self.fps as f64;
            if let Some(a) = self.audio.as_mut() {
                a.pump(now_ms + 1000.0)?;
            }
            while !self.input.isReadyForMoreMediaData() {
                if let Some(a) = self.audio.as_mut() {
                    a.pump(now_ms + 1000.0)?;
                }
                if self.writer.status() != AVAssetWriterStatus::Writing {
                    return Err(err("video encoding stopped", self.writer.error()));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            let pool: Retained<CVPixelBufferPool> = self.adaptor.pixelBufferPool().ok_or_else(|| err("no pixel buffers", self.writer.error()))?;
            let mut out: *mut CVPixelBuffer = std::ptr::null_mut();
            if CVPixelBufferPool::create_pixel_buffer(None, &pool, NonNull::from(&mut out)) != kCVReturnSuccess || out.is_null() {
                return Err("cannot get a pixel buffer".into());
            }
            let buffer: CFRetained<CVPixelBuffer> = CFRetained::from_raw(NonNull::new_unchecked(out));
            CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
            let base = CVPixelBufferGetBaseAddress(&buffer) as *mut u8;
            let stride = CVPixelBufferGetBytesPerRow(&buffer);
            for y in 0..h {
                let src = &rgba[y * w * 4..(y + 1) * w * 4];
                let dst = std::slice::from_raw_parts_mut(base.add(y * stride), w * 4);
                for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0) {
                    *d = [s[2], s[1], s[0], 255];
                }
            }
            CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
            let time = CMTime { value: self.frame, timescale: self.fps, flags: CMTimeFlags::Valid, epoch: 0 };
            if !self.adaptor.appendPixelBuffer_withPresentationTime(&buffer, time) {
                return Err(err("cannot add a frame to the video", self.writer.error()));
            }
        }
        self.frame += 1;
        Ok(())
    }

    /// Completes the file.
    pub fn finish(mut self) -> Result<(), String> {
        unsafe { self.input.markAsFinished() };
        // The rest of the sound, as fast as the writer takes it.
        if let Some(a) = self.audio.as_mut() {
            while !a.done {
                a.pump(f64::MAX)?;
                unsafe {
                    if self.writer.status() != AVAssetWriterStatus::Writing {
                        return Err(err("video encoding stopped", self.writer.error()));
                    }
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        let (tx, rx) = mpsc::channel();
        let done = RcBlock::new(move || {
            let _ = tx.send(());
        });
        unsafe {
            self.writer.finishWritingWithCompletionHandler(&done);
        }
        rx.recv_timeout(Duration::from_secs(120)).map_err(|_| "finishing the video timed out".to_string())?;
        unsafe {
            if self.writer.status() != AVAssetWriterStatus::Completed {
                return Err(err("the video could not be completed", self.writer.error()));
            }
        }
        Ok(())
    }

    /// Stops and removes the partial file.
    pub fn cancel(self) {
        unsafe { self.writer.cancelWriting() };
    }
}
