//! VNC framebuffer management and dirty region tracking.
//!
//! This module provides the core framebuffer functionality for the VNC server, including:
//! - Pixel data storage and access
//! - Dirty region tracking for efficient updates
//! - Client notification system for framebuffer changes

use std::sync::atomic::{AtomicU16, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::sync::Weak;
use tokio::sync::RwLock;

/// Represents a rectangular region of the framebuffer that has been modified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRegion {
    /// The X coordinate of the top-left corner of the region.
    pub x: u16,
    /// The Y coordinate of the top-left corner of the region.
    pub y: u16,
    /// The width of the region.
    pub width: u16,
    /// The height of the region.
    pub height: u16,
}

impl DirtyRegion {
    /// Creates a new `DirtyRegion`.
    #[must_use]
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Merges this `DirtyRegion` with another, returning a new `DirtyRegion`
    /// that contains both.
    #[must_use]
    pub fn merge(&self, other: &DirtyRegion) -> DirtyRegion {
        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = self
            .x
            .saturating_add(self.width)
            .max(other.x.saturating_add(other.width));
        let y2 = self
            .y
            .saturating_add(self.height)
            .max(other.y.saturating_add(other.height));

        DirtyRegion {
            x: x1,
            y: y1,
            width: x2.saturating_sub(x1),
            height: y2.saturating_sub(y1),
        }
    }

    /// Checks if this `DirtyRegion` intersects with another.
    #[must_use]
    pub fn intersects(&self, other: &DirtyRegion) -> bool {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self
            .x
            .saturating_add(self.width)
            .min(other.x.saturating_add(other.width));
        let y2 = self
            .y
            .saturating_add(self.height)
            .min(other.y.saturating_add(other.height));

        x1 < x2 && y1 < y2
    }

    /// Computes the intersection of two `DirtyRegion`s.
    #[must_use]
    pub fn intersect(&self, other: &DirtyRegion) -> Option<DirtyRegion> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self
            .x
            .saturating_add(self.width)
            .min(other.x.saturating_add(other.width));
        let y2 = self
            .y
            .saturating_add(self.height)
            .min(other.y.saturating_add(other.height));

        if x1 < x2 && y1 < y2 {
            Some(DirtyRegion {
                x: x1,
                y: y1,
                width: x2.saturating_sub(x1),
                height: y2.saturating_sub(y1),
            })
        } else {
            None
        }
    }
}

/// A struct for receiving notifications about dirty (modified) regions in the framebuffer.
///
/// Uses a `Weak` reference to the client's `modified_regions` to allow for a
/// push-based update model.
#[derive(Clone, Debug)]
pub struct DirtyRegionReceiver {
    /// A `Weak` reference to a `RwLock`-protected vector of `DirtyRegion`s.
    regions: Weak<RwLock<Vec<DirtyRegion>>>,
}

impl DirtyRegionReceiver {
    /// Creates a new `DirtyRegionReceiver`.
    #[must_use]
    pub fn new(regions: Weak<RwLock<Vec<DirtyRegion>>>) -> Self {
        Self { regions }
    }

    /// Adds a new dirty region to the receiver's list.
    ///
    /// Merges the new region with any existing intersecting regions
    /// and enforces memory limits.
    pub async fn add_dirty_region(&self, region: DirtyRegion) {
        const MAX_REGIONS: usize = 10;
        const MAX_TOTAL_PIXELS: usize = 1920 * 1080 * 2;

        if let Some(regions_arc) = self.regions.upgrade() {
            let mut regions = regions_arc.write().await;

            // Merge with ALL intersecting regions
            let mut merged_region = region;
            regions.retain(|existing| {
                if existing.intersects(&merged_region) {
                    merged_region = existing.merge(&merged_region);
                    false
                } else {
                    true
                }
            });

            regions.push(merged_region);

            let total_pixels: usize = regions
                .iter()
                .map(|r| (r.width as usize) * (r.height as usize))
                .sum();

            if regions.len() > MAX_REGIONS || total_pixels > MAX_TOTAL_PIXELS {
                if let Some(first) = regions.first().copied() {
                    let merged = regions.iter().skip(1).fold(first, |acc, r| acc.merge(r));
                    regions.clear();
                    regions.push(merged);
                }
            }
        }
    }
}

/// Represents the VNC server's framebuffer.
///
/// Manages the pixel data of the remote screen, tracks dirty regions,
/// and notifies connected clients of updates.
#[derive(Clone, Debug)]
pub struct Framebuffer {
    /// The width of the framebuffer in pixels.
    width: Arc<AtomicU16>,
    /// The height of the framebuffer in pixels.
    height: Arc<AtomicU16>,
    /// The raw pixel data (RGBA32).
    data: Arc<RwLock<Vec<u8>>>,
    /// List of receivers to notify on changes.
    receivers: Arc<RwLock<Vec<DirtyRegionReceiver>>>,
}

impl Framebuffer {
    /// Creates a new `Framebuffer` with the given dimensions.
    #[must_use]
    pub fn new(width: u16, height: u16) -> Self {
        let size = (width as usize) * (height as usize) * 4; // RGBA32
        Self {
            width: Arc::new(AtomicU16::new(width)),
            height: Arc::new(AtomicU16::new(height)),
            data: Arc::new(RwLock::new(vec![0; size])),
            receivers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Registers a `DirtyRegionReceiver` to be notified of framebuffer updates.
    pub async fn register_receiver(&self, receiver: DirtyRegionReceiver) {
        let mut receivers = self.receivers.write().await;
        receivers.push(receiver);
    }

    /// Removes dead `Weak` references from the list of receivers.
    async fn cleanup_receivers(&self) {
        let mut receivers = self.receivers.write().await;
        receivers.retain(|r| r.regions.strong_count() > 0);
    }

    /// Marks a rectangular region as dirty and notifies all registered receivers.
    pub async fn mark_dirty_region(&self, x: u16, y: u16, width: u16, height: u16) {
        let region = DirtyRegion::new(x, y, width, height);

        let receivers_copy = {
            let receivers = self.receivers.read().await;
            receivers.clone()
        };

        for receiver in &receivers_copy {
            receiver.add_dirty_region(region).await;
        }

        self.cleanup_receivers().await;
    }

    /// Returns the width of the framebuffer.
    #[must_use]
    pub fn width(&self) -> u16 {
        self.width.load(AtomicOrdering::Relaxed)
    }

    /// Returns the height of the framebuffer.
    #[must_use]
    pub fn height(&self) -> u16 {
        self.height.load(AtomicOrdering::Relaxed)
    }

    /// Updates a specified cropped region of the framebuffer with new data.
    #[allow(clippy::cast_possible_truncation)]
    pub async fn update_cropped(
        &self,
        data: &[u8],
        crop_x: u16,
        crop_y: u16,
        crop_width: u16,
        crop_height: u16,
    ) -> Result<(), String> {
        // Validate crop region
        if crop_x.saturating_add(crop_width) > self.width() {
            return Err(format!(
                "Crop region exceeds framebuffer width: {}+{} > {}",
                crop_x,
                crop_width,
                self.width()
            ));
        }
        if crop_y.saturating_add(crop_height) > self.height() {
            return Err(format!(
                "Crop region exceeds framebuffer height: {}+{} > {}",
                crop_y,
                crop_height,
                self.height()
            ));
        }

        let expected_size = (crop_width as usize) * (crop_height as usize) * 4;
        if data.len() != expected_size {
            return Err(format!(
                "Invalid crop data size: expected {}, got {}",
                expected_size,
                data.len()
            ));
        }

        let mut fb = self.data.write().await;

        let mut changed = false;
        let mut min_x = u16::MAX;
        let mut min_y = u16::MAX;
        let mut max_x = 0u16;
        let mut max_y = 0u16;
        let crop_width_usize = crop_width as usize;
        let frame_width_usize = self.width() as usize;

        for y in 0..crop_height {
            let src_offset = (y as usize) * crop_width_usize * 4;
            let dst_offset = ((crop_y + y) as usize * frame_width_usize + crop_x as usize) * 4;
            let src_row = &data[src_offset..src_offset + crop_width_usize * 4];
            let dst_row = &fb[dst_offset..dst_offset + crop_width_usize * 4];

            if src_row != dst_row {
                let abs_y = crop_y + y;
                min_y = min_y.min(abs_y);
                max_y = max_y.max(abs_y);

                for x in 0..crop_width {
                    let px_offset = x as usize * 4;
                    if src_row[px_offset..px_offset + 4] != dst_row[px_offset..px_offset + 4] {
                        let abs_x = crop_x + x;
                        min_x = min_x.min(abs_x);
                        max_x = max_x.max(abs_x);
                    }
                }

                fb[dst_offset..dst_offset + crop_width_usize * 4].copy_from_slice(src_row);
                changed = true;
            }
        }

        if changed {
            let width = (max_x - min_x + 1).min(self.width() - min_x);
            let height = (max_y - min_y + 1).min(self.height() - min_y);
            drop(fb);
            self.mark_dirty_region(min_x, min_y, width, height).await;
        }

        Ok(())
    }

    /// Retrieves the pixel data for a specific rectangular region.
    pub async fn get_rect(
        &self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Result<Vec<u8>, String> {
        if x.saturating_add(width) > self.width() || y.saturating_add(height) > self.height() {
            return Err(format!(
                "Rectangle out of bounds: ({}, {}, {}, {}) exceeds ({}, {})",
                x,
                y,
                width,
                height,
                self.width(),
                self.height()
            ));
        }

        let data = self.data.read().await;
        let mut result = Vec::with_capacity((width as usize) * (height as usize) * 4);

        for row in y..(y + height) {
            let start = ((row as usize) * (self.width() as usize) + (x as usize)) * 4;
            let end = start + (width as usize) * 4;
            result.extend_from_slice(&data[start..end]);
        }

        Ok(result)
    }

    /// Returns a copy of the entire framebuffer's pixel data.
    #[allow(dead_code)]
    pub async fn get_full_data(&self) -> Vec<u8> {
        self.data.read().await.clone()
    }

    /// Resizes the framebuffer to new dimensions.
    pub async fn resize(&self, new_width: u16, new_height: u16) -> Result<(), String> {
        const MAX_DIMENSION: u16 = 8192;

        if new_width == 0 || new_height == 0 {
            return Err("Framebuffer dimensions must be greater than zero".to_string());
        }

        if new_width > MAX_DIMENSION || new_height > MAX_DIMENSION {
            return Err(format!(
                "Framebuffer dimensions too large: {new_width}x{new_height} (max: {MAX_DIMENSION})"
            ));
        }

        let old_width = self.width();
        let old_height = self.height();

        if new_width == old_width && new_height == old_height {
            return Ok(());
        }

        let new_size = (new_width as usize) * (new_height as usize) * 4;
        let mut new_data = vec![0u8; new_size];

        {
            let old_data = self.data.read().await;
            let copy_width = old_width.min(new_width) as usize;
            let copy_height = old_height.min(new_height) as usize;

            for y in 0..copy_height {
                let old_offset = y * (old_width as usize) * 4;
                let new_offset = y * (new_width as usize) * 4;
                let len = copy_width * 4;
                new_data[new_offset..new_offset + len]
                    .copy_from_slice(&old_data[old_offset..old_offset + len]);
            }
        }

        {
            let mut data = self.data.write().await;
            *data = new_data;
        }

        self.width.store(new_width, AtomicOrdering::Release);
        self.height.store(new_height, AtomicOrdering::Release);

        self.mark_dirty_region(0, 0, new_width, new_height).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- DirtyRegion tests ---

    #[test]
    fn test_dirty_region_new() {
        let r = DirtyRegion::new(10, 20, 100, 200);
        assert_eq!(r.x, 10);
        assert_eq!(r.y, 20);
        assert_eq!(r.width, 100);
        assert_eq!(r.height, 200);
    }

    #[test]
    fn test_dirty_region_merge() {
        let r1 = DirtyRegion::new(0, 0, 50, 50);
        let r2 = DirtyRegion::new(25, 25, 50, 50);
        let merged = r1.merge(&r2);
        assert_eq!(merged.x, 0);
        assert_eq!(merged.y, 0);
        assert_eq!(merged.width, 75);
        assert_eq!(merged.height, 75);
    }

    #[test]
    fn test_dirty_region_merge_non_overlapping() {
        let r1 = DirtyRegion::new(0, 0, 10, 10);
        let r2 = DirtyRegion::new(20, 20, 10, 10);
        let merged = r1.merge(&r2);
        assert_eq!(merged.x, 0);
        assert_eq!(merged.y, 0);
        assert_eq!(merged.width, 30);
        assert_eq!(merged.height, 30);
    }

    #[test]
    fn test_dirty_region_intersects() {
        let r1 = DirtyRegion::new(0, 0, 50, 50);
        let r2 = DirtyRegion::new(25, 25, 50, 50);
        assert!(r1.intersects(&r2));
        assert!(r2.intersects(&r1));
    }

    #[test]
    fn test_dirty_region_not_intersects() {
        let r1 = DirtyRegion::new(0, 0, 10, 10);
        let r2 = DirtyRegion::new(20, 20, 10, 10);
        assert!(!r1.intersects(&r2));
    }

    #[test]
    fn test_dirty_region_intersect_some() {
        let r1 = DirtyRegion::new(0, 0, 50, 50);
        let r2 = DirtyRegion::new(25, 25, 50, 50);
        let intersection = r1.intersect(&r2);
        assert!(intersection.is_some());
        let i = intersection.unwrap();
        assert_eq!(i.x, 25);
        assert_eq!(i.y, 25);
        assert_eq!(i.width, 25);
        assert_eq!(i.height, 25);
    }

    #[test]
    fn test_dirty_region_intersect_none() {
        let r1 = DirtyRegion::new(0, 0, 10, 10);
        let r2 = DirtyRegion::new(20, 20, 10, 10);
        assert!(r1.intersect(&r2).is_none());
    }

    #[test]
    fn test_dirty_region_intersect_adjacent() {
        // Touching but not overlapping
        let r1 = DirtyRegion::new(0, 0, 10, 10);
        let r2 = DirtyRegion::new(10, 0, 10, 10);
        assert!(!r1.intersects(&r2));
        assert!(r1.intersect(&r2).is_none());
    }

    // --- Framebuffer tests ---

    #[tokio::test]
    async fn test_framebuffer_new() {
        let fb = Framebuffer::new(800, 600);
        assert_eq!(fb.width(), 800);
        assert_eq!(fb.height(), 600);
    }

    #[tokio::test]
    async fn test_framebuffer_get_rect() {
        let fb = Framebuffer::new(10, 10);
        let data = fb.get_rect(0, 0, 10, 10).await.unwrap();
        assert_eq!(data.len(), 10 * 10 * 4);
        // Initial data is all zeros
        assert!(data.iter().all(|&b| b == 0));
    }

    #[tokio::test]
    async fn test_framebuffer_get_rect_out_of_bounds() {
        let fb = Framebuffer::new(10, 10);
        let result = fb.get_rect(5, 5, 10, 10).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_framebuffer_update_cropped() {
        let fb = Framebuffer::new(10, 10);

        // Fill a 5x5 region with red pixels
        let red_pixels = vec![255u8, 0, 0, 255].repeat(5 * 5);
        fb.update_cropped(&red_pixels, 0, 0, 5, 5).await.unwrap();

        // Verify the updated region
        let data = fb.get_rect(0, 0, 5, 5).await.unwrap();
        for chunk in data.chunks_exact(4) {
            assert_eq!(chunk, &[255, 0, 0, 255]);
        }

        // Verify untouched region is still zeros
        let data = fb.get_rect(5, 5, 5, 5).await.unwrap();
        assert!(data.iter().all(|&b| b == 0));
    }

    #[tokio::test]
    async fn test_framebuffer_update_cropped_invalid_size() {
        let fb = Framebuffer::new(10, 10);
        let result = fb.update_cropped(&[0; 10], 0, 0, 5, 5).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_framebuffer_update_cropped_out_of_bounds() {
        let fb = Framebuffer::new(10, 10);
        let pixels = vec![0u8; 10 * 10 * 4];
        let result = fb.update_cropped(&pixels, 5, 5, 10, 10).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_framebuffer_resize() {
        let fb = Framebuffer::new(10, 10);

        // Fill with data
        let pixels = vec![255u8; 10 * 10 * 4];
        fb.update_cropped(&pixels, 0, 0, 10, 10).await.unwrap();

        // Resize larger
        fb.resize(20, 20).await.unwrap();
        assert_eq!(fb.width(), 20);
        assert_eq!(fb.height(), 20);

        // Original data should be preserved in top-left
        let data = fb.get_rect(0, 0, 10, 10).await.unwrap();
        assert!(data.iter().all(|&b| b == 255));

        // New area should be zeros
        let data = fb.get_rect(10, 10, 10, 10).await.unwrap();
        assert!(data.iter().all(|&b| b == 0));
    }

    #[tokio::test]
    async fn test_framebuffer_resize_smaller() {
        let fb = Framebuffer::new(20, 20);
        fb.resize(10, 10).await.unwrap();
        assert_eq!(fb.width(), 10);
        assert_eq!(fb.height(), 10);
    }

    #[tokio::test]
    async fn test_framebuffer_resize_same() {
        let fb = Framebuffer::new(10, 10);
        fb.resize(10, 10).await.unwrap();
        assert_eq!(fb.width(), 10);
    }

    #[tokio::test]
    async fn test_framebuffer_resize_zero() {
        let fb = Framebuffer::new(10, 10);
        let result = fb.resize(0, 10).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_framebuffer_resize_too_large() {
        let fb = Framebuffer::new(10, 10);
        let result = fb.resize(9000, 9000).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dirty_region_receiver_notification() {
        let fb = Framebuffer::new(100, 100);

        // Create a receiver
        let regions = Arc::new(RwLock::new(Vec::new()));
        let receiver = DirtyRegionReceiver::new(Arc::downgrade(&regions));
        fb.register_receiver(receiver).await;

        // Update framebuffer
        let pixels = vec![255u8; 10 * 10 * 4];
        fb.update_cropped(&pixels, 5, 5, 10, 10).await.unwrap();

        // Check that the receiver got the dirty region
        let dirty = regions.read().await;
        assert!(!dirty.is_empty());
    }

    #[tokio::test]
    async fn test_dirty_region_receiver_dropped() {
        let fb = Framebuffer::new(100, 100);

        // Create and drop a receiver
        {
            let regions = Arc::new(RwLock::new(Vec::new()));
            let receiver = DirtyRegionReceiver::new(Arc::downgrade(&regions));
            fb.register_receiver(receiver).await;
        }
        // regions dropped here

        // Should not panic when marking dirty with dead receivers
        fb.mark_dirty_region(0, 0, 10, 10).await;
    }

    #[tokio::test]
    async fn test_dirty_region_receiver_merge_on_overflow() {
        let regions = Arc::new(RwLock::new(Vec::new()));
        let receiver = DirtyRegionReceiver::new(Arc::downgrade(&regions));

        // Add more than MAX_REGIONS non-overlapping regions
        for i in 0..15 {
            let x = (i * 10) % 150;
            receiver
                .add_dirty_region(DirtyRegion::new(x, 0, 5, 5))
                .await;
        }

        // Should have been merged down due to limit
        let dirty = regions.read().await;
        assert!(dirty.len() <= 10);
    }

    #[tokio::test]
    async fn test_framebuffer_clone() {
        let fb = Framebuffer::new(10, 10);
        let fb2 = fb.clone();

        // Both should share the same data
        assert_eq!(fb.width(), fb2.width());
        assert_eq!(fb.height(), fb2.height());

        // Updating one should be visible in the other
        let pixels = vec![255u8; 10 * 10 * 4];
        fb.update_cropped(&pixels, 0, 0, 10, 10).await.unwrap();

        let data = fb2.get_rect(0, 0, 10, 10).await.unwrap();
        assert!(data.iter().all(|&b| b == 255));
    }
}
