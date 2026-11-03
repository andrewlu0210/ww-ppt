//! Frames of a video clip (AVFoundation), drawn over the clip's picture on a slide.

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use objc2::rc::Retained;
use objc2_av_foundation::{AVAsset, AVAssetImageGenerator};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColorSpace, CGContext, CGImage};
use objc2_core_media::{CMTime, CMTimeFlags};
use objc2_foundation::{NSString, NSURL};

unsafe extern "C-unwind" {
    fn CGBitmapContextCreate(data: *mut c_void, width: usize, height: usize, bits_per_component: usize, bytes_per_row: usize, space: Option<&CGColorSpace>, bitmap_info: u32) -> Option<std::ptr::NonNull<CGContext>>;
}

/// A video file whose frames can be asked for by time.
pub struct VideoSource {
    generator: Retained<AVAssetImageGenerator>,
    file: PathBuf,
}

impl Drop for VideoSource {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

impl VideoSource {
    /// Opens video `bytes`; `part` (the part name) gives the extension AVFoundation goes by.
    pub fn open(bytes: &[u8], part: &str) -> Result<VideoSource, String> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let ext = part.rsplit('.').next().filter(|e| e.len() <= 5).unwrap_or("mp4");
        let file = std::env::temp_dir().join(format!("want-point-video-{}-{}.{ext}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
        std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
        unsafe {
            let asset = AVAsset::assetWithURL(&url);
            let generator = AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset);
            generator.setAppliesPreferredTrackTransform(true);
            // The frame at the time asked for, not the nearest key frame.
            let zero = CMTime { value: 0, timescale: 1, flags: CMTimeFlags::Valid, epoch: 0 };
            generator.setRequestedTimeToleranceBefore(zero);
            generator.setRequestedTimeToleranceAfter(zero);
            Ok(VideoSource { generator, file })
        }
    }

    /// The frame `ms` into the video.
    pub fn image_at(&self, ms: f64) -> Option<Retained<CGImage>> {
        let time = CMTime { value: ms.max(0.0).round() as i64, timescale: 1000, flags: CMTimeFlags::Valid, epoch: 0 };
        #[allow(deprecated)]
        unsafe {
            self.generator.copyCGImageAtTime_actualTime_error(time, std::ptr::null_mut()).ok()
        }
    }
}

/// Draws `image` into the `x`, `y`, `w`, `h` rectangle (pixels from the top left) of the
/// `width` × `height` RGBA `pixels`.
pub fn draw(pixels: &mut [u8], width: u32, height: u32, image: &CGImage, (x, y, w, h): (f64, f64, f64, f64)) {
    let Some(space) = CGColorSpace::new_device_rgb() else { return };
    // Premultiplied alpha, last: RGBA bytes.
    const PREMULTIPLIED_LAST: u32 = 1;
    unsafe {
        let Some(ctx) = CGBitmapContextCreate(pixels.as_mut_ptr().cast(), width as usize, height as usize, 8, width as usize * 4, Some(&space), PREMULTIPLIED_LAST) else { return };
        let ctx: CFRetained<CGContext> = CFRetained::from_raw(ctx);
        // The context's origin is at the bottom left.
        let rect = CGRect { origin: CGPoint { x, y: height as f64 - (y + h) }, size: CGSize { width: w, height: h } };
        CGContext::draw_image(Some(&ctx), rect, Some(image));
    }
}
