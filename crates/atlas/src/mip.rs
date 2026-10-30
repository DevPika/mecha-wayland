//! Mip generation: a 2 by 2 box filter over a channel count, applied to
//! the region an insert touched, and to a whole bitmap for `fit`.

use geometry::Rect;

use crate::{Bitmap, Page};

/// How four-channel pixels carry alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Alpha {
    /// Colour is independent of alpha: weight it by alpha when averaging.
    Straight,
    /// Colour is already multiplied by alpha: a plain average.
    Premultiplied,
}

/// `src` is `width` by `height` with `channels` bytes per pixel. The
/// result is `max(width / 2, 1)` by `max(height / 2, 1)`. Four `Straight`
/// channels are averaged premultiplied and stored straight again, a block
/// whose alpha sums to zero staying zero; `Premultiplied` ones and other
/// channel counts are a plain average. Empty when `width == 0 ||
/// height == 0`, so a zero-dimension region never indexes past the (empty)
/// slice.
pub(crate) fn downsample(
    src: &[u8],
    width: u32,
    height: u32,
    channels: usize,
    alpha: Alpha,
) -> Vec<u8> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let (w, h) = (width as usize, height as usize);
    let (ow, oh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; ow * oh * channels];
    let at = |x: usize, y: usize| &src[(y.min(h - 1) * w + x.min(w - 1)) * channels..][..channels];
    for oy in 0..oh {
        for ox in 0..ow {
            let px = [
                at(2 * ox, 2 * oy),
                at(2 * ox + 1, 2 * oy),
                at(2 * ox, 2 * oy + 1),
                at(2 * ox + 1, 2 * oy + 1),
            ];
            let dst = &mut out[(oy * ow + ox) * channels..][..channels];
            if channels == 4 && alpha == Alpha::Straight {
                let total: u32 = px.iter().map(|p| p[3] as u32).sum();
                if total == 0 {
                    continue;
                }
                for c in 0..3 {
                    // premultiplied sum over 4 pixels, divided by the alpha sum: straight again
                    let sum: u32 = px.iter().map(|p| p[c] as u32 * p[3] as u32).sum();
                    dst[c] = ((sum + total / 2) / total) as u8;
                }
                dst[3] = ((total + 2) / 4) as u8;
            } else {
                for c in 0..channels {
                    let sum: u32 = px.iter().map(|p| p[c] as u32).sum();
                    dst[c] = ((sum + 2) / 4) as u8;
                }
            }
        }
    }
    out
}

impl Page {
    /// Rebuild levels 1.. under `rect`, a level-0 rect on the class's
    /// 16 grid at its origin, its size rounded up to 16. Each level reads
    /// the one above it, so the region shrinks by half per level and never
    /// straddles a texel.
    pub(crate) fn regenerate(&mut self, rect: Rect) {
        let channels = self.class().format().bytes();
        let align = self.class().align();
        let x0 = rect.x() as u32;
        let y0 = rect.y() as u32;
        let w0 = (rect.width() as u32).div_ceil(align) * align;
        let h0 = (rect.height() as u32).div_ceil(align) * align;
        if w0 == 0 || h0 == 0 {
            return;
        }
        for k in 1..self.levels() {
            let (sx, sy, sw, sh) = (x0 >> (k - 1), y0 >> (k - 1), w0 >> (k - 1), h0 >> (k - 1));
            let src_side = Page::dims(k - 1) as usize;
            // Cut the source region out contiguously, downsample, paste at level k.
            let mut region = Vec::with_capacity((sw * sh) as usize * channels);
            {
                let src = self.level(k - 1);
                for y in sy..sy + sh {
                    let row = (y as usize * src_side + sx as usize) * channels;
                    region.extend_from_slice(&src[row..row + sw as usize * channels]);
                }
            }
            // Image pages hold premultiplied pixels; the others are single-channel.
            let alpha = if channels == 4 { Alpha::Premultiplied } else { Alpha::Straight };
            let small = downsample(&region, sw, sh, channels, alpha);
            let (dx, dy, dw, dh) = (sx / 2, sy / 2, (sw / 2).max(1), (sh / 2).max(1));
            let dst_side = Page::dims(k) as usize;
            let dst = self.level_mut(k);
            for r in 0..dh as usize {
                let d = ((dy as usize + r) * dst_side + dx as usize) * channels;
                let s = r * dw as usize * channels;
                dst[d..d + dw as usize * channels]
                    .copy_from_slice(&small[s..s + dw as usize * channels]);
            }
        }
    }
}

impl Bitmap {
    /// Halve through the mip filter until both sides are at most `max`.
    /// A bitmap that already fits is returned as is. `max` clamps to at
    /// least 1, so a caller passing 0 terminates at a 1 by 1 bitmap
    /// instead of looping forever.
    pub fn fit(mut self, max: u32) -> Bitmap {
        let max = max.max(1);
        while self.width > max || self.height > max {
            let channels = self.format.bytes();
            self.pixels =
                downsample(&self.pixels, self.width, self.height, channels, Alpha::Straight);
            self.width = (self.width / 2).max(1);
            self.height = (self.height / 2).max(1);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtlasId, Class, Format, page::PAGE};

    #[test]
    fn one_channel_averages() {
        let src = [10u8, 20, 30, 40];
        assert_eq!(downsample(&src, 2, 2, 1, Alpha::Straight), vec![25]);
    }

    #[test]
    fn four_channels_average_premultiplied() {
        // opaque red next to three transparent pixels: red at quarter alpha,
        // not a dark red.
        let mut src = vec![0u8; 16];
        src[0..4].copy_from_slice(&[255, 0, 0, 255]);
        let out = downsample(&src, 2, 2, 4, Alpha::Straight);
        assert_eq!(out, vec![255, 0, 0, 64]);
        // all transparent stays zero without dividing by zero
        assert_eq!(downsample(&[0u8; 16], 2, 2, 4, Alpha::Straight), vec![0, 0, 0, 0]);
    }

    #[test]
    fn premultiplied_average_is_plain() {
        let mut src = vec![0u8; 16];
        src[0..4].copy_from_slice(&[255, 0, 0, 255]);
        let out = downsample(&src, 2, 2, 4, Alpha::Premultiplied);
        assert_eq!(out, vec![64, 0, 0, 64]);
    }

    #[test]
    fn odd_sizes_round_down_to_at_least_one() {
        let src = vec![100u8; 3 * 1];
        assert_eq!(downsample(&src, 3, 1, 1, Alpha::Straight), vec![100]);
    }

    #[test]
    fn downsample_of_an_empty_region_is_empty() {
        assert_eq!(downsample(&[], 0, 0, 1, Alpha::Straight), Vec::<u8>::new());
        assert_eq!(downsample(&[], 0, 5, 4, Alpha::Straight), Vec::<u8>::new());
    }

    #[test]
    fn regenerate_touches_only_the_region() {
        let mut p = Page::new(AtlasId(0), Class::Icon);
        // Fill a 32 by 32 region at (16, 16) with 200 at level 0, and put a
        // sentinel outside it at level 1.
        {
            let l0 = p.level_mut(0);
            for y in 16..48 {
                for x in 16..48 {
                    l0[y * PAGE as usize + x] = 200;
                }
            }
        }
        {
            let l1 = p.level_mut(1);
            l1[0] = 77; // pixel (0,0) at level 1: covers (0..2, 0..2) at level 0, outside the region
        }
        p.regenerate(Rect::new(16.0, 16.0, 32.0, 32.0));
        let side1 = (PAGE / 2) as usize;
        assert_eq!(
            p.level(1)[8 * side1 + 8],
            200,
            "level 1 pixel (8,8) is the region's corner"
        );
        assert_eq!(
            p.level(1)[23 * side1 + 23],
            200,
            "level 1 pixel (23,23) is the region's far corner"
        );
        assert_eq!(p.level(1)[24 * side1 + 24], 0, "just past it");
        assert_eq!(p.level(1)[0], 77, "outside the region is untouched");
        let side4 = (PAGE / 16) as usize;
        assert_eq!(
            p.level(4)[side4 + 1],
            200,
            "level 4 pixel (1,1) is the whole 16-pixel block at (16,16)"
        );
        assert_eq!(p.level(4)[2 * side4 + 2], 200);
        assert_eq!(p.level(4)[3 * side4 + 3], 0);
        // The region's edge at level 4 is exactly its 16-grid, so nothing partial.
        assert_eq!(p.level(4)[0], 0);
    }

    #[test]
    fn fit_halves_until_it_fits() {
        let b = Bitmap {
            width: 200,
            height: 100,
            format: Format::R8,
            pixels: vec![50; 200 * 100],
        };
        let f = b.fit(64);
        assert_eq!((f.width, f.height), (50, 25));
        assert_eq!(f.pixels.len(), 50 * 25);
        assert!(f.pixels.iter().all(|&v| v == 50));
        let small = Bitmap {
            width: 10,
            height: 10,
            format: Format::Rgba8,
            pixels: vec![1; 400],
        };
        assert_eq!(small.clone().fit(64), small, "already fits: unchanged");
    }

    #[test]
    fn fit_with_a_zero_max_terminates_at_one_pixel() {
        let b = Bitmap {
            width: 8,
            height: 4,
            format: Format::R8,
            pixels: vec![9; 8 * 4],
        };
        let f = b.fit(0);
        assert_eq!((f.width, f.height), (1, 1));
        assert_eq!(f.pixels.len(), 1);
    }
}
