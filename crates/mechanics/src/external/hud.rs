//! Cosmetic HUD published by the trusted provider. The provider names only
//! textures inside the admitted data bundle; this side reads and checks them.
//! Frames are kept per session beside the simulation, never inside it.
use mechanics_link::{HudFrame, HudTexture};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

pub const MAX_TEXTURES: usize = 64;
pub const MAX_TEXTURE_SIDE: u32 = 4096;
pub const MAX_VERTICES: usize = 1 << 18;

/// Straight-alpha sRGB RGBA8 texels.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Default)]
pub(super) struct Overlay {
    images: Mutex<Arc<Vec<Image>>>,
    frame: Mutex<(u64, Option<Arc<HudFrame>>)>,
}

impl Overlay {
    /// Replaces the table after a provider start. An unusable table disables
    /// the textures (draws then skip) without affecting the simulation.
    pub(super) fn textures(&self, root: &Path, table: &[HudTexture]) {
        let images = load(root, table).unwrap_or_else(|error| {
            eprintln!("Movement provider HUD textures unavailable: {error}");
            Vec::new()
        });
        if let Ok(mut slot) = self.images.lock() {
            *slot = Arc::new(images);
        }
    }
    pub(super) fn images(&self) -> Arc<Vec<Image>> {
        self.images.lock().map(|i| i.clone()).unwrap_or_default()
    }
    pub(super) fn publish(&self, session: u64, frame: HudFrame) {
        if !valid(&frame) {
            eprintln!("Movement provider sent an invalid HUD frame");
            return;
        }
        if let Ok(mut slot) = self.frame.lock() {
            *slot = (session, Some(Arc::new(frame)));
        }
    }
    pub(super) fn frame(&self, session: u64) -> Option<Arc<HudFrame>> {
        let slot = self.frame.lock().ok()?;
        (slot.0 == session).then(|| slot.1.clone()).flatten()
    }
}

fn load(root: &Path, table: &[HudTexture]) -> Result<Vec<Image>, String> {
    if table.len() > MAX_TEXTURES {
        return Err("too many textures".into());
    }
    table
        .iter()
        .map(|t| {
            if !admitted(&t.path) {
                return Err(format!("unsupported texture path {}", t.path));
            }
            if t.width == 0
                || t.height == 0
                || t.width > MAX_TEXTURE_SIDE
                || t.height > MAX_TEXTURE_SIDE
            {
                return Err(format!("invalid texture size {}", t.path));
            }
            let rgba = std::fs::read(root.join(&t.path)).map_err(|e| format!("{}: {e}", t.path))?;
            if rgba.len() != t.width as usize * t.height as usize * 4 {
                return Err(format!("texture length mismatch {}", t.path));
            }
            Ok(Image {
                width: t.width,
                height: t.height,
                rgba,
            })
        })
        .collect()
}

/// Only RGBA payloads of the two admitted HUD folders, as checked by the bundle.
pub fn admitted(path: &str) -> bool {
    (path.starts_with("private/hud/") || path.starts_with("private/session-marker/"))
        && path.ends_with(".rgba")
        && crate::data::admitted(path)
}

fn valid(frame: &HudFrame) -> bool {
    frame.layers.len() <= 8
        && frame.layers.iter().all(|layer| {
            layer.vertices.len() <= MAX_VERTICES
                && layer
                    .vertices
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite() && v.abs() <= 1e6)
                && layer.draws.iter().all(|d| {
                    d.count % 3 == 0
                        && (d.first as usize)
                            .checked_add(d.count as usize)
                            .is_some_and(|end| end <= layer.vertices.len())
                        && d.texture.is_none_or(|t| (t as usize) < MAX_TEXTURES)
                        && d.multiply.iter().chain(&d.add).all(|v| v.is_finite())
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mechanics_link::{HudDraw, HudFit, HudLayer};
    fn frame(first: u32, count: u32) -> HudFrame {
        HudFrame {
            revision: 1,
            layers: vec![HudLayer {
                fit: HudFit::Stretch,
                draws: vec![HudDraw {
                    texture: Some(0),
                    multiply: [1.; 4],
                    add: [0.; 4],
                    first,
                    count,
                }],
                vertices: vec![[0.; 4]; 6],
            }],
        }
    }
    #[test]
    fn hud_frames_are_bounded_and_kept_per_session() {
        let overlay = Overlay::default();
        overlay.publish(7, frame(0, 6));
        assert!(overlay.frame(7).is_some());
        assert!(overlay.frame(8).is_none());
        for bad in [frame(3, 6), frame(0, 4), frame(u32::MAX, 3)] {
            overlay.publish(9, bad);
            assert!(overlay.frame(9).is_none());
        }
        let mut nan = frame(0, 6);
        nan.layers[0].vertices[2][1] = f32::NAN;
        overlay.publish(9, nan);
        assert!(overlay.frame(9).is_none());
    }
    #[test]
    fn hud_textures_only_come_from_admitted_rgba_payloads() {
        for path in [
            "private/hud/assets/data/fe/fonts/futuraheavy/0000_futuraheavy.Texture.rgba",
            "private/session-marker/texture-0.rgba",
        ] {
            assert!(admitted(path), "{path}");
        }
        for path in [
            "private/skater.glb",
            "private/hud/runtime/trickdisplay.json",
            "private/hud/../skater.rgba",
            "/private/hud/a.rgba",
            "private/custom/a.rgba",
        ] {
            assert!(!admitted(path), "{path}");
        }
        let root = std::env::temp_dir().join(format!("csr-hud-{}", std::process::id()));
        let file = root.join("private/session-marker/texture-0.rgba");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, [255u8; 16]).unwrap();
        let texture = |width| HudTexture {
            path: "private/session-marker/texture-0.rgba".into(),
            width,
            height: 2,
        };
        assert_eq!(load(&root, &[texture(2)]).unwrap()[0].rgba.len(), 16);
        assert!(load(&root, &[texture(3)]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
