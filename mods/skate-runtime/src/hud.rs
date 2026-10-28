// SPDX-License-Identifier: GPL-3.0-only
//! Original trick display and session-marker panel, published as plain
//! textured triangles. The upstream APT runtime advances once per native
//! physics tick, after scoring; its display list is traversed only when the
//! host asks for a frame. The host draws it; no renderer lives here.
//! Failures disable the overlay and never affect the simulation.
use mechanics_link::{HudDraw, HudFit, HudFrame, HudLayer, HudTexture};
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// Immutable display data from the admitted bundle, parsed once per root.
pub(crate) struct Source {
    scoring: Option<(serde_json::Value, crate::apt_scene::Shapes)>,
    textures: Vec<HudTexture>,
    /// Scoring texture key (relative to `private/hud`) to table index.
    index: BTreeMap<String, u16>,
    marker: Option<HudLayer>,
}

#[derive(serde::Deserialize)]
struct MarkerList {
    version: u32,
    canvas: [f32; 2],
    textures: Vec<MarkerTexture>,
    meshes: Vec<MarkerMesh>,
}
#[derive(serde::Deserialize)]
struct MarkerTexture {
    file: String,
    width: u32,
    height: u32,
}
#[derive(serde::Deserialize)]
struct MarkerMesh {
    texture: Option<usize>,
    color: [f32; 4],
    vertices: Vec<MarkerVertex>,
}
#[derive(serde::Deserialize)]
struct MarkerVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

fn linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn checked(root: &Path, path: String, width: u32, height: u32) -> Result<HudTexture, String> {
    let size = std::fs::metadata(root.join(&path))
        .map_err(|e| format!("{path}: {e}"))?
        .len();
    if width == 0 || height == 0 || size != u64::from(width) * u64::from(height) * 4 {
        return Err(format!("invalid HUD texture {path}"));
    }
    Ok(HudTexture {
        path,
        width,
        height,
    })
}

impl Source {
    pub(crate) fn load(root: &Path) -> Self {
        let mut source = Self {
            scoring: None,
            textures: Vec::new(),
            index: BTreeMap::new(),
            marker: None,
        };
        if let Err(error) = source.load_scoring(root) {
            eprintln!("Original trick display unavailable: {error}");
            source.scoring = None;
            source.textures.clear();
            source.index.clear();
        }
        let first = source.textures.len();
        match marker(root, first) {
            Ok(Some((layer, textures))) => {
                source.marker = Some(layer);
                source.textures.extend(textures);
            }
            Ok(None) => {}
            Err(error) => eprintln!("Original session-marker panel unavailable: {error}"),
        }
        source
    }
    fn load_scoring(&mut self, root: &Path) -> Result<(), String> {
        let path = root.join("private/hud/runtime/trickdisplay.json");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.to_string()),
        };
        let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let shapes: crate::apt_scene::Shapes =
            serde_json::from_value(json["shapes"].clone()).map_err(|e| e.to_string())?;
        // Same texture set the standalone HUD uploads: shape bitmaps and the
        // primary/secondary native font atlases.
        let mut files = BTreeMap::new();
        for texture in shapes.values().flatten().filter_map(|s| s.texture.as_ref()) {
            files.insert(texture.rgba.clone(), [texture.width, texture.height]);
        }
        let text = crate::apt_text::TextAssets::load(&json)?;
        for font in text.fonts.values() {
            files.insert(font.texture.clone(), font.size);
            if let Some(foreground) = &font.foreground {
                files.insert(foreground.texture.clone(), foreground.size);
            }
        }
        for (key, [width, height]) in files {
            let path = format!("private/hud/{}", key.replace('\\', "/"));
            self.index.insert(key, self.textures.len() as u16);
            self.textures.push(checked(root, path, width, height)?);
        }
        self.scoring = Some((json, shapes));
        Ok(())
    }
    pub(crate) fn textures(&self) -> &[HudTexture] {
        &self.textures
    }
}

/// The converted static display list, drawn while the marker modifier is held.
/// Primitive colours are sRGB constants applied to sRGB-decoded texels.
fn marker(root: &Path, first: usize) -> Result<Option<(HudLayer, Vec<HudTexture>)>, String> {
    let folder = root.join("private/session-marker");
    let bytes = match std::fs::read(folder.join("hud.json")) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let list: MarkerList = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if list.version != 1 || list.canvas != mechanics_link::HUD_CANVAS {
        return Err("unsupported marker display list".into());
    }
    let mut textures = Vec::new();
    for t in list.textures {
        if Path::new(&t.file).components().count() != 1
            || !t.file.starts_with("texture-")
            || !t.file.ends_with(".rgba")
        {
            return Err("invalid marker texture descriptor".into());
        }
        textures.push(checked(
            root,
            format!("private/session-marker/{}", t.file),
            t.width,
            t.height,
        )?);
    }
    let mut layer = HudLayer {
        fit: HudFit::ContainTopLeft,
        draws: Vec::new(),
        vertices: Vec::new(),
    };
    for mesh in list.meshes {
        if mesh.vertices.len() % 3 != 0 || mesh.texture.is_some_and(|t| t >= textures.len()) {
            return Err("invalid marker geometry".into());
        }
        let [r, g, b, a] = mesh.color;
        layer.draws.push(HudDraw {
            texture: mesh.texture.map(|t| (first + t) as u16),
            multiply: [linear(r), linear(g), linear(b), a],
            add: [0.; 4],
            first: layer.vertices.len() as u32,
            count: mesh.vertices.len() as u32,
        });
        layer.vertices.extend(
            mesh.vertices
                .iter()
                .map(|v| [v.position[0], v.position[1], v.uv[0], v.uv[1]]),
        );
    }
    Ok(Some((layer, textures)))
}

pub(crate) struct Hud {
    source: Arc<Source>,
    runtime: Option<crate::hud_runtime::Runtime>,
    sent: Option<Vec<HudLayer>>,
    revision: u64,
}
impl Hud {
    /// A fresh movie per reconstruction; restoring replays the same ticks.
    pub(crate) fn new(root: &Path, input: crate::hud_runtime::Input) -> Self {
        let source = crate::asset_cache::hud(root);
        let runtime = source.scoring.as_ref().and_then(|(json, _)| {
            crate::hud_runtime::Runtime::load(json, input)
                .map_err(|e| eprintln!("Original trick display failed to start: {e}"))
                .ok()
        });
        Self {
            source,
            runtime,
            sent: None,
            revision: 0,
        }
    }
    /// One original HUD update per native physics tick, after scoring. The
    /// trick flags are reset at the next scoring advance.
    pub(crate) fn update(&mut self, scoring: &crate::scoring_runtime::Runtime) {
        let Some(runtime) = &mut self.runtime else {
            return;
        };
        let input = scoring.hud_input();
        if std::env::var_os("CSR_SKATE_HUD_TRACE").is_some()
            && (scoring.new_trick
                || scoring.modified_trick
                || scoring.close_tricks
                || input.sequence_score != runtime.bindings.input.sequence_score)
        {
            eprintln!(
                "SKATE_HUD trick={:?} score={} line={} x{} new={} modified={} close={}",
                input.trick_name,
                input.sequence_score,
                input.line_score,
                input.multiplier,
                scoring.new_trick,
                scoring.modified_trick,
                scoring.close_tricks
            );
        }
        if let Err(error) = runtime.update(
            input,
            scoring.new_trick,
            scoring.modified_trick,
            scoring.close_tricks,
        ) {
            eprintln!("Original trick display stopped: {error}");
            self.runtime = None;
        }
    }
    /// The overlay if it changed since the previous publication.
    pub(crate) fn frame(&mut self, marker: bool) -> Option<HudFrame> {
        let mut layers = Vec::new();
        if let (Some(runtime), Some((_, shapes))) = (&self.runtime, &self.source.scoring) {
            match crate::apt_scene::draw(&runtime.bindings.movie, &runtime.vm, shapes) {
                Ok(draws) => {
                    let mut layer = HudLayer {
                        fit: HudFit::Stretch,
                        draws: Vec::new(),
                        vertices: Vec::new(),
                    };
                    for draw in draws {
                        // The standalone renderer skips draws without an
                        // uploaded texture (solid units have none).
                        let Some(&texture) = self.source.index.get(&draw.texture) else {
                            continue;
                        };
                        layer.draws.push(HudDraw {
                            texture: Some(texture),
                            multiply: draw.multiply,
                            add: draw.add,
                            first: layer.vertices.len() as u32,
                            count: draw.vertices.len() as u32,
                        });
                        layer.vertices.extend(draw.vertices.iter().map(|v| {
                            [v.position[0], v.position[1], v.uv[0], v.uv[1]]
                        }));
                    }
                    layers.push(layer);
                }
                Err(error) => {
                    eprintln!("Original trick display geometry: {error}");
                    self.runtime = None;
                }
            }
        }
        if marker {
            layers.extend(self.source.marker.clone());
        }
        if self.sent.as_ref() == Some(&layers) {
            return None;
        }
        self.revision += 1;
        self.sent = Some(layers.clone());
        Some(HudFrame {
            revision: self.revision,
            layers,
        })
    }
}

impl crate::Runtime {
    pub fn hud_textures(&self) -> Vec<HudTexture> {
        self.hud.source.textures().to_vec()
    }
    pub fn hud_frame(&mut self) -> Option<HudFrame> {
        self.hud.frame(self.marker.ui.visible)
    }
}
