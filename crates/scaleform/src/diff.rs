//! Screenshot difference metric for fidelity checks against
//! `docs/reference/` (1920x1080 for the home screen).

use std::path::Path;

use anyhow::{Context, Result, bail};

pub struct Metrics {
    /// Mean absolute difference per channel, 0..255.
    pub mae: f64,
    pub rmse: f64,
    pub psnr: f64,
    /// Share of pixels whose largest channel difference exceeds 32.
    pub over_32: f64,
}

/// Rectangles `(x0, y0, x1, y1)` left out of the metric.
pub type Mask = Vec<(u32, u32, u32, u32)>;

/// Parts of `docs/reference/menu-2018-home.png` that are not Scaleform
/// content: the blog web page inside the News panel, the `net_graph` text,
/// and the Steam avatars (profile card and both friend columns).
pub const HOME_MASK: &str = "734,392,1582,1053;1840,980,1920,1025;294,164,356,226;285,428,332,1048;480,428,527,1048";

/// Parses `x0,y0,x1,y1;x0,y0,x1,y1`.
pub fn parse_mask(s: &str) -> Result<Mask> {
    s.split(';')
        .filter(|r| !r.trim().is_empty())
        .map(|r| {
            let v: Vec<u32> = r.split(',').map(|n| n.trim().parse()).collect::<Result<_, _>>()?;
            match v[..] {
                [x0, y0, x1, y1] => Ok((x0, y0, x1, y1)),
                _ => bail!("mask rectangle needs x0,y0,x1,y1: {r}"),
            }
        })
        .collect()
}

pub fn compare(a: &image::RgbaImage, b: &image::RgbaImage, mask: &Mask) -> Result<Metrics> {
    if a.dimensions() != b.dimensions() {
        bail!("size mismatch: {:?} vs {:?}", a.dimensions(), b.dimensions());
    }
    let masked = |x: u32, y: u32| mask.iter().any(|&(x0, y0, x1, y1)| x >= x0 && x < x1 && y >= y0 && y < y1);
    let (mut sum, mut sq, mut over, mut count) = (0f64, 0f64, 0usize, 0usize);
    for (x, y, pa) in a.enumerate_pixels() {
        if masked(x, y) {
            continue;
        }
        count += 1;
        let pb = b.get_pixel(x, y);
        let mut worst = 0i32;
        for c in 0..3 {
            let d = (pa[c] as i32 - pb[c] as i32).abs();
            sum += d as f64;
            sq += (d * d) as f64;
            worst = worst.max(d);
        }
        if worst > 32 {
            over += 1;
        }
    }
    let n = count.max(1) as f64;
    let mse = sq / (n * 3.0);
    Ok(Metrics {
        mae: sum / (n * 3.0),
        rmse: mse.sqrt(),
        psnr: if mse == 0.0 { f64::INFINITY } else { 10.0 * (255.0f64 * 255.0 / mse).log10() },
        over_32: over as f64 / n,
    })
}

impl std::fmt::Display for Metrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MAE {:.2}  RMSE {:.2}  PSNR {:.2} dB  pixels off by >32: {:.1}%",
            self.mae,
            self.rmse,
            self.psnr,
            self.over_32 * 100.0
        )
    }
}

/// Compares two image files: full frame, masked, and a 4x3 grid of region
/// MAE so a difference points at a panel. Optionally writes a heat map.
pub fn report(a: &Path, b: &Path, mask: &Mask, heatmap: Option<&Path>) -> Result<String> {
    let ia = image::open(a).with_context(|| a.display().to_string())?.to_rgba8();
    let ib = image::open(b).with_context(|| b.display().to_string())?.to_rgba8();
    let mut out = format!("full frame: {}\n", compare(&ia, &ib, &Mask::new())?);
    if !mask.is_empty() {
        out += &format!("masked ({} rects): {}\n", mask.len(), compare(&ia, &ib, mask)?);
    }
    let (w, h) = ia.dimensions();
    for gy in 0..3 {
        out += "  region MAE";
        for gx in 0..4 {
            let (x0, x1, y0, y1) = (gx * w / 4, (gx + 1) * w / 4, gy * h / 3, (gy + 1) * h / 3);
            let mut sum = 0f64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let (pa, pb) = (ia.get_pixel(x, y), ib.get_pixel(x, y));
                    for c in 0..3 {
                        sum += (pa[c] as f64 - pb[c] as f64).abs();
                    }
                }
            }
            out += &format!("{:7.2}", sum / ((x1 - x0) * (y1 - y0) * 3) as f64);
        }
        out += "\n";
    }
    if let Some(path) = heatmap {
        let map = image::RgbaImage::from_fn(w, h, |x, y| {
            let (pa, pb) = (ia.get_pixel(x, y), ib.get_pixel(x, y));
            let d = (0..3).map(|c| (pa[c] as i32 - pb[c] as i32).unsigned_abs()).max().unwrap_or(0);
            let v = (d * 4).min(255) as u8;
            image::Rgba([v, v, v, 255])
        });
        map.save(path)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_have_no_error() {
        let a = image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]));
        let m = compare(&a, &a, &Mask::new()).unwrap();
        assert_eq!(m.mae, 0.0);
        assert!(m.psnr.is_infinite());
    }

    #[test]
    fn masked_pixels_are_ignored() {
        let a = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));
        let mut b = a.clone();
        b.put_pixel(0, 0, image::Rgba([255, 255, 255, 255]));
        assert!(compare(&a, &b, &Mask::new()).unwrap().mae > 0.0);
        assert_eq!(compare(&a, &b, &parse_mask("0,0,1,1").unwrap()).unwrap().mae, 0.0);
    }
}
