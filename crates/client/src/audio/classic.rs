//! Optional private historical weapon waves, with installed-file fallback.
//! Classic shots keep their declared normal pitch instead of modern resampling.
//! Historical scripts/assets are never embedded in this module.
mod historical;
mod handling;

use super::{Entry, decoded_audio};
use content::{ContentId, ContentKind};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoundEra {
    InstalledLegacy,
    #[default]
    Classic2015,
}

#[derive(Clone, Debug)]
pub(super) struct FrozenWave {
    pub bytes: Arc<[u8]>,
    pub channels: u16,
}

struct Selection {
    binding: &'static str,
    alias: &'static str,
}

#[derive(Default)]
pub(super) struct Profile {
    era: SoundEra,
    selections: HashMap<ContentId, Vec<Selection>>,
    historical_pairs: usize,
}

#[derive(Clone, Copy)]
struct WaveSpec<'a> {
    path: &'a str,
    size: usize,
    sha256: &'a str,
}

struct WeaponSpec<'a> {
    weapon: &'static str,
    binding: &'static str,
    alias: &'static str,
    near_wave: WaveSpec<'a>,
    distant: Option<DistantSpec<'a>>,
    mix: Option<ShotMix>,
}

struct DistantSpec<'a> {
    // None restores a historical partner absent from the installed declarations.
    binding: Option<&'static str>,
    alias: &'static str,
    wave: WaveSpec<'a>,
}

#[derive(Clone, Copy)]
struct ShotMix {
    volume: f32,
    falloff_max: Option<f32>,
}

impl ShotMix {
    const STOCK: Self = Self { volume: 1.0, falloff_max: Some(3937.0) };

    fn apply(self, entry: &mut Entry) {
        entry.volume = (self.volume, self.volume);
        if let Some(max) = self.falloff_max {
            // Literal CS_update_weapon defaults from the cached shipped 2015
            // stack. Keep the renderer's fitted curve/floor, not a new DSP model.
            let mut curve = entry.weapon_falloff.unwrap_or_default();
            curve.min = 400.0;
            curve.max = max;
            curve.curve_amount = 0.7;
            entry.weapon_falloff = Some(curve);
            entry.xfade_bounds = (800.0, 2800.0);
        }
    }
}

// Shot paths come from the published December 2015 sound declarations. Sizes
// and SHA256 pin the retained fallback files in Legacy app 4465480. The optional
// private pack has separate verified historical fingerprints in historical.rs.
// Some retained files differ from the 2015 directory CRC/size; those fallback
// approximations are only used when a complete historical pair is unavailable.
// The cached 2015 game_sounds_weapons and sound_operator_stacks declarations
// supply literal volumes/bounds only. Keep installed limits and the renderer's
// mixer/curve shape rather than transplanting the historical operator graph.
// Every selected historical near/distant declaration uses PITCH_NORM (or 100).
// Modern pitch changes belong to the modern waves: M4A4 uses 120, Dual Berettas
// 110, distant FAMAS 105 and several pistols add jitter. Applying those to the
// retained waves changes their identity/timing; the private reference clips
// corroborate normal pitch for M4A4, Glock and P250.
macro_rules! shot {
    ($weapon:literal, $binding:literal, $mode:literal,
        ($path:literal, $size:literal, $hash:literal)) => {
        WeaponSpec {
            weapon: $weapon, binding: $binding,
            alias: concat!("csr.classic2015.", $weapon, ".", $mode),
            near_wave: WaveSpec { path: $path, size: $size, sha256: $hash },
            distant: None,
            mix: Some(ShotMix::STOCK),
        }
    };
    ($weapon:literal, $binding:literal, $mode:literal,
        ($path:literal, $size:literal, $hash:literal),
        ($far_binding:literal, $far_path:literal, $far_size:literal, $far_hash:literal)) => {
        WeaponSpec {
            distant: Some(DistantSpec {
                binding: Some($far_binding),
                alias: concat!("csr.classic2015.", $weapon, ".", $mode, ".distant"),
                wave: WaveSpec { path: $far_path, size: $far_size, sha256: $far_hash },
            }),
            ..shot!($weapon, $binding, $mode, ($path, $size, $hash))
        }
    };
}

const STOCK: &[WeaponSpec<'static>] = &[
    shot!("deagle", "Weapon_DEagle.Single", "single",
        ("sound/weapons/deagle/deagle-1.wav", 394698, "c84c7a87cdeb04e106df58bf60137ee8eee70c91d3281c8c8c61618d9c99e5ef"),
        ("weapon_deagle.singledistant", "sound/weapons/deagle/deagle-1-distant.wav", 247540, "fb4ddf1c90471a5b7a39ac6da3e63387e6419643de45babd539c922d15d271ff")),
    shot!("elite", "Weapon_Elite.Single", "single",
        ("sound/weapons/elite/elite-1.wav", 348536, "37266a50da5e1ac7a05cfd71c07fa829ff325ff62d334b6b8673fe598124cc0d"),
        ("weapon_elite.singledistant", "sound/weapons/elite/elite-1-distant.wav", 220544, "cec759b3c54707bad9b5a9f6da4557cf6ddf0663023a4cf433ebb680b8d052af")),
    shot!("fiveseven", "Weapon_FiveSeven.Single", "single",
        ("sound/weapons/fiveseven/fiveseven-1.wav", 265728, "f4b9c20faa2d04b55376a627e9cd768fb4b7575c6652ab87f402f51eafdffed6"),
        ("weapon_fiveseven.singledistant", "sound/weapons/fiveseven/fiveseven-1-distant.wav", 220818, "7572b4b884f1695fc53d9f180bdcf355b556e24ee0d26fb957a58940c9c5cfc6")),
    shot!("glock", "Weapon_Glock.Single", "single",
        ("sound/weapons/glock18/glock18-1.wav", 195596, "ec0b1f71d4ff2a56df7c676cfa21b9825eaa9e68071e4e68ffe4b9762e981495"),
        ("weapon_glock.singledistant", "sound/weapons/glock18/glock18-1-distant.wav", 202972, "46f5909a39f11930199429969b5f14148d0590b0815d8817acd5935cd198c75f")),
    WeaponSpec { mix: Some(ShotMix { volume: 0.7, ..ShotMix::STOCK }),
        ..shot!("ak47", "Weapon_AK47.Single", "single",
        ("sound/weapons/ak47/ak47-1.wav", 288436, "a504312e586d54332bc42b533242bb7b57ba8704439138a624bf4edc18f71903"),
        ("weapon_ak47.singledistant", "sound/weapons/ak47/ak47-1-distant.wav", 203206, "260c3237643c74e4950ba2a96d1179228023f9e759235c43e4d1bc34f5ab95b9")) },
    shot!("aug", "Weapon_AUG.Single", "single",
        ("sound/weapons/aug/aug-1.wav", 407788, "b28184046c10ff5d717771a7e877efadc9f59bce4afc95c6b09fbf970cd346ca"),
        ("weapon_aug.singledistant", "sound/weapons/aug/aug-1-distant.wav", 257936, "b70c9151cbde7013dc4ae0ad796df760899b1a00745077270bb2ad1133c1b26e")),
    shot!("awp", "Weapon_AWP.Single", "single",
        ("sound/weapons/awp/awp1.wav", 728752, "c93fb4714540f2895b1c89764b13238b78f5e53a7fa6e9478ee8da2102409032"),
        ("weapon_awp.singledistant", "sound/weapons/awp/awp1-distant.wav", 203206, "13f376ef9821a9c73978cabbea9ebb1b1adbdfdb27f108c412f0949da1cec864")),
    shot!("famas", "Weapon_FAMAS.Single", "single",
        ("sound/weapons/famas/famas-1.wav", 296956, "f4900ec2a9da21d52add0a6796da8b61578aa24252cf2e72d28235e2bacb6284"),
        ("weapon_famas.singledistant", "sound/weapons/famas/famas-1-distant.wav", 187724, "a066cf93c84631abbc8689a90de4ea36531d2b46935dad455683ae62052252bd")),
    shot!("g3sg1", "Weapon_G3SG1.Single", "single",
        ("sound/weapons/g3sg1/g3sg1-1.wav", 331540, "62ad61182bf31b7175bf03eaac88628e7a1c5fdaa132f3e696739b8e64231e9d"),
        ("weapon_g3sg1.singledistant", "sound/weapons/g3sg1/g3sg1-1-distant.wav", 203206, "be2545cb1c42318da3ccd0f4c5a1d8f263fcbc442ffe586a6d4ed4b2d94da6b3")),
    shot!("galilar", "Weapon_GalilAR.Single", "single",
        ("sound/weapons/galilar/galil-1.wav", 350844, "7571bc91913f9c401665e2105bdbc1f1be0d13aac21a4602f710a660bd51c442"),
        ("weapon_galilar.singledistant", "sound/weapons/galilar/galil-1-distant.wav", 203206, "3b73bbdb891488701cc7f83738352f68e15d1f28b8231b4d8b58d2679664b64c")),
    shot!("m249", "Weapon_M249.Single", "single",
        ("sound/weapons/m249/m249-1.wav", 327362, "6a8b9f22545710dfa62ee25e34f37284764b79475c7d4b922965ed872f11d1c5"),
        ("weapon_m249.singledistant", "sound/weapons/m249/m249-1-distant.wav", 173070, "9ab6182f53d0bb80f69dda04ef31b269ce677277c76553a4805be0de95bb441d")),
    shot!("m4a1", "Weapon_M4A1.Single", "single",
        ("sound/weapons/m4a1/m4a1_unsil-1.wav", 211724, "9763983af9ef5951c541a14a6a9040d36ae2420026671c824369aaeb1226e793"),
        ("weapon_m4a4.singledistant", "sound/weapons/m4a1/m4a1_unsil-1-distant.wav", 203206, "6127e9d080862b16aec0ceca6c0e6431ac54f4f0fdf38f1c7787da0be7671b50")),
    shot!("mac10", "Weapon_MAC10.Single", "single",
        ("sound/weapons/mac10/mac10-1.wav", 253390, "650c21ad4cd3a47eb17d09c4564f5cd289adc1d934d2eb78e12fc4868d7ec888"),
        ("weapon_mac10.singledistant", "sound/weapons/mac10/mac10-1-distant.wav", 203642, "f908704dd628924472a0d031f3460cdb7ca54734ce30bfe7c7615a6406189738")),
    shot!("p90", "Weapon_P90.Single", "single",
        ("sound/weapons/p90/p90-1.wav", 248300, "0cbb602f0c4e686739061ea34b9ada773757bf0c3ce526251955243980ee087e"),
        ("weapon_p90.singledistant", "sound/weapons/p90/p90-1-distant.wav", 347676, "3b09ddebd9d1a9b1a522c836ce92a6a807a63b2c2fe4f7092bfe4a8eaf1b4c4a")),
    shot!("ump45", "Weapon_UMP45.Single", "single",
        ("sound/weapons/ump45/ump45-1.wav", 216090, "e5e1d3eec592dec89c88f897436962328ac6f5d7df3881328b5e9b776e823f7e"),
        ("weapon_ump45.singledistant", "sound/weapons/ump45/ump45-1-distant.wav", 426358, "d04a56725183eb44400a813d140f5ee907c728e3c3f4eaa893cda9bc24f0dcfe")),
    shot!("xm1014", "Weapon_XM1014.Single", "single",
        ("sound/weapons/xm1014/xm1014-1.wav", 565528, "c7ae55f1a8e916a5aa7e31ed30d3454198173726b66a0c6cd8e0c70dfffc9481"),
        ("weapon_xm1014.singledistant", "sound/weapons/xm1014/xm1014-1-distant.wav", 209520, "abde82d8e3d2db501d9a5877dc15ffe0186a514ea0dc3a5ef45a07f08d8ac92d")),
    shot!("bizon", "Weapon_bizon.Single", "single",
        ("sound/weapons/bizon/bizon-1.wav", 248300, "c91682a2ff6110caf6827fb3503d8215cfa8c7073500ecec18c701daa4d3403f"),
        ("weapon_bizon.singledistant", "sound/weapons/bizon/bizon-1-distant.wav", 203662, "c35795206604b4a06fc0ef48607178eeccf29170980a46ffd9a8db62038e5c29")),
    shot!("mag7", "Weapon_Mag7.Single", "single",
        ("sound/weapons/mag7/mag7-1.wav", 271872, "5e57ad339f0fbead233f2aa8ee215c313850b3c0fb827ce1b6ee7f7d4030a021"),
        ("weapon_mag7.singledistant", "sound/weapons/mag7/mag7-1-distant.wav", 203206, "82101e5e1f55c55dbc3d6203aea1f65fa23232b1257b6ccbd42ff298bba6a898")),
    shot!("negev", "Weapon_Negev.Single", "single",
        ("sound/weapons/negev/negev-1.wav", 327870, "2f5bb19f4f9fe7532a26bb1fe6fcf991bf2f5172bf86db4a2621d76b64ec97de"),
        ("weapon_negev.singledistant", "sound/weapons/negev/negev-1-distant.wav", 203204, "4f1f291ba6a2be4037e0495fe88a68e53439bc02dd08764fe912eedebc83e48b")),
    shot!("sawedoff", "Weapon_Sawedoff.Single", "single",
        ("sound/weapons/sawedoff/sawedoff-1.wav", 495416, "cad0dee904fae92ffa6d9bf1b719abcb1b78723c542c197292de38e11b7fbc24"),
        ("weapon_sawedoff.singledistant", "sound/weapons/sawedoff/sawedoff-1-distant.wav", 295946, "d3ba917629786c623f9a2f543be3d061ea95610f94934b58c93b213b1993857e")),
    shot!("tec9", "Weapon_tec9.Single", "single",
        ("sound/weapons/tec9/tec9-1.wav", 270416, "a2dd44715b23d2d152fec60b203d930efc254d264836982b504b376e07692365"),
        ("weapon_tec9.singledistant", "sound/weapons/tec9/tec9-1-distant.wav", 203206, "fd909ebda3a5a2d512d6701808161cd749df79353f064c70c6bb5390a6a511d6")),
    WeaponSpec { mix: Some(ShotMix { volume: 1.0, falloff_max: None }),
        ..shot!("taser", "Weapon_Taser.Single", "single",
        ("sound/weapons/taser/taser_shoot.wav", 257448, "13125cdb85f84ab71ed3921fa55977288dfcafb8e36a414a83b1420468586748")) },
    shot!("hkp2000", "Weapon_hkp2000.Single", "single",
        ("sound/weapons/hkp2000/hkp2000-1.wav", 371628, "cb2583cd776030758225310911cf1ab18f6540dfa2508a968469f1c8c03d7241"),
        ("weapon_hkp2000.singledistant", "sound/weapons/hkp2000/hkp2000-1-distant.wav", 290358, "d21e30b500130bc0dce69e91083b63280a6d877d885d40e1dacba38b33e5f400")),
    shot!("mp7", "Weapon_MP7.Single", "single",
        ("sound/weapons/mp7/mp7-1.wav", 332074, "eaf15cfa8e59169ca9576e730965775eec1a7775277b19d286b165b013aa491c"),
        ("weapon_mp7.singledistant", "sound/weapons/mp7/mp7-1-distant.wav", 203206, "fbbdfc95fdca54f4b8bf108c009b70955b56529c726d6189db5275f7e6e8ca1f")),
    shot!("mp9", "Weapon_MP9.Single", "single",
        ("sound/weapons/mp9/mp9-1.wav", 161420, "218880b4f192ccf29306beb0f4d3c68cf34e87d3232b17f033bbdf3defac9852"),
        ("weapon_mp9.singledistant", "sound/weapons/mp9/mp9-1-distant.wav", 203206, "2ab5164dbf59674f6363ad4a5a0434318cee2020d778d62e0e51316fb1f60dcc")),
    shot!("nova", "Weapon_Nova.Single", "single",
        ("sound/weapons/nova/nova-1.wav", 505264, "d30659b4cb5aa1a6f72d292f9a4a568b59671ee9b96bac3afa63d4a600c7b6b6"),
        ("weapon_nova.singledistant", "sound/weapons/nova/nova-1-distant.wav", 228812, "5d9087a4c6c03e83e708519e36eb9670d89187f8f52af0f72fba44d2154e79bd")),
    shot!("p250", "Weapon_P250.Single", "single",
        ("sound/weapons/p250/p250-1.wav", 206732, "01b10acbcb7d48e104a5c973599a623a2d8f3d93cb1c4f50d535c1176e2dd363"),
        ("weapon_p250.singledistant", "sound/weapons/p250/p250-1-distant.wav", 203206, "bad8ddf7f4d0f7063a0cabd6996081e01c55a20e818c230177272988210e823f")),
    shot!("scar20", "Weapon_scar20.Single", "single",
        ("sound/weapons/scar20/scar20_unsil-1.wav", 302274, "89fc663556fe372bdafd97481537a5fc3803ae9de58f2e535d9cba71923a3b25"),
        ("weapon_scar20.singledistant", "sound/weapons/scar20/scar20_unsil-1-distant.wav", 203206, "4b5b6c1b385c20fa4bbe82022deadf5b09540a2324e6e9d51d4d1f3d37d35f2a")),
    shot!("sg556", "Weapon_SG556.Single", "single",
        ("sound/weapons/sg556/sg556-1.wav", 397542, "e3dfef911aeb36b1816e23c23f4891c82fbaef80f76e8d0bfecabd5f770509b3"),
        ("weapon_sg556.singledistant", "sound/weapons/sg556/sg556-1-distant.wav", 203206, "0dec8def6fd8fbc7c1ed608bede5f545369070bd103aff3acb188a28ea52add0")),
    shot!("ssg08", "Weapon_SSG08.Single", "single",
        ("sound/weapons/ssg08/ssg08-1.wav", 252128, "33547c45fb5ddfb7057fe6dc97c693bda9274181940ec7756361fea76ece88c8"),
        ("weapon_ssg08.singledistant", "sound/weapons/ssg08/ssg08-1-distant.wav", 203206, "711541994cc93290732d7ec72abf11b6acce91337f087e807c62b5b08c4fe94b")),
    shot!("m4a1_silencer", "Weapon_M4A1.SilencedSingle", "unsilenced",
        ("sound/weapons/m4a1/m4a1-1-single.wav", 247626, "c05556215073c7a6a9243efe517efdf08362dd22ca89a357cb6dc379309026c3"),
        ("weapon_m4a1.usdistant", "sound/weapons/m4a1/m4a1-1-distant.wav", 23122, "fe23d5c351f5b095437c4326a715693cd380c8e81b1f0f8fa2793a2b9970f80c")),
    WeaponSpec {
        mix: Some(ShotMix { falloff_max: Some(1377.0), ..ShotMix::STOCK }),
        distant: Some(DistantSpec {
            binding: None,
            alias: "csr.classic2015.m4a1_silencer.silenced.distant",
            wave: WaveSpec {
                path: "sound/weapons/m4a1/m4a1-1-distant.wav",
                size: 23122,
                sha256: "fe23d5c351f5b095437c4326a715693cd380c8e81b1f0f8fa2793a2b9970f80c",
            },
        }),
        ..shot!("m4a1_silencer", "Weapon_M4A1.Silenced", "silenced",
        ("sound/weapons/m4a1/m4a1-1.wav", 44438, "8548c3b0937bf07656c73021439b76a946a4a8bb339b208508af6e916f072195"))
    },
    shot!("usp_silencer", "Weapon_USP.Single", "unsilenced",
        ("sound/weapons/usp/usp_unsil-1.wav", 196608, "3176347649f491669d715dd325eb40a6f69884245af7c3757cbf18d6d07e510f"),
        ("weapon_usp.singledistant", "sound/weapons/usp/usp_unsil-1-distant.wav", 173596, "a4605b061f28a9101a0d3f265377d35dee290808135c2ae8b9e7f03884689cfb")),
    WeaponSpec { mix: Some(ShotMix { falloff_max: Some(1378.0), ..ShotMix::STOCK }),
        ..shot!("usp_silencer", "Weapon_USP.SilencedShot", "silenced",
        ("sound/weapons/usp/usp1.wav", 45830, "7ec5f84a2c36ea263b71571b7ee548833fd66dca8e81336032e0214084fa5c0a")) },
    shot!("cz75a", "Weapon_CZ75A.Single", "single",
        ("sound/weapons/cz75a/cz75a-1.wav", 247542, "015a8969485640892238a8c29d978b4412483011a3cb48e71ac43295d3b7d77b"),
        ("weapon_cz75a.singledistant", "sound/weapons/cz75a/cz75a-1-distant.wav", 191940, "9b5d41885f0031c73c626f978cd279e115cd2f3c65e56c95251b57c7c80820ad")),
    shot!("revolver", "Weapon_Revolver.Single", "single",
        ("sound/weapons/revolver/revolver-1_01.wav", 617520, "a6305095619ba8e9aae31c6cc9e64917b1bb7e40bd6dd59a5129e27524e3c31d"),
        ("weapon_revolver.singledistant", "sound/weapons/revolver/revolver-1_distant.wav", 308820, "02a8b6c42b1b18529ed2c6f535da4263a649aad0f7c736f71f6fcd84753c6276")),
];

fn verified_wave(fs: &vfs::Vfs, spec: &WaveSpec<'_>) -> Result<FrozenWave, &'static str> {
    let bytes = fs.read(spec.path).ok_or("retained waveform missing")?;
    if bytes.len() != spec.size {
        return Err("retained waveform size differs");
    }
    if format!("{:x}", Sha256::digest(&bytes)) != spec.sha256 {
        return Err("retained waveform fingerprint differs");
    }
    let (bytes, channels) = decoded_audio(bytes).ok_or("retained waveform is unsupported")?;
    Ok(FrozenWave {
        bytes: Arc::from(bytes),
        channels,
    })
}

impl Profile {
    pub(super) fn load(fs: &vfs::Vfs, entries: &mut HashMap<String, Entry>, era: SoundEra) -> Self {
        let mut profile = Self {
            era,
            ..Default::default()
        };
        if era == SoundEra::Classic2015 {
            // Resource-only packs are explicitly opted into by this profile.
            // Never mount them into the world's VFS or read them in modern mode.
            let historical = historical::Historical::open(
                &packs::Installed::current().dir().join(historical::FILENAME),
            );
            for spec in STOCK {
                if let Err(reason) = profile.install_with_historical(
                    fs, entries, spec, historical.as_ref(),
                ) {
                    // Fixed identifiers/reasons, at most one diagnostic per stock state.
                    bevy::log::warn!(
                        "Classic2015 {} unavailable: {}; using installed shot",
                        spec.weapon,
                        reason
                    );
                }
            }
            if let Some(historical) = &historical {
                handling::install(fs, entries, historical);
            }
            bevy::log::info!(
                "Classic2015 shots: {} historical pairs, {} retained pairs",
                profile.historical_pairs,
                profile.selections.values().map(Vec::len).sum::<usize>() - profile.historical_pairs,
            );
        }
        profile
    }

    #[cfg(test)]
    fn install(
        &mut self,
        fs: &vfs::Vfs,
        entries: &mut HashMap<String, Entry>,
        spec: &WeaponSpec<'_>,
    ) -> Result<(), &'static str> {
        self.install_with_historical(fs, entries, spec, None)
    }

    fn install_with_historical(
        &mut self,
        fs: &vfs::Vfs,
        entries: &mut HashMap<String, Entry>,
        spec: &WeaponSpec<'_>,
        historical: Option<&historical::Historical>,
    ) -> Result<(), &'static str> {
        if entries.contains_key(spec.alias)
            || spec
                .distant
                .as_ref()
                .is_some_and(|d| entries.contains_key(d.alias))
        {
            return Err("profile entry name already declared");
        }
        let mut near = entries
            .get(&spec.binding.to_ascii_lowercase())
            .cloned()
            .ok_or("installed shot entry missing")?;
        if near.distant.as_deref() != spec.distant.as_ref().and_then(|d| d.binding) {
            return Err("installed distant binding differs");
        }
        // Validate the complete pair before publishing anything. Never mix a
        // classic near shot with a modern distant waveform after a partial failure.
        let (near_audio, far_audio, from_historical) = match historical.and_then(|pack| pack.pair(spec).ok()) {
            Some((near, far)) => (near, far, true),
            None => {
                let near = verified_wave(fs, &spec.near_wave)?;
                let far = spec.distant.as_ref()
                    .map(|far| verified_wave(fs, &far.wave))
                    .transpose()?;
                (near, far, false)
            }
        };
        let distant = if let Some(far) = &spec.distant {
            let mut distant = if let Some(binding) = far.binding {
                entries.get(binding).cloned().ok_or("installed distant entry missing")?
            } else {
                // Suppressed M4's historical partner has verified retained bytes
                // but no modern entry. Reuse the weapon voice shape, then bind
                // its declared far literals; do not borrow another gun's mixer.
                let mut entry = near.clone();
                entry.soundlevel = 94.0;
                entry.limit = None; // Historical partner has no start stack.
                entry
            };
            distant.waves = vec![far.wave.path.to_string()];
            distant.frozen_wave = far_audio;
            distant.pitch = (100.0, 100.0);
            if spec.mix.is_some() {
                ShotMix::STOCK.apply(&mut distant);
            }
            distant.explicit_near_fade = false;
            distant.distant = None;
            distant.distant_layer = true;
            Some((far.alias, distant))
        } else {
            None
        };
        near.waves = vec![spec.near_wave.path.to_string()];
        near.frozen_wave = Some(near_audio);
        near.pitch = (100.0, 100.0);
        if let Some(mix) = spec.mix {
            mix.apply(&mut near);
            if mix.falloff_max.is_some() {
                near.explicit_near_fade = true;
            }
        }
        near.distant = distant.as_ref().map(|(alias, _)| alias.to_string());
        entries.insert(spec.alias.to_string(), near);
        if let Some((alias, entry)) = distant {
            entries.insert(alias.to_string(), entry);
        }
        self.selections
            .entry(ContentId::csgo(ContentKind::Weapon, spec.weapon))
            .or_default()
            .push(Selection { binding: spec.binding, alias: spec.alias });
        self.historical_pairs += usize::from(from_historical);
        Ok(())
    }

    pub(super) fn era(&self) -> SoundEra {
        self.era
    }

    pub(super) fn select(&self, id: &ContentId, binding: &str) -> Option<&str> {
        self.selections
            .get(id)?
            .iter()
            .find(|entry| entry.binding.eq_ignore_ascii_case(binding))
            .map(|entry| entry.alias)
    }

    /// Bounded local AK balance from its published 2015 declaration: volume
    /// 0.7 with volume_accumulate input3 = 0.55 replacing the mixer input.
    /// Same-wave native PCM supports lowering the modern 0.9 local response;
    /// this declared response is a playable approximation, not a calibrated
    /// speaker-output match. Do not multiply another FirstWeapons mixer gain.
    /// This replaces the local gain rather than multiplying the authored volume;
    /// world shots/distant layers use their own literals, feedback stays separate.
    pub(super) fn local_gain(&self, entry: &str) -> Option<f32> {
        let alias = self.select(
            &ContentId::csgo(ContentKind::Weapon, "ak47"),
            "Weapon_AK47.Single",
        )?;
        (entry == alias).then_some(0.7 * 0.55)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::GameOrigin;
    use std::collections::BTreeMap;

    pub(super) fn wav(sample: i16) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&38u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&44100u32.to_le_bytes());
        bytes.extend_from_slice(&88200u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&sample.to_le_bytes());
        bytes
    }

    fn entries() -> HashMap<String, Entry> {
        let doc = kv::parse(
            r#"
            "Test.Single" { "wave" "weapons/test/current.wav" "volume" "0.9"
                "operator_stacks" { "start_stack" { "import_stack" "CS_distance_weapon_start"
                    "play_distant_version_1" { "entry_name" "Test.Distant" } }
                    "update_stack" { "import_stack" "CS_update_weapon" } } }
            "Test.Distant" { "wave" "weapons/test/current_distant.wav" "volume" "0.2" }
        "#,
        )
        .unwrap();
        doc.iter()
            .filter_map(|(name, value)| {
                value
                    .as_block()
                    .and_then(super::super::parse_entry)
                    .map(|entry| (name.to_ascii_lowercase(), entry))
            })
            .collect()
    }

    fn spec<'a>(near: &'a str, distant: &'a str) -> WeaponSpec<'a> {
        WeaponSpec {
            weapon: "ak47",
            binding: "Test.Single",
            alias: "csr.test.near",
            near_wave: WaveSpec {
                path: "sound/old_near.wav",
                size: 46,
                sha256: near,
            },
            distant: Some(DistantSpec {
                binding: Some("test.distant"),
                alias: "csr.test.distant",
                wave: WaveSpec {
                    path: "sound/old_distant.wav",
                    size: 46,
                    sha256: distant,
                },
            }),
            mix: None,
        }
    }

    fn fs(near: Vec<u8>, distant: Vec<u8>) -> vfs::Vfs {
        let mut fs = vfs::Vfs::new();
        fs.push_back(
            "synthetic",
            vfs::Mount::Memory(BTreeMap::from([
                ("sound/old_near.wav".into(), Arc::from(near)),
                ("sound/old_distant.wav".into(), Arc::from(distant)),
            ])),
        );
        fs
    }

    pub(super) fn private_pack(near: &[u8], far: Option<&[u8]>, origin: &str, wrong_id: bool) -> packs::Pack {
        let mut inputs = vec![("sound/old_near.wav", near)];
        if let Some(far) = far {
            inputs.push(("sound/old_distant.wav", far));
        }
        let resources: Vec<_> = inputs.iter().map(|(path, _)| packs::ResourceDef {
            id: ContentId::new(
                GameOrigin::from_prefix(origin), ContentKind::Sound,
                if wrong_id { "wrong" } else { path.strip_prefix("sound/").unwrap() },
            ).to_string(),
            path: path.to_string(),
        }).collect();
        let manifest = packs::Manifest {
            format_version: 1,
            id: "classic_weapon_audio_2015".into(),
            name: "Synthetic historical audio".into(),
            version: "1.0.0".into(),
            origin: origin.into(),
            kind: packs::PackKind::Assets,
            provides: resources.iter().map(|r| r.id.clone()).collect(),
            content_hash: String::new(),
            min_game_version: "0.0.0".into(),
            description: String::new(),
            maps: Vec::new(),
            resources,
        };
        packs::Pack::from_bytes(packs::build(&manifest, &inputs).unwrap()).unwrap()
    }

    #[test]
    fn private_historical_pair_replaces_both_retained_waves_without_mutating_public_entries() {
        let old_near = wav(7000);
        let old_far = wav(-8000);
        let near = wav(1000);
        let far = wav(-2000);
        let old_near_hash = format!("{:x}", Sha256::digest(&old_near));
        let old_far_hash = format!("{:x}", Sha256::digest(&old_far));
        let historical_spec = spec(&old_near_hash, &old_far_hash);
        let historical = historical::Historical::from_pack(
            private_pack(&old_near, Some(&old_far), "classic2015", false),
            &[historical_spec.near_wave, historical_spec.distant.unwrap().wave],
        ).unwrap();
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let far_hash = format!("{:x}", Sha256::digest(&far));
        let mut entries = entries();
        let mut profile = Profile::default();
        profile.install_with_historical(
            &fs(near, far), &mut entries, &spec(&near_hash, &far_hash), Some(&historical),
        ).unwrap();
        assert_eq!(entries["csr.test.near"].frozen_wave.as_ref().unwrap().bytes.as_ref(), old_near);
        assert_eq!(entries["csr.test.distant"].frozen_wave.as_ref().unwrap().bytes.as_ref(), old_far);
        assert!(entries["test.single"].frozen_wave.is_none());
        assert!(entries["test.distant"].frozen_wave.is_none());
        assert_eq!(profile.select(&ContentId::csgo(ContentKind::Weapon, "ak47"), "Test.Single"), Some("csr.test.near"));
        assert_eq!(entries["csr.test.near"].pitch, (100.0, 100.0));
        assert_eq!(entries["csr.test.distant"].pitch, (100.0, 100.0));
        assert_eq!(profile.historical_pairs, 1);
    }

    #[test]
    #[ignore = "requires the private verified 2015 audio pack in CSR_DATA_DIR/packs and Legacy installed data"]
    fn installed_private_handling_respects_pack_contents_and_sound_era() {
        let path = packs::Installed::current().dir().join(historical::FILENAME);
        let pack = path.is_file().then(|| historical::Historical::open(&path).expect("private pack"));
        let (_, fs) = level::mount_game(None).unwrap();
        let installed = super::super::SoundLibrary::load_with_era(&fs, SoundEra::InstalledLegacy);
        let mag_ready = pack.as_ref().is_some_and(|pack|
            pack.wave("sound/weapons/mag7/mag7_pump_back.wav").is_ok()
                && pack.wave("sound/weapons/mag7/mag7_pump_forward.wav").is_ok());
        for era in [SoundEra::Classic2015, SoundEra::InstalledLegacy] {
            let library = super::super::SoundLibrary::load_with_era(&fs, era);
            for (binding, path) in [
                ("weapon_nova.pumpduringshot", "sound/weapons/nova/nova_pump.wav"),
                ("weapon_nova.pump", "sound/weapons/nova/nova_pump.wav"),
                ("weapon_mag7.pumpback", "sound/weapons/mag7/mag7_pump_back.wav"),
                ("weapon_mag7.pumpforward", "sound/weapons/mag7/mag7_pump_forward.wav"),
            ] {
                let expected = (era == SoundEra::Classic2015
                    && (!binding.starts_with("weapon_mag7.") || mag_ready))
                    .then(|| pack.as_ref().and_then(|pack| pack.wave(path).ok())).flatten();
                let actual = library.entries[binding].frozen_wave.as_ref();
                let classic_active = expected.is_some();
                match expected {
                    Some(wave) => assert_eq!(actual.expect("classic handling activation").bytes, wave.bytes),
                    None => assert!(actual.is_none(), "installed handling must remain available"),
                }
                let mut definition = library.entries[binding].clone();
                definition.frozen_wave = None;
                let mut expected_definition = installed.entries[binding].clone();
                if classic_active {
                    let volume = if binding == "weapon_nova.pumpduringshot" { 0.3 } else { 1.0 };
                    expected_definition.volume = (volume, volume);
                    expected_definition.pitch = (100.0, 100.0);
                    expected_definition.channel = 3;
                    expected_definition.limit = None;
                }
                assert_eq!(format!("{definition:?}"), format!("{expected_definition:?}"),
                    "handling metadata must follow the admitted wave era: {binding}");
            }
        }
    }

    #[test]
    #[ignore = "requires the private verified 2015 audio pack in CSR_DATA_DIR/packs and Legacy installed data"]
    fn installed_private_rifle_handling_uses_original_waves_and_declarations() {
        let path = packs::Installed::current().dir().join(historical::FILENAME);
        let pack = path.is_file().then(|| historical::Historical::open(&path).expect("private pack"));
        let (_, fs) = level::mount_game(None).unwrap();
        let installed = super::super::SoundLibrary::load_with_era(&fs, SoundEra::InstalledLegacy);
        let m4 = [
            ("weapon_m4a1.draw", "sound/weapons/m4a1/m4a1_draw.wav", 0.5, 6),
            ("weapon_m4a1.clipout", "sound/weapons/m4a1/m4a1_clipout.wav", 1.0, 3),
            ("weapon_m4a1.clipin", "sound/weapons/m4a1/m4a1_clipin.wav", 1.0, 3),
            ("weapon_m4a1.boltforward", "sound/weapons/m4a1/m4a1_boltforward.wav", 1.0, 3),
            ("weapon_m4a1.boltback", "sound/weapons/m4a1/m4a1_boltback.wav", 1.0, 3),
        ];
        let ak = [
            ("weapon_ak47.draw", "sound/weapons/ak47/ak47_draw.wav", 0.5, 6),
            ("weapon_ak47.boltpull", "sound/weapons/ak47/ak47_boltpull.wav", 1.0, 6),
            ("weapon_ak47.clipout", "sound/weapons/ak47/ak47_clipout.wav", 1.0, 3),
            ("weapon_ak47.clipin", "sound/weapons/ak47/ak47_clipin.wav", 1.0, 3),
        ];
        let m4_ready = pack.as_ref().is_some_and(|pack| m4.iter().all(|(_, path, _, _)| pack.wave(path).is_ok()));
        for era in [SoundEra::Classic2015, SoundEra::InstalledLegacy] {
            let library = super::super::SoundLibrary::load_with_era(&fs, era);
            for (binding, path, volume, channel) in ak.into_iter().chain(m4) {
                let active = era == SoundEra::Classic2015 && pack.is_some()
                    && (binding.starts_with("weapon_ak47.") || m4_ready);
                let mut actual = library.entries[binding].clone();
                let mut expected = installed.entries[binding].clone();
                if active {
                    let expected_wave = if binding.starts_with("weapon_ak47.") {
                        decoded_audio(fs.read(path).unwrap()).unwrap().0
                    } else {
                        pack.as_ref().unwrap().wave(path).unwrap().bytes
                    };
                    assert_eq!(actual.frozen_wave.take().expect("classic rifle handling").bytes, expected_wave, "{binding}");
                    expected.volume = (volume, volume);
                    expected.pitch = (100.0, 100.0);
                    expected.channel = channel;
                    expected.limit = None;
                }
                assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{binding}: {era:?}");
            }
            for binding in ["weapon_m4a1s.boltback", "weapon_m4a1s.boltforward", "weapon_m4a1.cliphit"] {
                assert_eq!(format!("{:?}", library.entries[binding]), format!("{:?}", installed.entries[binding]));
            }
        }
    }

    #[test]
    #[ignore = "requires the private verified 2015 audio pack in CSR_DATA_DIR/packs and Legacy installed data"]
    fn installed_private_historical_pack_decodes_and_drives_the_classic_profile() {
        let path = packs::Installed::current().dir().join(historical::FILENAME);
        let pack = historical::Historical::open(&path).expect("private historical pack");
        let (_, fs) = level::mount_game(None).unwrap();
        let mut library = super::super::SoundLibrary::load_with_era(&fs, SoundEra::InstalledLegacy);
        let mut profile = Profile::default();
        for spec in STOCK {
            profile.install_with_historical(&fs, &mut library.entries, spec, Some(&pack)).unwrap();
            if spec.weapon != "revolver" {
                let (expected, far) = pack.pair(spec).unwrap();
                assert_eq!(library.entries[spec.alias].frozen_wave.as_ref().unwrap().bytes, expected.bytes);
                if let Some(distant) = &spec.distant {
                    assert_eq!(library.entries[distant.alias].frozen_wave.as_ref().unwrap().bytes, far.unwrap().bytes);
                }
            }
        }
        assert_eq!(profile.historical_pairs, STOCK.len() - 1);
        assert_eq!(profile.selections.values().map(Vec::len).sum::<usize>(), STOCK.len());
        println!("Historical pack decoded and selected for {} weapon modes; revolver retained", profile.historical_pairs);
    }

    #[test]
    fn absent_private_pack_keeps_verified_retained_pair() {
        let missing = std::env::temp_dir().join(format!(
            "csr-classic-absent-{}", std::process::id(),
        )).join(historical::FILENAME);
        assert!(!missing.exists());
        let historical = historical::Historical::open(&missing);
        assert!(historical.is_none());
        let near = wav(1000);
        let far = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let far_hash = format!("{:x}", Sha256::digest(&far));
        let mut entries = entries();
        Profile::default().install_with_historical(
            &fs(near.clone(), far.clone()), &mut entries,
            &spec(&near_hash, &far_hash), historical.as_ref(),
        ).unwrap();
        assert_eq!(entries["csr.test.near"].frozen_wave.as_ref().unwrap().bytes.as_ref(), near);
        assert_eq!(entries["csr.test.distant"].frozen_wave.as_ref().unwrap().bytes.as_ref(), far);
    }

    #[test]
    fn corrupt_or_missing_private_far_falls_back_as_a_complete_retained_pair() {
        let old_near = wav(7000);
        let old_far = wav(-8000);
        let corrupt_far = wav(-7999);
        let near = wav(1000);
        let far = wav(-2000);
        let old_near_hash = format!("{:x}", Sha256::digest(&old_near));
        let old_far_hash = format!("{:x}", Sha256::digest(&old_far));
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let far_hash = format!("{:x}", Sha256::digest(&far));
        for private_far in [Some(corrupt_far.as_slice()), None] {
            let historical_spec = spec(&old_near_hash, &old_far_hash);
            let historical = historical::Historical::from_pack(
                private_pack(&old_near, private_far, "classic2015", false),
                &[historical_spec.near_wave, historical_spec.distant.unwrap().wave],
            ).unwrap();
            let mut entries = entries();
            Profile::default().install_with_historical(
                &fs(near.clone(), far.clone()), &mut entries,
                &spec(&near_hash, &far_hash), Some(&historical),
            ).unwrap();
            assert_eq!(entries["csr.test.near"].frozen_wave.as_ref().unwrap().bytes.as_ref(), near);
            assert_eq!(entries["csr.test.distant"].frozen_wave.as_ref().unwrap().bytes.as_ref(), far);
        }
    }

    #[test]
    fn valid_pack_with_wrong_namespace_or_resource_identity_is_refused() {
        let near = wav(1000);
        let hash = format!("{:x}", Sha256::digest(&near));
        let spec = spec(&hash, "");
        for (origin, wrong_id) in [("unrelated", false), ("classic2015", true)] {
            let pack = private_pack(&near, None, origin, wrong_id);
            assert!(historical::Historical::from_pack(pack, &[WaveSpec {
                path: spec.near_wave.path, size: spec.near_wave.size, sha256: &hash,
            }]).is_err());
        }
    }

    #[test]
    fn both_suppressor_states_keep_their_own_verified_selection() {
        let near = wav(1000);
        let far = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let far_hash = format!("{:x}", Sha256::digest(&far));
        let fs = fs(near, far);
        let mut entries = entries();
        let mut suppressed = entries["test.single"].clone();
        suppressed.distant = None;
        entries.insert("test.suppressed".into(), suppressed);
        let mut profile = Profile::default();
        let mut plain = spec(&near_hash, &far_hash);
        plain.weapon = "usp_silencer";
        profile.install(&fs, &mut entries, &plain).unwrap();
        let mut suppressed = spec(&near_hash, &far_hash);
        suppressed.weapon = "usp_silencer";
        suppressed.binding = "Test.Suppressed";
        suppressed.alias = "csr.test.suppressed";
        suppressed.distant = None;
        profile.install(&fs, &mut entries, &suppressed).unwrap();
        let id = ContentId::csgo(ContentKind::Weapon, "usp_silencer");
        assert_eq!(profile.select(&id, "Test.Single"), Some("csr.test.near"));
        assert_eq!(profile.select(&id, "Test.Suppressed"), Some("csr.test.suppressed"));
        assert_eq!(entries["csr.test.near"].distant.as_deref(), Some("csr.test.distant"));
        assert!(entries["csr.test.suppressed"].distant.is_none());
        assert_eq!(profile.select(&id, "Custom.Suppressed"), None);
    }

    #[test]
    fn classic_ak_local_balance_survives_authored_world_volume() {
        use bevy::{audio::AudioSource, ecs::world::CommandQueue, prelude::*};
        use super::super::{Emit, Emitter, SoundGain, SoundLibrary};
        let near = wav(1000);
        let far = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let far_hash = format!("{:x}", Sha256::digest(&far));
        let fs = fs(near, far);
        let mut entries = entries();
        // Keep the public shot playable without adding another VFS fixture.
        let public = entries.get_mut("test.single").unwrap();
        public.frozen_wave = Some(verified_wave(&fs, &spec(&near_hash, &far_hash).near_wave).unwrap());
        let mut ak = entries["test.single"].clone();
        ak.frozen_wave = None;
        entries.insert("weapon_ak47.single".into(), ak);
        let mut profile = Profile::default();
        let mut spec = spec(&near_hash, &far_hash);
        spec.binding = "Weapon_AK47.Single";
        spec.mix = STOCK.iter().find(|s| s.weapon == "ak47").unwrap().mix;
        profile.install(&fs, &mut entries, &spec).unwrap();
        let mut library = SoundLibrary { entries, classic: profile, ..Default::default() };
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut sources = Assets::<AudioSource>::default();
        {
            let mut commands = Commands::new(&mut queue, &world);
            for (entry, emit) in [
                ("csr.test.near", Emit { owner: Some(1), local: true, at: None }),
                ("csr.test.near", Emit { owner: Some(2), local: false, at: None }),
                ("test.single", Emit { owner: Some(3), local: true, at: None }),
                ("csr.test.near", Emit { owner: Some(4), local: false, at: Some(glam::Vec3::X * 100.0) }),
            ] {
                library.play_with(&mut commands, &fs, &mut sources, entry, emit);
            }
        }
        queue.apply(&mut world);
        let voice = |owner| library.playing.iter()
            .find(|p| p.owner == owner && p.entry != "csr.test.distant").unwrap().entity;
        assert!((world.get::<SoundGain>(voice(1)).unwrap().0 - 0.385).abs() < 1e-6);
        assert_eq!(world.get::<SoundGain>(voice(2)).unwrap().0, 0.7);
        assert_eq!(world.get::<SoundGain>(voice(3)).unwrap().0, 0.9);
        let emitter = world.get::<Emitter>(voice(4)).unwrap();
        assert_eq!(emitter.base, 0.7 * super::super::Category::Weapon.field());
        assert!(emitter.gunfire.is_some());
        assert_eq!(library.entries["weapon_ak47.single"].volume, (0.9, 0.9));
        assert_eq!(library.entries["csr.test.distant"].volume, (1.0, 1.0));
        assert_eq!(Profile::default().local_gain("csr.test.near"), None);
    }

    #[test]
    fn historical_m4_pair_restores_literals_atomically_without_a_modern_far_entry() {
        let stock = STOCK.iter().find(|s| s.binding == "Weapon_M4A1.Silenced").unwrap();
        let far = stock.distant.as_ref().unwrap();
        assert!(far.binding.is_none());
        assert_eq!(far.wave.path, "sound/weapons/m4a1/m4a1-1-distant.wav");
        let near = wav(1000);
        let distant = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let distant_hash = format!("{:x}", Sha256::digest(&distant));
        let fixture = spec(&near_hash, &distant_hash);
        let spec = WeaponSpec {
            weapon: stock.weapon, binding: "Test.Single", alias: stock.alias,
            mix: stock.mix, near_wave: fixture.near_wave,
            distant: Some(DistantSpec {
                binding: far.binding, alias: far.alias,
                wave: fixture.distant.unwrap().wave,
            }),
        };
        for corrupt_far in [false, true] {
            let fs = fs(near.clone(), if corrupt_far { wav(-1999) } else { distant.clone() });
            let mut entries = entries();
            entries.remove("test.distant");
            let installed = entries.get_mut("test.single").unwrap();
            installed.distant = None;
            installed.weapon_falloff.as_mut().unwrap().max = 800.0;
            installed.weapon_falloff.as_mut().unwrap().curve_amount = 0.9;
            installed.xfade_bounds = (800.0, 1500.0);
            let mut profile = Profile::default();
            assert_eq!(profile.install(&fs, &mut entries, &spec).is_ok(), !corrupt_far);
            assert_eq!(entries["test.single"].volume, (0.9, 0.9));
            assert_eq!(entries["test.single"].xfade_bounds, (800.0, 1500.0));
            assert!(entries["test.single"].distant.is_none());
            if corrupt_far {
                assert_eq!(entries.len(), 1);
                assert!(profile.selections.is_empty());
                continue;
            }
            for (alias, max) in [(stock.alias, 1377.0), (far.alias, 3937.0)] {
                let entry = &entries[alias];
                assert_eq!(entry.volume, (1.0, 1.0));
                let curve = entry.weapon_falloff.unwrap();
                assert_eq!((curve.min, curve.max, curve.curve_amount), (400.0, max, 0.7));
                assert_eq!(entry.xfade_bounds, (800.0, 2800.0));
                assert_eq!(entry.pitch, (100.0, 100.0));
            }
            assert_eq!(entries[stock.alias].distant.as_deref(), Some(far.alias));
            assert!(entries[stock.alias].explicit_near_fade);
            assert!(entries[far.alias].distant_layer);
            assert_eq!(entries[far.alias].soundlevel, 94.0);
            assert_eq!(entries[far.alias].limit, None);
        }
    }

    #[test]
    fn decoy_playback_selects_verified_pair_and_preserves_disabled_or_failed_fallback() {
        use bevy::{
            audio::{AudioPlayer, AudioSource, PlaybackSettings},
            ecs::world::CommandQueue,
            prelude::*,
        };
        let old_near = wav(1000);
        let old_far = wav(-2000);
        let current_near = wav(3000);
        let current_far = wav(-4000);
        let near_hash = format!("{:x}", Sha256::digest(&old_near));
        let far_hash = format!("{:x}", Sha256::digest(&old_far));
        for (era, corrupt_far) in [
            (SoundEra::Classic2015, false),
            (SoundEra::InstalledLegacy, false),
            (SoundEra::Classic2015, true),
        ] {
            let mut fs = fs(
                old_near.clone(),
                if corrupt_far {
                    wav(-1999)
                } else {
                    old_far.clone()
                },
            );
            fs.push_back(
                "synthetic current",
                vfs::Mount::Memory(BTreeMap::from([
                    (
                        "sound/weapons/test/current.wav".into(),
                        Arc::from(current_near.clone()),
                    ),
                    (
                        "sound/weapons/test/current_distant.wav".into(),
                        Arc::from(current_far.clone()),
                    ),
                ])),
            );
            let mut entries = entries();
            let near = entries.remove("test.single").unwrap();
            entries.insert("weapon_ak47.single".into(), near);
            let mut profile = if era == SoundEra::InstalledLegacy {
                Profile::load(&fs, &mut entries, era)
            } else {
                Profile::default()
            };
            if era == SoundEra::Classic2015 {
                let mut spec = spec(&near_hash, &far_hash);
                spec.binding = "Weapon_AK47.Single";
                let installed = profile.install(&fs, &mut entries, &spec);
                assert_eq!(installed.is_ok(), !corrupt_far);
            }
            let mut sounds = super::super::SoundLibrary {
                entries,
                classic: profile,
                ..Default::default()
            };
            let mut world = World::new();
            let mut queue = CommandQueue::default();
            let mut sources = Assets::<AudioSource>::default();
            let pos = glam::Vec3::new(100.0, 200.0, 300.0);
            let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
                "7" { name weapon_ak47 baseitem 1 visuals { sound_single_shot Weapon_AK47.Single } }
            } }"#).unwrap();
            let weapon = catalog.get("weapon_ak47").unwrap();
            {
                let mut commands = Commands::new(&mut queue, &world);
                sounds.play_decoy(&mut commands, &fs, &mut sources, weapon, pos);
            }
            queue.apply(&mut world);
            let mut played = Vec::new();
            for (player, emitter, settings) in world
                .query::<(
                    &AudioPlayer<AudioSource>,
                    &super::super::Emitter,
                    &PlaybackSettings,
                )>()
                .iter(&world)
            {
                played.push(sources.get(&player.0).unwrap().bytes.to_vec());
                assert_eq!(emitter.pos, crate::coords::to_bevy(pos));
                assert!(settings.spatial);
            }
            let mut expected = if era == SoundEra::Classic2015 && !corrupt_far {
                vec![old_near.clone(), old_far.clone()]
            } else {
                vec![current_near.clone(), current_far.clone()]
            };
            played.sort();
            expected.sort();
            assert_eq!(played, expected, "era {era:?}, corrupt far {corrupt_far}");
            // A decoy is world audio, not a voice owned by its thrower.
            assert!(sounds.playing.is_empty());
        }
    }

    #[test]
    fn verified_pair_preserves_original_entries_and_only_selects_exact_stock_identity() {
        let near = wav(1000);
        let distant = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let distant_hash = format!("{:x}", Sha256::digest(&distant));
        let fs = fs(near.clone(), distant.clone());
        let mut entries = entries();
        // Modern pitch is authored for different waveforms. Keep public entries
        // intact while restoring normal pitch to both verified classic aliases.
        entries.get_mut("test.single").unwrap().pitch = (120.0, 120.0);
        entries.get_mut("test.distant").unwrap().pitch = (98.0, 101.0);
        let mut profile = Profile::default();
        profile
            .install(&fs, &mut entries, &spec(&near_hash, &distant_hash))
            .unwrap();
        assert_eq!(
            entries["test.single"].waves,
            ["sound/weapons/test/current.wav"]
        );
        assert_eq!(
            entries["test.distant"].waves,
            ["sound/weapons/test/current_distant.wav"]
        );
        assert!(entries["test.single"].frozen_wave.is_none());
        assert_eq!(entries["test.single"].pitch, (120.0, 120.0));
        assert_eq!(entries["test.distant"].pitch, (98.0, 101.0));
        assert_eq!(entries["csr.test.near"].pitch, (100.0, 100.0));
        assert_eq!(entries["csr.test.distant"].pitch, (100.0, 100.0));
        assert_eq!(
            entries["csr.test.near"]
                .frozen_wave
                .as_ref()
                .unwrap()
                .bytes
                .as_ref(),
            near
        );
        assert_eq!(
            entries["csr.test.distant"]
                .frozen_wave
                .as_ref()
                .unwrap()
                .bytes
                .as_ref(),
            distant
        );
        assert_eq!(
            entries["csr.test.near"].volume,
            entries["test.single"].volume
        );
        assert_eq!(entries["csr.test.near"].limit, Some(2));
        assert_eq!(
            entries["csr.test.near"].distant.as_deref(),
            Some("csr.test.distant")
        );
        assert!(entries["csr.test.distant"].distant_layer);
        let id = ContentId::csgo(ContentKind::Weapon, "ak47");
        assert_eq!(profile.select(&id, "TEST.single"), Some("csr.test.near"));
        assert_eq!(profile.select(&id, "Custom.Single"), None);
        assert_eq!(
            profile.select(
                &ContentId::new(GameOrigin::Mod, ContentKind::Weapon, "ak47"),
                "Test.Single"
            ),
            None
        );
        assert_eq!(
            profile.select(&ContentId::csgo(ContentKind::Sound, "ak47"), "Test.Single"),
            None
        );
        assert_eq!(
            profile.select(
                &ContentId::csgo(ContentKind::Weapon, "usp_silencer"),
                "Test.Single"
            ),
            None
        );
    }

    #[test]
    fn missing_corrupt_and_unsupported_variants_never_publish_half_a_pair() {
        let near = wav(1000);
        let distant = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let distant_hash = format!("{:x}", Sha256::digest(&distant));
        for fs in [
            vfs::Vfs::new(),
            fs(near.clone(), wav(-1999)),
            fs(vec![0; 46], distant.clone()),
        ] {
            let mut entries = entries();
            let mut profile = Profile::default();
            assert!(
                profile
                    .install(&fs, &mut entries, &spec(&near_hash, &distant_hash))
                    .is_err()
            );
            assert_eq!(entries.len(), 2);
            assert!(profile.selections.is_empty());
        }
        let bad = vec![0u8; 46];
        let bad_hash = format!("{:x}", Sha256::digest(&bad));
        let mut entries = entries();
        let mut profile = Profile::default();
        assert_eq!(
            profile.install(
                &fs(bad, distant),
                &mut entries,
                &spec(&bad_hash, &distant_hash)
            ),
            Err("retained waveform is unsupported")
        );
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn later_pack_shadow_cannot_replace_verified_near_or_distant_bytes() {
        let near = wav(1000);
        let distant = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let distant_hash = format!("{:x}", Sha256::digest(&distant));
        let mut fs = fs(near.clone(), distant.clone());
        let mut entries = entries();
        let mut profile = Profile::default();
        profile
            .install(&fs, &mut entries, &spec(&near_hash, &distant_hash))
            .unwrap();
        fs.push_front(
            "selected pack",
            vfs::Mount::Memory(BTreeMap::from([
                ("sound/old_near.wav".into(), Arc::from(wav(9000))),
                ("sound/old_distant.wav".into(), Arc::from(wav(8000))),
            ])),
        );
        assert_ne!(fs.read("sound/old_near.wav").unwrap(), near);
        assert_ne!(fs.read("sound/old_distant.wav").unwrap(), distant);
        assert_eq!(
            entries["csr.test.near"]
                .frozen_wave
                .as_ref()
                .unwrap()
                .bytes
                .as_ref(),
            near
        );
        assert_eq!(
            entries["csr.test.distant"]
                .frozen_wave
                .as_ref()
                .unwrap()
                .bytes
                .as_ref(),
            distant
        );
    }

    #[test]
    fn explicit_installed_mode_and_reserved_alias_collision_leave_definitions_untouched() {
        let mut entries = entries();
        let profile = Profile::load(&vfs::Vfs::new(), &mut entries, SoundEra::InstalledLegacy);
        assert_eq!(profile.era(), SoundEra::InstalledLegacy);
        assert_eq!(entries.len(), 2);
        let declared = entries["test.single"].clone();
        entries.insert("csr.test.near".into(), declared);
        let mut profile = Profile::default();
        assert_eq!(
            profile.install(&vfs::Vfs::new(), &mut entries, &spec("", "")),
            Err("profile entry name already declared")
        );
        assert_eq!(entries.len(), 3);
        assert!(entries["csr.test.near"].frozen_wave.is_none());
        assert!(profile.selections.is_empty());
    }

    #[test]
    fn absent_distant_routing_publishes_only_verified_near_without_inventing_a_layer() {
        use bevy::{
            audio::{AudioPlayer, AudioSource},
            ecs::world::CommandQueue,
            prelude::*,
        };
        let near = wav(1000);
        let hash = format!("{:x}", Sha256::digest(&near));
        let mut spec = spec(&hash, "");
        spec.distant = None;
        let mut entries = entries();
        let installed = entries.get_mut("test.single").unwrap();
        installed.distant = None;
        installed.weapon_falloff = Some(super::super::WeaponFalloff { min: 400.0, max: 800.0, floor: 0.05, curve_amount: 0.9 });
        installed.xfade_bounds = (800.0, 1500.0);
        installed.explicit_near_fade = true;
        let mut profile = Profile::default();
        // Unrouted/missing distant bytes do not affect a near-only definition.
        let fs = fs(near.clone(), vec![]);
        profile.install(&fs, &mut entries, &spec).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(!entries.contains_key("csr.test.distant"));
        assert!(entries["csr.test.near"].distant.is_none());
        assert!(!entries["csr.test.near"].distant_layer);
        assert_eq!(entries["csr.test.near"].weapon_falloff, entries["test.single"].weapon_falloff);
        assert_eq!(entries["csr.test.near"].xfade_bounds, (800.0, 1500.0));
        assert!(entries["csr.test.near"].explicit_near_fade);
        assert!(entries["test.single"].distant.is_none());
        let mut library = super::super::SoundLibrary {
            entries,
            classic: profile,
            ..Default::default()
        };
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut sources = Assets::<AudioSource>::default();
        {
            let mut commands = Commands::new(&mut queue, &world);
            library.play_with(
                &mut commands,
                &fs,
                &mut sources,
                "csr.test.near",
                super::super::Emit {
                    at: Some(glam::Vec3::ZERO),
                    ..Default::default()
                },
            );
        }
        queue.apply(&mut world);
        let played: Vec<_> = world
            .query::<&AudioPlayer<AudioSource>>()
            .iter(&world)
            .map(|player| sources.get(&player.0).unwrap().bytes.to_vec())
            .collect();
        assert_eq!(played, vec![near]);
    }

    #[test]
    fn actual_audio_sources_use_verified_pair_after_a_pack_shadows_both_paths() {
        use bevy::{
            audio::{AudioPlayer, AudioSource},
            ecs::world::CommandQueue,
            prelude::*,
        };
        let near = wav(1000);
        let distant = wav(-2000);
        let near_hash = format!("{:x}", Sha256::digest(&near));
        let distant_hash = format!("{:x}", Sha256::digest(&distant));
        let mut fs = fs(near.clone(), distant.clone());
        let mut entries = entries();
        let mut profile = Profile::default();
        profile
            .install(&fs, &mut entries, &spec(&near_hash, &distant_hash))
            .unwrap();
        fs.push_front(
            "selected pack",
            vfs::Mount::Memory(BTreeMap::from([
                ("sound/old_near.wav".into(), Arc::from(wav(9000))),
                ("sound/old_distant.wav".into(), Arc::from(wav(8000))),
            ])),
        );
        let mut library = super::super::SoundLibrary {
            entries,
            classic: profile,
            ..Default::default()
        };
        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut sources = Assets::<AudioSource>::default();
        {
            let mut commands = Commands::new(&mut queue, &world);
            library.play_with(
                &mut commands,
                &fs,
                &mut sources,
                "csr.test.near",
                super::super::Emit {
                    at: Some(glam::Vec3::ZERO),
                    ..Default::default()
                },
            );
        }
        queue.apply(&mut world);
        let played: Vec<Vec<u8>> = world
            .query::<&AudioPlayer<AudioSource>>()
            .iter(&world)
            .map(|player| sources.get(&player.0).unwrap().bytes.to_vec())
            .collect();
        assert_eq!(played.len(), 2);
        assert!(played.contains(&near));
        assert!(played.contains(&distant));
    }
}
