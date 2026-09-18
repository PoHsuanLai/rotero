//! Page-pixel vs display-pixel mapping for the PDF overlay.
//!
//! The rendered PNG is `bitmap_w × bitmap_h` at `render_zoom = zoom * dpr`.
//! The wrapper is laid out at `bitmap * (zoom / render_zoom)` CSS pixels so
//! the page is sharp on HiDPI without using CSS `zoom` (which put `offsetX`
//! in a different space than overlay `left`/`top`).
//!
//! - **Page pixels** — bitmap / pdfrum selection / stored annotation geometry.
//! - **Display pixels** — CSS layout of the wrapper, `element_coordinates()`.
//! - **Viewport pixels** — `client_coordinates()`, menus, citation card.

/// Scale from page pixels to display CSS pixels: `zoom / render_zoom`.
pub fn display_scale(zoom: f32, render_zoom: f32) -> f64 {
    if render_zoom > 0.0 {
        f64::from(zoom) / f64::from(render_zoom)
    } else {
        1.0
    }
}

/// Size and scale of one on-screen page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSpace {
    pub bitmap_w: u32,
    pub bitmap_h: u32,
    pub display_scale: f64,
}

impl PageSpace {
    pub fn from_zooms(bitmap_w: u32, bitmap_h: u32, zoom: f32, render_zoom: f32) -> Self {
        Self {
            bitmap_w,
            bitmap_h,
            display_scale: display_scale(zoom, render_zoom),
        }
    }

    pub fn display_w(self) -> f64 {
        f64::from(self.bitmap_w) * self.display_scale.max(0.0)
    }

    pub fn display_h(self) -> f64 {
        f64::from(self.bitmap_h) * self.display_scale.max(0.0)
    }

    /// Mouse offset inside the wrapper (display CSS px) → page/bitmap pixels.
    pub fn overlay_to_page(self, x: f64, y: f64) -> (f64, f64) {
        let z = self.display_scale;
        if z <= 0.0 { (x, y) } else { (x / z, y / z) }
    }

    pub fn page_to_display(self, x: f64, y: f64) -> (f64, f64) {
        (x * self.display_scale, y * self.display_scale)
    }

    pub fn page_rect_to_display(self, x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
        let (x, y) = self.page_to_display(x, y);
        let (w, h) = self.page_to_display(w, h);
        (x, y, w, h)
    }

    /// Geometry stored against a (possibly older) bitmap size → current display CSS px.
    ///
    /// Missing `stored_w`/`stored_h` is treated as "already in the current bitmap".
    pub fn stored_to_display_x(self, x: f64, stored_w: f64) -> f64 {
        self.stored_axis(x, stored_w, self.bitmap_w)
    }

    pub fn stored_to_display_y(self, y: f64, stored_h: f64) -> f64 {
        self.stored_axis(y, stored_h, self.bitmap_h)
    }

    pub fn stored_rect_to_display(
        self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        stored_w: f64,
        stored_h: f64,
    ) -> (f64, f64, f64, f64) {
        (
            self.stored_to_display_x(x, stored_w),
            self.stored_to_display_y(y, stored_h),
            self.stored_to_display_x(w, stored_w),
            self.stored_to_display_y(h, stored_h),
        )
    }

    fn stored_axis(self, v: f64, stored: f64, current_bitmap: u32) -> f64 {
        if stored > 0.0 {
            v / stored * f64::from(current_bitmap) * self.display_scale
        } else {
            v * self.display_scale
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retina_page() -> PageSpace {
        // 612pt page at zoom 1.5, dpr 2 → bitmap 1836, display scale 0.5.
        PageSpace::from_zooms(1836, 2376, 1.5, 3.0)
    }

    #[test]
    fn retina_display_scale_is_inverse_dpr() {
        let s = retina_page();
        assert!((s.display_scale - 0.5).abs() < 1e-9);
        assert!((s.display_w() - 918.0).abs() < 1e-9);
    }

    #[test]
    fn one_x_display_matches_bitmap() {
        let s = PageSpace::from_zooms(918, 1188, 1.5, 1.5);
        assert!((s.display_scale - 1.0).abs() < 1e-9);
        assert!((s.display_w() - 918.0).abs() < 1e-9);
        let (x, y) = s.overlay_to_page(100.0, 200.0);
        assert!((x - 100.0).abs() < 1e-9 && (y - 200.0).abs() < 1e-9);
    }

    #[test]
    fn retina_click_is_doubled_into_page_pixels() {
        let s = retina_page();
        // Click at the visual centre of the displayed page.
        let (x, y) = s.overlay_to_page(s.display_w() / 2.0, s.display_h() / 2.0);
        assert!((x - 918.0).abs() < 1e-6);
        assert!((y - 1188.0).abs() < 1e-6);
    }

    #[test]
    fn overlay_to_page_inverts_page_to_display() {
        let s = retina_page();
        let (dx, dy) = s.page_to_display(400.0, 800.0);
        let (px, py) = s.overlay_to_page(dx, dy);
        assert!((px - 400.0).abs() < 1e-9 && (py - 800.0).abs() < 1e-9);
    }

    #[test]
    fn live_zoom_before_rerender_uses_current_scale() {
        // User hit '+' : zoom 2.1, bitmap still at render_zoom 3.0.
        let s = PageSpace::from_zooms(1836, 2376, 2.1, 3.0);
        assert!(s.display_scale > 0.69 && s.display_scale < 0.71);
        let (x, y) = s.overlay_to_page(s.display_w() / 2.0, s.display_h() / 2.0);
        assert!((x - 918.0).abs() < 1e-6);
        assert!((y - 1188.0).abs() < 1e-6);
    }

    #[test]
    fn stored_geometry_tracks_current_bitmap_and_display() {
        let s = retina_page();
        // Highlight saved at this same bitmap size.
        let (x, y, w, h) = s.stored_rect_to_display(200.0, 400.0, 40.0, 20.0, 1836.0, 2376.0);
        assert!((x - 100.0).abs() < 1e-9);
        assert!((y - 200.0).abs() < 1e-9);
        assert!((w - 20.0).abs() < 1e-9);
        assert!((h - 10.0).abs() < 1e-9);
    }

    #[test]
    fn stored_geometry_rescales_after_rerender_at_new_zoom() {
        // Saved at zoom 1.5 / dpr 2 (bitmap 1836), now showing zoom 2.0 / dpr 2
        // (bitmap 2448) with display scale 0.5.
        let s = PageSpace::from_zooms(2448, 3168, 2.0, 4.0);
        let (x, _, w, _) = s.stored_rect_to_display(918.0, 0.0, 183.6, 10.0, 1836.0, 2376.0);
        // 918/1836 of the page, displayed 2448*0.5 = 1224 wide → 612.
        assert!((x - 612.0).abs() < 1e-6);
        assert!((w - 122.4).abs() < 1e-6);
    }

    #[test]
    fn missing_stored_page_size_assumes_current_bitmap() {
        let s = retina_page();
        let x = s.stored_to_display_x(200.0, 0.0);
        assert!((x - 100.0).abs() < 1e-9);
    }
}
