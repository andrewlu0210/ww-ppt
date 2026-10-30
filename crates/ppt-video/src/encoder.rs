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
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus, AVFileTypeMPEG4, AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey, AVVideoCodecTypeH264,
    AVVideoCompressionPropertiesKey, AVVideoHeightKey, AVVideoWidthKey,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{CMTime, CMTimeFlags};
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
}

fn err(what: &str, e: Option<Retained<objc2_foundation::NSError>>) -> String {
    match e {
        Some(e) => format!("{what}: {}", e.localizedDescription()),
        None => what.to_string(),
    }
}

/// `dict` entries as an `NSDictionary<NSString, AnyObject>`.
fn dict(entries: &[(&NSString, Retained<AnyObject>)]) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<&NSString> = entries.iter().map(|e| e.0).collect();
    let values: Vec<&AnyObject> = entries.iter().map(|e| &*e.1).collect();
    NSDictionary::from_slices(&keys, &values)
}

fn number(v: i64) -> Retained<AnyObject> {
    Retained::into_super(Retained::into_super(NSNumber::new_i64(v))).into()
}

impl Encoder {
    /// Starts a file at `path` (replaced if it exists) for `width` × `height` frames.
    pub fn new(path: &Path, width: u32, height: u32, fps: u32) -> Result<Encoder, String> {
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
            if !writer.startWriting() {
                return Err(err("cannot start writing the video", writer.error()));
            }
            writer.startSessionAtSourceTime(CMTime { value: 0, timescale: fps as i32, flags: CMTimeFlags::Valid, epoch: 0 });
            Ok(Encoder { writer, input, adaptor, width, height, fps: fps as i32, frame: 0 })
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
            while !self.input.isReadyForMoreMediaData() {
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
    pub fn finish(self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        let done = RcBlock::new(move || {
            let _ = tx.send(());
        });
        unsafe {
            self.input.markAsFinished();
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
