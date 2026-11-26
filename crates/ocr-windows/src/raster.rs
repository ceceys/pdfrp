//! The pixels Windows reads: the page as 8-bit grey composited over white paper, shrunk by area
//! averaging when it is larger than Windows (or the caller) allows.

use migez_ocr::OcrImage;

/// An 8-bit grey raster, row-major: exactly `width * height` bytes.
pub(crate) struct Grey {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// The size an image of `width` × `height` is read at so that neither side is over `max_side`:
/// unchanged when it fits; otherwise the long side becomes `max_side` and the short one shrinks
/// in proportion (rounded to the nearest pixel, at least 1). A `max_side` of 0 counts as 1.
pub(crate) fn fit(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let max_side = max_side.max(1);
    let long = width.max(height);
    if long <= max_side {
        return (width, height);
    }
    // Integer arithmetic, so the long side is exactly `max_side` and the short side rounds
    // exactly. `long` is over `max_side`, so it is not zero.
    let shrink = |side: u32| {
        let scaled = (u64::from(side) * u64::from(max_side) + u64::from(long) / 2) / u64::from(long);
        u32::try_from(scaled).unwrap_or(max_side).clamp(1, max_side)
    };
    if width >= height { (max_side, shrink(height)) } else { (shrink(width), max_side) }
}

/// One pixel's grey (0 ink … 255 paper), with the weights [`OcrImage::gray`] uses, laid over
/// white. The rasters the engine hands over are premultiplied RGBA (the renderer's output), so
/// paper shows through by `255 - alpha`: a transparent pixel reads as paper, not as ink.
fn grey_of([r, g, b, a]: [u8; 4]) -> u8 {
    let ink = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
    // At most 255 + 255; the paper saturates.
    u8::try_from(ink + 255 - u32::from(a)).unwrap_or(u8::MAX)
}

/// `image` in grey ([`grey_of`] each pixel), at most `max_side` pixels on a side ([`fit`]).
/// When it has to shrink, each output pixel is the average of the source area it covers, the
/// cells at the area's edges counted by the fraction covered.
pub(crate) fn grey(image: &OcrImage, max_side: u32) -> Grey {
    let pixels = image.rgba().as_chunks::<4>().0;
    let (width, height) = fit(image.width(), image.height(), max_side);
    if (width, height) == (image.width(), image.height()) {
        return Grey { width, height, pixels: pixels.iter().map(|p| grey_of(*p)).collect() };
    }
    let (sw, sh) = (image.width() as usize, image.height() as usize);
    let (dw, dh) = (width as usize, height as usize);
    let columns = spans(sw, dw);
    let rows = spans(sh, dh);
    let mut out = Vec::with_capacity(dw.saturating_mul(dh));
    // One source row shrunk across, and the weighted sum of those for one output row.
    let mut row = vec![0f32; dw];
    let mut sum = vec![0f32; dw];
    for span in &rows {
        sum.fill(0.0);
        for (y, weight) in span.cells() {
            let start = y.saturating_mul(sw);
            let source = pixels.get(start..start.saturating_add(sw)).unwrap_or_default();
            shrink_row(source, &columns, &mut row);
            for (s, v) in sum.iter_mut().zip(&row) {
                *s += v * weight;
            }
        }
        // `total` is the span's length in source pixels, more than 0 (see `spans`).
        out.extend(sum.iter().map(|s| (s / span.total).round().clamp(0.0, 255.0) as u8));
    }
    Grey { width, height, pixels: out }
}

/// `source` (one row of RGBA pixels) in grey, shrunk to one value per span of `columns`.
fn shrink_row(source: &[[u8; 4]], columns: &[Span], out: &mut [f32]) {
    for (value, span) in out.iter_mut().zip(columns) {
        let sum: f32 = span.cells().map(|(x, weight)| source.get(x).map_or(255.0, |p| f32::from(grey_of(*p))) * weight).sum();
        *value = sum / span.total;
    }
}

/// The source cells one output pixel covers along an axis: `first..=last`, the first and the
/// last perhaps only in part (`head` and `tail` of their width), `total` source pixels in all.
#[derive(Clone, Debug, PartialEq)]
struct Span {
    first: usize,
    last: usize,
    head: f32,
    tail: f32,
    total: f32,
}

impl Span {
    /// Each covered cell and how much of it the output pixel covers.
    fn cells(&self) -> impl Iterator<Item = (usize, f32)> {
        (self.first..=self.last).map(|i| {
            let weight = if i == self.first {
                self.head
            } else if i == self.last {
                self.tail
            } else {
                1.0
            };
            (i, weight)
        })
    }
}

/// The spans `dst` output pixels cover of `src` source pixels (`1 <= dst <= src`, clamped
/// there): output pixel `i` covers `[i * src / dst, (i + 1) * src / dst)`.
fn spans(src: usize, dst: usize) -> Vec<Span> {
    let src = src.max(1);
    let dst = dst.clamp(1, src);
    let step = src as f64 / dst as f64;
    (0..dst)
        .map(|i| {
            let a = i as f64 * step;
            let b = ((i + 1) as f64 * step).min(src as f64);
            let first = (a.floor() as usize).min(src - 1);
            let last = (b.ceil() as usize).saturating_sub(1).clamp(first, src - 1);
            let head = (((first + 1) as f64).min(b) - a).max(0.0);
            let tail = if last == first { head } else { (b - last as f64).max(0.0) };
            // A span is `step` >= 1 source pixel long; the floor only guards the division.
            Span { first, last, head: head as f32, tail: tail as f32, total: ((b - a) as f32).max(f32::MIN_POSITIVE) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, grey: impl Fn(u32, u32) -> u8) -> OcrImage {
        let rgba = (0..height).flat_map(|y| (0..width).map(move |x| (x, y))).flat_map(|(x, y)| [grey(x, y), grey(x, y), grey(x, y), 255]).collect();
        OcrImage::new(width, height, rgba).unwrap()
    }

    #[test]
    fn images_that_fit_keep_their_size() {
        assert_eq!(fit(2550, 3300, 10_000), (2550, 3300));
        assert_eq!(fit(10_000, 10_000, 10_000), (10_000, 10_000));
        assert_eq!(fit(1, 1, 1), (1, 1));
    }

    /// The long side lands exactly on the limit and the short side rounds to the nearest pixel;
    /// nothing ever reaches 0.
    #[test]
    fn larger_images_shrink_in_proportion() {
        assert_eq!(fit(2550, 1250, 1601), (1601, 785), "1250 × 1601 / 2550 = 784.8");
        assert_eq!(fit(1250, 2550, 1601), (785, 1601), "the same, standing");
        assert_eq!(fit(3000, 3000, 1000), (1000, 1000));
        assert_eq!(fit(10_000, 3, 100), (100, 1), "0.03 rounds up to the 1-pixel minimum");
        assert_eq!(fit(400, 200, 0), (1, 1), "a zero limit counts as 1");
    }

    /// Premultiplied pixels over white: opaque ones keep their grey, transparent ones are paper.
    #[test]
    fn grey_is_laid_over_white_paper() {
        assert_eq!(grey_of([0, 0, 0, 255]), 0);
        assert_eq!(grey_of([255, 255, 255, 255]), 255);
        assert_eq!(grey_of([0, 0, 0, 0]), 255, "transparent is paper, not ink");
        assert_eq!(grey_of([0, 0, 0, 128]), 127, "half-covered black is mid grey");
        assert_eq!(grey_of([255, 0, 0, 255]), 76, "the weights OcrImage::gray uses");
    }

    #[test]
    fn an_image_that_fits_is_read_as_its_grey() {
        let img = image(3, 2, |x, y| (x * 10 + y * 100) as u8);
        let g = grey(&img, 3);
        assert_eq!((g.width, g.height), (3, 2));
        assert_eq!(g.pixels, vec![0, 10, 20, 100, 110, 120]);
    }

    /// Halving averages each 2 × 2 block exactly.
    #[test]
    fn halving_averages_blocks() {
        let img = image(4, 2, |x, _| if x < 2 { 0 } else { 200 });
        let g = grey(&img, 2);
        assert_eq!((g.width, g.height, g.pixels), (2, 1, vec![0, 200]));
        let checker = image(4, 4, |x, y| if (x + y) % 2 == 0 { 0 } else { 254 });
        assert_eq!(grey(&checker, 2).pixels, vec![127; 4]);
    }

    /// At a ratio that is not whole the edge cells count in part, and the mean is kept.
    #[test]
    fn fractional_shrinking_keeps_the_mean() {
        let flat = image(7, 5, |_, _| 90);
        let g = grey(&flat, 3);
        assert_eq!((g.width, g.height), (3, 2));
        assert!(g.pixels.iter().all(|&v| v == 90), "a flat image stays flat: {:?}", g.pixels);
        // Left half black, right half 200, 10 px read at 3: the middle pixel covers
        // [3.33, 6.67), 1.67 px of each, so (0 + 200) / 2.
        let split = image(10, 1, |x, _| if x < 5 { 0 } else { 200 });
        assert_eq!(grey(&split, 3).pixels, vec![0, 100, 200]);
    }

    /// The buffer always holds exactly one byte per pixel of the size it reports, at any ratio.
    #[test]
    fn the_buffer_matches_the_size_it_reports() {
        for (w, h, max) in [(10, 7, 3), (7, 10, 3), (9, 9, 4), (31, 2, 30), (5, 5, 5)] {
            let g = grey(&image(w, h, |x, y| ((x + y) % 256) as u8), max);
            assert_eq!(g.pixels.len(), g.width as usize * g.height as usize, "{w}×{h} at {max}");
            assert!(g.width <= max && g.height <= max, "{w}×{h} at {max}: {}×{}", g.width, g.height);
        }
    }

    /// Spans tile the source exactly: their lengths and their cells' weights add up to it.
    #[test]
    fn spans_tile_the_source() {
        for (src, dst) in [(10, 3), (2550, 1601), (7, 7), (5, 1), (10_000, 9_999)] {
            let s = spans(src, dst);
            assert_eq!(s.len(), dst);
            let total: f64 = s.iter().map(|span| f64::from(span.total)).sum();
            assert!((total - src as f64).abs() < 0.01, "{src} → {dst}: {total}");
            let weights: f64 = s.iter().flat_map(|span| span.cells().map(|(_, w)| f64::from(w))).sum();
            assert!((weights - src as f64).abs() < 0.01, "{src} → {dst}: weights {weights}");
            assert!(s.iter().all(|span| span.last < src && span.first <= span.last));
        }
    }
}
