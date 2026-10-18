//! Owned loading images from the admitted pinned classic map BSPs.
//!
//! Decoding runs on the movie thread, outside the shared frontend-data lock.
//! No archive bytes or installed artwork are included in the application.
use sha2::{Digest, Sha256};
use std::{io::Read, sync::{OnceLock, atomic::{AtomicBool, Ordering}}};

#[derive(Clone, Copy)]
struct Pin {
    name: &'static str,
    bytes: usize,
    sha256: &'static str,
}

const PINS: &[Pin] = &[
    Pin {
        name: "de_dust2_2017",
        bytes: 53_274_736,
        sha256: "51502850271C3A8C50320094DA669C430CFA81B3C411511F22FA8A27AA52247D",
    },
    Pin {
        name: "de_inferno_2016",
        bytes: 30_300_620,
        sha256: "55BF80411C395623FC46D8EFD9FBF1C23DCE89A92D464050AAD7DBC0D0834588",
    },
    Pin {
        name: "de_nuke_2016",
        bytes: 24_583_496,
        sha256: "091E25D8DC34CD95F801A4E6D80D7E2BBD5829D0B81EF7F077F1D79307FA9A3D",
    },
    Pin {
        name: "de_train_2015",
        bytes: 46_592_888,
        sha256: "93EC736BC1E74C36F4EEEEFB4C8C1307F2244D68AEB9C25D0EFB8A17F2F4FC38",
    },
    Pin {
        name: "de_vertigo_2017",
        bytes: 29_929_772,
        sha256: "C89FA26AAC1ED883E3B8619A5616ECB69F77B719F595F008B0FCD0E961667295",
    },
    Pin {
        name: "de_mirage_2019",
        bytes: 46_520_604,
        sha256: "929F09470991A74DF58B8D41F806AB013034E3BF2D478C02EE884BA252143100",
    },
    Pin {
        name: "de_cbble_2018",
        bytes: 84_669_888,
        sha256: "EF4AE673B41125DB6E41D66D6CBE5BDE0713F924FDD17EA2071AA002BC52117A",
    },
];

pub(super) fn supported(name: &str) -> bool {
    PINS.iter().any(|pin| pin.name == name)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Image {
    Radar,
    Icon,
}

struct Images {
    radar: Vec<u8>,
    icon: Vec<u8>,
}

pub(super) struct ArchiveArt {
    pub map: &'static workshop::OldMap,
    images: OnceLock<Option<Images>>,
    observed_init: AtomicBool,
}

impl PartialEq for ArchiveArt {
    fn eq(&self, other: &Self) -> bool {
        self.map == other.map
    }
}
impl Eq for ArchiveArt {}

impl ArchiveArt {
    pub fn new(name: &str) -> Option<Self> {
        supported(name).then(|| Self {
            map: workshop::find(name).expect("supported pinned archive"),
            images: OnceLock::new(),
            observed_init: AtomicBool::new(false),
        })
    }

    pub fn url(&self, serial: u64, image: Image) -> String {
        let id = self.map.content_id();
        format!(
            "img://loading/{serial}/{}/{}/{}/{}",
            id.origin.prefix(),
            id.kind.as_str(),
            id.name,
            match image {
                Image::Radar => "radar",
                Image::Icon => "icon",
            }
        )
    }

    pub fn requested(&self, serial: u64, url: &str) -> Option<Image> {
        [Image::Radar, Image::Icon]
            .into_iter()
            .find(|image| self.url(serial, *image) == url)
    }

    pub fn image(&self, fs: &vfs::Vfs, image: Image) -> Option<Vec<u8>> {
        let images = self.images.get_or_init(|| self.load(fs)).as_ref()?;
        Some(
            match image {
                Image::Radar => &images.radar,
                Image::Icon => &images.icon,
            }
            .clone(),
        )
    }

    pub fn unavailable(&self) -> bool {
        matches!(self.images.get(), Some(None))
    }

    pub fn observe_init(&self) { self.observed_init.store(true, Ordering::Relaxed); }
    pub fn observed_init(&self) -> bool { self.observed_init.load(Ordering::Relaxed) }

    fn load(&self, fs: &vfs::Vfs) -> Option<Images> {
        let pin = PINS.iter().find(|pin| pin.name == self.map.name)?;
        let path = workshop::Store::current().bsp_path(self.map);
        let file = std::fs::File::open(path).ok()?;
        if file.metadata().ok()?.len() != pin.bytes as u64 {
            return None;
        }
        let mut bytes = Vec::with_capacity(pin.bytes);
        file.take(pin.bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() != pin.bytes || format!("{:X}", Sha256::digest(&bytes)) != pin.sha256 {
            return None;
        }
        let archive = embedded(&bytes)?;
        // This is the actual older overview, never the installed remake radar.
        // Some compatibility BSPs (Mirage, Vertigo, Cobble) only contain the
        // ordinary radar. Both variants belong to this verified older BSP.
        let spectate_path = format!("resource/overviews/{}_radar_spectate.dds", self.map.base);
        let normal_path = format!("resource/overviews/{}_radar.dds", self.map.base);
        let (radar_path, radar) = match archive.read(&spectate_path).ok()? {
            Some(radar) => (spectate_path, radar),
            None => (normal_path.clone(), archive.read(&normal_path).ok()??),
        };
        let radar = scaleform::loading_radar_png(&radar)?;
        // Collection emblems are addressed by the archive's explicit base map;
        // the map title/content identity remains the selected archive.
        let icon = fs.read(&format!(
            "resource/flash/images/map_icons/collection_icon_{}.png",
            self.map.base
        ))?;
        if icon.len() > 4 * 1024 * 1024 {
            return None;
        }
        bevy::log::info!(
            "Loading archive art: content={} generation-owned radar={} bsp_sha256={}",
            self.map.content_id(),
            radar_path,
            pin.sha256
        );
        Some(Images { radar, icon })
    }
}

fn embedded(bytes: &[u8]) -> Option<vfs::ZipArchive> {
    let header = bsp::lump::Header::parse(bytes).ok()?;
    if !(19..=21).contains(&header.version) {
        return None;
    }
    let info = header.lumps[bsp::lumps::PAKFILE];
    if info.version != 0 || info.four_cc != 0 {
        return None;
    }
    let start = usize::try_from(info.offset).ok()?;
    let size = usize::try_from(info.length).ok()?;
    if size == 0 || size > 64 * 1024 * 1024 {
        return None;
    }
    let end = start.checked_add(size)?;
    vfs::ZipArchive::from_bytes(bytes.get(start..end)?.to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_image_identity_requires_exact_content_generation_and_kind() {
        let dust = ArchiveArt::new("de_dust2_2017").unwrap();
        let inferno = ArchiveArt::new("de_inferno_2016").unwrap();
        let url = dust.url(42, Image::Radar);
        assert_eq!(url, "img://loading/42/csgo/map/de_dust2_2017/radar");
        assert_eq!(dust.requested(42, &url), Some(Image::Radar));
        assert_eq!(dust.requested(43, &url), None);
        assert_eq!(inferno.requested(42, &url), None);
        assert_eq!(dust.requested(42, &(url + "/../icon")), None);
        assert!(ArchiveArt::new("de_dust2").is_none());
        assert!(ArchiveArt::new("de_nuke_2016").is_some());
        assert!(ArchiveArt::new("de_nuke_2019").is_none());
        for archive in workshop::OLD_MAPS {
            assert_eq!(supported(archive.name), archive.menu_supported(), "{}", archive.name);
        }
    }

    #[test]
    #[ignore = "requires all admitted pinned classic BSPs in CSR_DATA_DIR and CS:GO Legacy"]
    fn installed_archives_decode_their_actual_embedded_radar_images() {
        // The frontend uses this exact mount path, including Store::current(),
        // rather than the movie navigator's separate install-only filesystem.
        let (_, fs) = level::mount_game(None).expect("frontend mount stack");
        let installed = PINS.iter().map(|pin| pin.name.to_owned())
            .collect();
        let modes = crate::frontend_game::load(&fs, &installed);
        for pin in PINS {
            assert!(
                fs.exists(&format!("maps/{}.bsp", pin.name)),
                "frontend BSP alias: {}",
                pin.name
            );
            assert!(
                modes
                    .block("maps")
                    .and_then(|maps| maps.block(pin.name))
                    .is_some(),
                "frontend card metadata alias: {}",
                pin.name
            );
            let art = ArchiveArt::new(pin.name).unwrap();
            let png = art
                .image(&fs, Image::Radar)
                .expect("verified embedded original radar");
            let image = image::load_from_memory(&png).unwrap();
            assert_eq!(
                (image.width(), image.height()),
                (1024, 1024),
                "{}",
                pin.name
            );
            let icon = art
                .image(&fs, Image::Icon)
                .expect("same base-map collection emblem");
            image::load_from_memory(&icon).expect("installed collection PNG");
        }
    }
}
