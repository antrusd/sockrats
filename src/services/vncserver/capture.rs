//! Screen capture module for the VNC server.
//!
//! Captures the primary monitor's screen at the configured frame rate and feeds
//! RGBA pixel data into the VNC [`Framebuffer`]. Uses the `xcap` crate for
//! cross-platform screen capture (X11, Wayland, Windows, macOS).
//!
//! # Architecture
//!
//! ```text
//! ScreenCapture::start()
//!       │
//!       ▼
//!   spawn_blocking(monitor.capture_image())
//!       │
//!       ▼
//!   RgbaImage → into_raw() → Vec<u8>
//!       │
//!       ▼
//!   framebuffer.update_cropped(pixels, 0, 0, w, h)
//! ```
//!
//! # Resolution Auto-Detection
//!
//! When the VNC server starts with screen capture enabled, the framebuffer is
//! automatically resized to match the primary monitor's resolution. If the
//! monitor resolution changes during capture, the framebuffer is resized
//! accordingly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use super::framebuffer::Framebuffer;

/// Screen capture controller.
///
/// Manages a background task that continuously captures the primary monitor's
/// screen and writes the pixel data into the VNC framebuffer.
#[derive(Debug)]
pub struct ScreenCapture {
    /// Shared framebuffer to write captured pixels into.
    framebuffer: Framebuffer,
    /// Maximum frames per second for capture rate limiting.
    max_fps: u8,
    /// Atomic flag to signal the capture loop to stop.
    running: Arc<AtomicBool>,
    /// Handle to the background capture task.
    task_handle: Option<JoinHandle<()>>,
}

/// Information about a detected monitor.
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Monitor name/identifier.
    pub name: String,
    /// Monitor width in pixels.
    pub width: u32,
    /// Monitor height in pixels.
    pub height: u32,
    /// Whether this is the primary monitor.
    pub is_primary: bool,
}

impl ScreenCapture {
    /// Creates a new [`ScreenCapture`] instance.
    ///
    /// # Arguments
    ///
    /// * `framebuffer` - The VNC framebuffer to write captured pixels into
    /// * `max_fps` - Maximum capture rate in frames per second (1-60)
    pub fn new(framebuffer: Framebuffer, max_fps: u8) -> Self {
        Self {
            framebuffer,
            max_fps: max_fps.clamp(1, 60),
            running: Arc::new(AtomicBool::new(false)),
            task_handle: None,
        }
    }

    /// Detects the primary monitor and returns its info.
    ///
    /// Scans all available monitors and returns the primary one.
    /// Falls back to the first monitor if no primary is detected.
    ///
    /// # Errors
    ///
    /// Returns an error string if no monitors are found.
    pub fn detect_primary_monitor() -> Result<MonitorInfo, String> {
        let monitors =
            xcap::Monitor::all().map_err(|e| format!("Failed to enumerate monitors: {e}"))?;

        if monitors.is_empty() {
            return Err("No monitors found".to_string());
        }

        // Find primary monitor, or fall back to first
        let monitor = monitors
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .unwrap_or(&monitors[0]);

        let name = monitor.name().unwrap_or_else(|_| "Unknown".to_string());
        let width = monitor.width().map_err(|e| format!("Failed to get width: {e}"))?;
        let height = monitor
            .height()
            .map_err(|e| format!("Failed to get height: {e}"))?;
        let is_primary = monitor.is_primary().unwrap_or(false);

        Ok(MonitorInfo {
            name,
            width,
            height,
            is_primary,
        })
    }

    /// Starts the screen capture loop.
    ///
    /// Auto-detects the primary monitor, resizes the framebuffer to match its
    /// resolution, then begins capturing frames in a background task.
    ///
    /// # Errors
    ///
    /// Returns an error if monitor detection fails or the framebuffer cannot
    /// be resized.
    pub async fn start(&mut self) -> Result<MonitorInfo, String> {
        if self.running.load(Ordering::Relaxed) {
            return Err("Screen capture is already running".to_string());
        }

        // Detect primary monitor
        let monitor_info = Self::detect_primary_monitor()?;

        info!(
            "Starting screen capture on monitor '{}' ({}x{}, primary={})",
            monitor_info.name, monitor_info.width, monitor_info.height, monitor_info.is_primary
        );

        // Resize framebuffer to match monitor resolution
        let width = monitor_info.width.min(8192) as u16;
        let height = monitor_info.height.min(8192) as u16;

        if self.framebuffer.width() != width || self.framebuffer.height() != height {
            self.framebuffer.resize(width, height).await.map_err(|e| {
                format!(
                    "Failed to resize framebuffer to {}x{}: {}",
                    width, height, e
                )
            })?;
            info!("Framebuffer resized to {}x{}", width, height);
        }

        // Start capture loop
        self.running.store(true, Ordering::Release);
        let running = Arc::clone(&self.running);
        let framebuffer = self.framebuffer.clone();
        let frame_interval = Duration::from_millis(1000 / u64::from(self.max_fps));

        let handle = tokio::spawn(async move {
            capture_loop(running, framebuffer, frame_interval).await;
        });

        self.task_handle = Some(handle);

        Ok(monitor_info)
    }

    /// Stops the screen capture loop.
    pub async fn stop(&mut self) {
        self.running.store(false, Ordering::Release);

        if let Some(handle) = self.task_handle.take() {
            // Give the task a moment to finish gracefully
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }

        info!("Screen capture stopped");
    }

    /// Returns whether the capture loop is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        // Task will be aborted when handle is dropped
    }
}

/// The main capture loop that runs in a background task.
///
/// Captures frames from the primary monitor at the configured rate and
/// writes them to the framebuffer.
async fn capture_loop(running: Arc<AtomicBool>, framebuffer: Framebuffer, frame_interval: Duration) {
    let mut interval = tokio::time::interval(frame_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut frame_count: u64 = 0;
    let mut consecutive_errors: u32 = 0;
    const MAX_CONSECUTIVE_ERRORS: u32 = 10;

    while running.load(Ordering::Relaxed) {
        interval.tick().await;

        // Capture in a blocking thread (xcap is synchronous)
        let capture_result = tokio::task::spawn_blocking(|| {
            capture_frame()
        })
        .await;

        match capture_result {
            Ok(Ok((pixels, width, height))) => {
                consecutive_errors = 0;

                // Check if resolution changed
                let fb_width = framebuffer.width();
                let fb_height = framebuffer.height();
                let cap_width = width.min(8192) as u16;
                let cap_height = height.min(8192) as u16;

                if fb_width != cap_width || fb_height != cap_height {
                    info!(
                        "Monitor resolution changed: {}x{} -> {}x{}",
                        fb_width, fb_height, cap_width, cap_height
                    );
                    if let Err(e) = framebuffer.resize(cap_width, cap_height).await {
                        error!("Failed to resize framebuffer: {}", e);
                        continue;
                    }
                }

                // Write captured pixels to framebuffer
                if let Err(e) = framebuffer
                    .update_cropped(&pixels, 0, 0, cap_width, cap_height)
                    .await
                {
                    warn!("Failed to update framebuffer: {}", e);
                }

                frame_count += 1;
                if frame_count % 300 == 0 {
                    debug!("Screen capture: {} frames captured", frame_count);
                }
            }
            Ok(Err(e)) => {
                consecutive_errors += 1;
                warn!(
                    "Screen capture error ({}/{}): {}",
                    consecutive_errors, MAX_CONSECUTIVE_ERRORS, e
                );

                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    error!(
                        "Too many consecutive capture errors ({}), stopping",
                        consecutive_errors
                    );
                    running.store(false, Ordering::Release);
                    break;
                }
            }
            Err(e) => {
                error!("Capture task panicked: {}", e);
                running.store(false, Ordering::Release);
                break;
            }
        }
    }

    info!(
        "Capture loop ended after {} frames",
        frame_count
    );
}

/// Captures a single frame from the primary monitor.
///
/// Returns (pixels, width, height) where pixels is RGBA32 data.
fn capture_frame() -> Result<(Vec<u8>, u32, u32), String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("Failed to list monitors: {e}"))?;

    if monitors.is_empty() {
        return Err("No monitors available".to_string());
    }

    let monitor = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .unwrap_or(&monitors[0]);

    let image = monitor
        .capture_image()
        .map_err(|e| format!("Failed to capture: {e}"))?;

    let width = image.width();
    let height = image.height();
    let pixels = image.into_raw(); // RGBA pixels

    Ok((pixels, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- MonitorInfo tests ---

    #[test]
    fn test_monitor_info_debug() {
        let info = MonitorInfo {
            name: "Test Monitor".to_string(),
            width: 1920,
            height: 1080,
            is_primary: true,
        };
        let debug = format!("{:?}", info);
        assert!(debug.contains("Test Monitor"));
        assert!(debug.contains("1920"));
        assert!(debug.contains("1080"));
    }

    #[test]
    fn test_monitor_info_clone() {
        let info = MonitorInfo {
            name: "Display".to_string(),
            width: 2560,
            height: 1440,
            is_primary: false,
        };
        let cloned = info.clone();
        assert_eq!(cloned.name, "Display");
        assert_eq!(cloned.width, 2560);
        assert_eq!(cloned.height, 1440);
        assert!(!cloned.is_primary);
    }

    // --- ScreenCapture construction tests ---

    #[test]
    fn test_screen_capture_new() {
        let fb = Framebuffer::new(800, 600);
        let capture = ScreenCapture::new(fb, 30);
        assert_eq!(capture.max_fps, 30);
        assert!(!capture.is_running());
    }

    #[test]
    fn test_screen_capture_fps_clamp_min() {
        let fb = Framebuffer::new(800, 600);
        let capture = ScreenCapture::new(fb, 0);
        assert_eq!(capture.max_fps, 1);
    }

    #[test]
    fn test_screen_capture_fps_clamp_max() {
        let fb = Framebuffer::new(800, 600);
        let capture = ScreenCapture::new(fb, 120);
        assert_eq!(capture.max_fps, 60);
    }

    #[test]
    fn test_screen_capture_fps_normal() {
        let fb = Framebuffer::new(800, 600);
        let capture = ScreenCapture::new(fb, 15);
        assert_eq!(capture.max_fps, 15);
    }

    #[test]
    fn test_screen_capture_is_not_running_initially() {
        let fb = Framebuffer::new(100, 100);
        let capture = ScreenCapture::new(fb, 30);
        assert!(!capture.is_running());
        assert!(capture.task_handle.is_none());
    }

    // --- Monitor detection tests (may not work in CI/headless) ---

    #[test]
    fn test_detect_primary_monitor() {
        match ScreenCapture::detect_primary_monitor() {
            Ok(info) => {
                assert!(info.width > 0);
                assert!(info.height > 0);
                assert!(!info.name.is_empty());
            }
            Err(e) => {
                eprintln!("No primary monitor (headless?): {}", e);
            }
        }
    }

    // --- Start/stop tests ---

    #[tokio::test]
    async fn test_screen_capture_start_stop() {
        let fb = Framebuffer::new(100, 100);
        let mut capture = ScreenCapture::new(fb, 5);

        match capture.start().await {
            Ok(info) => {
                assert!(capture.is_running());
                assert!(info.width > 0);
                assert!(info.height > 0);

                // Let it capture a few frames
                tokio::time::sleep(Duration::from_millis(500)).await;

                capture.stop().await;
                assert!(!capture.is_running());
            }
            Err(e) => {
                // Expected in headless environments
                eprintln!("Cannot start capture (headless?): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_screen_capture_double_start_errors() {
        let fb = Framebuffer::new(100, 100);
        let mut capture = ScreenCapture::new(fb, 5);

        match capture.start().await {
            Ok(_) => {
                // Second start should error
                let result = capture.start().await;
                assert!(result.is_err());
                assert!(result
                    .unwrap_err()
                    .contains("already running"));

                capture.stop().await;
            }
            Err(_) => {
                // Headless environment
            }
        }
    }

    #[tokio::test]
    async fn test_screen_capture_stop_when_not_running() {
        let fb = Framebuffer::new(100, 100);
        let mut capture = ScreenCapture::new(fb, 30);

        // Should not panic
        capture.stop().await;
        assert!(!capture.is_running());
    }

    #[tokio::test]
    async fn test_screen_capture_framebuffer_resize_on_start() {
        let fb = Framebuffer::new(100, 100);
        let fb_clone = fb.clone();
        let mut capture = ScreenCapture::new(fb, 5);

        match capture.start().await {
            Ok(info) => {
                // Framebuffer should have been resized to match monitor
                let expected_width = info.width.min(8192) as u16;
                let expected_height = info.height.min(8192) as u16;
                assert_eq!(fb_clone.width(), expected_width);
                assert_eq!(fb_clone.height(), expected_height);

                capture.stop().await;
            }
            Err(_) => {
                // Headless environment — fb should remain original size
                assert_eq!(fb_clone.width(), 100);
                assert_eq!(fb_clone.height(), 100);
            }
        }
    }

    // --- capture_frame tests ---

    #[test]
    fn test_capture_frame() {
        match capture_frame() {
            Ok((pixels, width, height)) => {
                assert!(width > 0);
                assert!(height > 0);
                // RGBA = 4 bytes per pixel
                assert_eq!(pixels.len(), (width as usize) * (height as usize) * 4);
            }
            Err(e) => {
                eprintln!("Cannot capture frame (headless?): {}", e);
            }
        }
    }

    // --- Drop test ---

    #[tokio::test]
    async fn test_screen_capture_drop_stops_running() {
        let fb = Framebuffer::new(100, 100);
        let running;
        {
            let mut capture = ScreenCapture::new(fb, 5);
            running = Arc::clone(&capture.running);

            if capture.start().await.is_ok() {
                assert!(running.load(Ordering::Relaxed));
            }
            // capture dropped here
        }
        // After drop, running should be false
        assert!(!running.load(Ordering::Relaxed));
    }
}
