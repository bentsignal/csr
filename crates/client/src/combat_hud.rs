//! Presentation snapshots and pixel regions for the shared authored HUD worker.
//! Gameplay continues to change only through simulation inputs.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub player: sim::PlayerId,
    pub life: u64,
    pub alive: bool,
    pub health: i32,
    pub armor: i32,
    pub helmet: bool,
    pub weapon: Option<Ammo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ammo {
    pub content: String,
    pub class_name: String,
    pub name_token: String,
    pub clip: i32,
    pub reserve: i32,
    pub capacity: i32,
    pub uses_ammo: bool,
}

impl Snapshot {
    pub fn from_player(player: &sim::Player) -> Self {
        Self {
            player: player.id,
            life: player.life_epoch,
            alive: player.alive,
            health: player.health,
            armor: player.armor,
            helmet: player.helmet,
            weapon: player.active_weapon().map(|weapon| Ammo {
                content: weapon.def.id.to_string(),
                class_name: weapon.def.class_name.clone(),
                name_token: weapon.def.name_token.clone(),
                clip: weapon.clip,
                reserve: weapon.reserve,
                capacity: weapon.def.clip_size,
                uses_ammo: weapon.uses_ammo(),
            }),
        }
    }

    /// Pending ordinary value updates may retain the last valid raster. A new
    /// viewed player/life must never inherit the old player's health or ammo.
    pub fn same_life(&self, other: &Self) -> bool {
        (self.player, self.life, self.alive) == (other.player, other.life, other.alive)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Partition in physical pixels, never fractional CSS halves. Odd dimensions
/// belong to the top/left region so no source pixel is duplicated or stretched.
pub(crate) fn regions(width: u32, height: u32) -> [Rect; 3] {
    let top = height.div_ceil(2);
    let left = width.div_ceil(2);
    [
        Rect {
            x: 0,
            y: 0,
            width,
            height: top,
        },
        Rect {
            x: 0,
            y: top,
            width: left,
            height: height - top,
        },
        Rect {
            x: left,
            y: top,
            width: width - left,
            height: height - top,
        },
    ]
}

pub(crate) fn crop(frame: &scaleform::Frame, rect: Rect) -> Option<Vec<u8>> {
    let stride = (frame.width as usize).checked_mul(4)?;
    let expected = stride.checked_mul(frame.height as usize)?;
    if frame.rgba.len() != expected
        || rect.width == 0
        || rect.height == 0
        || rect.x.checked_add(rect.width)? > frame.width
        || rect.y.checked_add(rect.height)? > frame.height
    {
        return None;
    }
    let row_bytes = (rect.width as usize).checked_mul(4)?;
    let mut rgba = Vec::with_capacity(row_bytes.checked_mul(rect.height as usize)?);
    for y in rect.y..rect.y + rect.height {
        let start = y as usize * stride + rect.x as usize * 4;
        rgba.extend_from_slice(frame.rgba.get(start..start + row_bytes)?);
    }
    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_damage_can_retain_pixels_but_respawn_and_spectating_cannot() {
        let old = Snapshot {
            player: 1,
            life: 3,
            alive: true,
            health: 100,
            armor: 100,
            helmet: true,
            weapon: None,
        };
        let mut next = old.clone();
        next.health = 73;
        assert!(old.same_life(&next));
        next.life += 1;
        assert!(!old.same_life(&next));
        next.life = old.life;
        next.player = 2;
        assert!(!old.same_life(&next));
        next.player = old.player;
        next.alive = false;
        assert!(!old.same_life(&next));
    }

    #[test]
    fn odd_viewport_crops_preserve_every_physical_pixel_once() {
        let frame = scaleform::Frame {
            width: 5,
            height: 3,
            serial: 1,
            presentation_revision: 7,
            rgba: (0..15).flat_map(|n| [n, n, n, 255]).collect(),
        };
        let parts = regions(5, 3);
        assert_eq!(
            parts.map(|rect| (rect.width, rect.height)),
            [(5, 2), (3, 1), (2, 1)]
        );
        let pixels: Vec<_> = parts
            .into_iter()
            .flat_map(|rect| {
                crop(&frame, rect)
                    .unwrap()
                    .chunks_exact(4)
                    .map(|pixel| pixel[0])
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(pixels, (0..15).collect::<Vec<_>>());
        assert!(
            crop(
                &frame,
                Rect {
                    x: 4,
                    y: 2,
                    width: 2,
                    height: 1
                }
            )
            .is_none()
        );
        assert!(
            crop(
                &frame,
                Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 0
                }
            )
            .is_none()
        );
    }

    #[test]
    fn truncated_raster_is_rejected_instead_of_hiding_usable_fallback() {
        let frame = scaleform::Frame {
            width: 1920,
            height: 1080,
            rgba: vec![0; 8],
            serial: 1,
            presentation_revision: 3,
        };
        assert!(crop(&frame, regions(1920, 1080)[0]).is_none());
    }
}
