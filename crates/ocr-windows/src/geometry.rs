//! Word boxes from the image Windows read back to the caller's pixels.
//!
//! Windows reports a word's box (`OcrWord::BoundingRect`, `[x, y, width, height]`) in the pixels
//! of the bitmap it was given. When it finds the text slanted it also reports the slant
//! (`OcrResult::TextAngle`, degrees) and measures the boxes in the bitmap turned straight about
//! its centre; turning the box corners back by that angle about the centre (clockwise for a
//! positive angle, image y pointing down) puts them on the slanted text, as the Windows OCR
//! sample does when it overlays the boxes on the picture. The skewed-page test checks this
//! against the ink.

/// Below this many degrees a reported slant is no slant: the boxes are used as they are.
const STRAIGHT: f64 = 1e-3;

/// `rect` (`[x, y, width, height]` in the `read` image, measured straight when `angle` is the
/// text's slant in degrees) as `[left, top, right, bottom]` in the `original` image's pixels:
/// turned back by the slant about the read image's centre (the box around the turned corners),
/// scaled from the read size to the original one, and clamped to the original image. `None`
/// for a box that is not finite or covers nothing.
pub(crate) fn to_original(rect: [f32; 4], angle: Option<f64>, read: (u32, u32), original: (u32, u32)) -> Option<[f32; 4]> {
    let [x, y, w, h] = rect.map(f64::from);
    if ![x, y, w, h].iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let (rw, rh) = (f64::from(read.0.max(1)), f64::from(read.1.max(1)));
    let (ow, oh) = (f64::from(original.0), f64::from(original.1));
    let mut corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)];
    if let Some(degrees) = angle.filter(|a| a.is_finite() && a.abs() >= STRAIGHT) {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let (cx, cy) = (rw / 2.0, rh / 2.0);
        for corner in &mut corners {
            let (dx, dy) = (corner.0 - cx, corner.1 - cy);
            *corner = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
        }
    }
    let (sx, sy) = (ow / rw, oh / rh);
    let xs = corners.map(|c| c.0 * sx);
    let ys = corners.map(|c| c.1 * sy);
    let left = xs.iter().copied().fold(f64::INFINITY, f64::min).clamp(0.0, ow);
    let right = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max).clamp(0.0, ow);
    let top = ys.iter().copied().fold(f64::INFINITY, f64::min).clamp(0.0, oh);
    let bottom = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max).clamp(0.0, oh);
    (right > left && bottom > top).then_some([left as f32, top as f32, right as f32, bottom as f32])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn a_box_read_at_full_size_is_its_own_corners() {
        assert_eq!(to_original([10.0, 20.0, 30.0, 5.0], None, (100, 50), (100, 50)), Some([10.0, 20.0, 40.0, 25.0]));
        assert_eq!(to_original([10.0, 20.0, 30.0, 5.0], Some(0.0), (100, 50), (100, 50)), Some([10.0, 20.0, 40.0, 25.0]), "no slant");
    }

    /// A shrunk reading scales back on each axis by its own ratio (the sides round apart).
    #[test]
    fn a_shrunk_reading_scales_back() {
        let r = to_original([100.0, 50.0, 200.0, 25.0], None, (1601, 785), (2550, 1250)).unwrap();
        let (sx, sy) = (2550.0 / 1601.0, 1250.0 / 785.0);
        assert!(close(r, [100.0 * sx, 50.0 * sy, 300.0 * sx, 75.0 * sy]), "{r:?}");
    }

    /// Positive angles turn clockwise about the read image's centre (y down): a quarter turn
    /// takes a box right of the centre to below it.
    #[test]
    fn a_slant_turns_the_box_about_the_centre() {
        // Centre (50, 50); a 10 × 4 box from (70, 48): 20 px right of the centre.
        let r = to_original([70.0, 48.0, 10.0, 4.0], Some(90.0), (100, 100), (100, 100)).unwrap();
        assert!(close(r, [48.0, 70.0, 52.0, 80.0]), "{r:?}");
        let back = to_original([70.0, 48.0, 10.0, 4.0], Some(-90.0), (100, 100), (100, 100)).unwrap();
        assert!(close(back, [48.0, 20.0, 52.0, 30.0]), "anticlockwise goes above: {back:?}");
    }

    /// A small slant moves a box far from the centre along the turn: 3° at 1000 px is 52 px.
    #[test]
    fn a_small_slant_moves_far_boxes_most() {
        // Centre (1500, 500); the box's corners are 1000 or 1010 px right of it, 5 px above or
        // below.
        let r = to_original([2500.0, 495.0, 10.0, 10.0], Some(3.0), (3000, 1000), (3000, 1000)).unwrap();
        let (sin, cos) = 3f64.to_radians().sin_cos();
        // Leftmost: the bottom-left corner (1000, 5); topmost: the top-left one (1000, -5).
        let left = 1500.0 + 1000.0 * cos - 5.0 * sin;
        let top = 500.0 + 1000.0 * sin - 5.0 * cos;
        assert!((f64::from(r[0]) - left).abs() < 1e-3 && (f64::from(r[1]) - top).abs() < 1e-3, "{r:?} vs ({left}, {top})");
        assert!(r[1] > 545.0, "moved down by about 52 px: {r:?}");
    }

    /// Boxes are clamped to the image; nothing finite and positive, nothing returned.
    #[test]
    fn hostile_boxes_are_clamped_or_refused() {
        assert_eq!(to_original([-5.0, -5.0, 20.0, 20.0], None, (100, 100), (100, 100)), Some([0.0, 0.0, 15.0, 15.0]));
        assert_eq!(to_original([90.0, 90.0, 50.0, 50.0], None, (100, 100), (100, 100)), Some([90.0, 90.0, 100.0, 100.0]));
        for rect in [
            [f32::NAN, 0.0, 10.0, 10.0],
            [0.0, f32::INFINITY, 10.0, 10.0],
            [0.0, 0.0, 0.0, 10.0],
            [0.0, 0.0, 10.0, -1.0],
            [200.0, 0.0, 10.0, 10.0],
            [0.0, 0.0, f32::MAX, f32::MAX],
        ] {
            let r = to_original(rect, None, (100, 100), (100, 100));
            assert!(r.is_none() || r.is_some_and(|r| r.iter().all(|v| v.is_finite() && (0.0..=100.0).contains(v))), "{rect:?} → {r:?}");
        }
        assert_eq!(to_original([200.0, 0.0, 10.0, 10.0], None, (100, 100), (100, 100)), None, "wholly outside");
        assert_eq!(
            to_original([10.0, 10.0, 10.0, 10.0], Some(f64::NAN), (100, 100), (100, 100)),
            Some([10.0, 10.0, 20.0, 20.0]),
            "a NaN slant is none"
        );
    }
}
