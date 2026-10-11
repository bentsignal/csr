//! Screenshot difference metric. Both images must be the same size (the
//! references are 1920x1080 for the home screen).

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

pub fn run(a: &Path, b: &Path, heatmap: Option<&Path>) -> Result<()> {
    let ia = image::open(a).with_context(|| a.display().to_string())?.to_rgba8();
    let ib = image::open(b).with_context(|| b.display().to_string())?.to_rgba8();
    let print = |label: &str, m: &Metrics| {
        println!(
            "{label}: MAE {:.2}  RMSE {:.2}  PSNR {:.2} dB  pixels off by >32: {:.1}%",
            m.mae,
            m.rmse,
            m.psnr,
            m.over_32 * 100.0
        )
    };
    print("full frame", &compare(&ia, &ib, &Mask::new())?);
    if let Ok(spec) = std::env::var("SPIKE_DIFF_MASK") {
        let mask = parse_mask(&spec)?;
        print(&format!("masked ({} rects)", mask.len()), &compare(&ia, &ib, &mask)?);
    }
    // Per-region breakdown on a 4x3 grid, so a diff points at a panel.
    let (w, h) = ia.dimensions();
    for gy in 0..3 {
        let mut row = String::new();
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
            row.push_str(&format!("{:7.2}", sum / ((x1 - x0) * (y1 - y0) * 3) as f64));
        }
        println!("  region MAE{row}");
    }
    if let Some(out) = heatmap {
        let map = image::RgbaImage::from_fn(w, h, |x, y| {
            let (pa, pb) = (ia.get_pixel(x, y), ib.get_pixel(x, y));
            let d = (0..3).map(|c| (pa[c] as i32 - pb[c] as i32).unsigned_abs()).max().unwrap_or(0);
            let v = (d * 4).min(255) as u8;
            image::Rgba([v, v, v, 255])
        });
        map.save(out)?;
    }
    Ok(())
}
