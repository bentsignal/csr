//! Private 2015 sound resources, isolated from installed/world search paths.
use super::{FrozenWave, WaveSpec, WeaponSpec, verified_wave};
use content::{ContentId, ContentKind, GameOrigin};
use std::{collections::HashMap, path::Path};

// Match Installed::install's manifest-ID filename, including drag/drop installs.
pub(super) const FILENAME: &str = "classic_weapon_audio_2015.csrpack";
const PACK_ID: &str = "classic_weapon_audio_2015";
const ORIGIN: &str = "classic2015";

pub(super) struct Historical {
    fs: vfs::Vfs,
    fingerprints: HashMap<String, (usize, String)>,
}

fn resource_id(path: &str) -> ContentId {
    ContentId::new(
        GameOrigin::from_prefix(ORIGIN),
        ContentKind::Sound,
        path.strip_prefix("sound/").unwrap_or(path),
    )
}

impl Historical {
    pub(super) fn open(path: &Path) -> Option<Self> {
        if !path.exists() {
            return None;
        }
        let result = packs::Pack::open_with_limits(path, packs::Limits {
            archive: 20 << 20,
            entry: 2 << 20,
            expanded: 24 << 20,
            entries: 178, // 175 accepted + two original shell inserts + manifest
            manifest: 64 << 10,
        }).and_then(|pack| {
            let specs: Vec<_> = WAVES.iter().chain(super::handling::waves()).copied().collect();
            Self::from_pack(pack, &specs)
        });
        match result {
            Ok(pack) => Some(pack),
            Err(_) => {
                bevy::log::warn!("Classic2015 private sound pack invalid; using retained shots and installed handling");
                None
            }
        }
    }

    // Pack::open/build already checks the container, manifest and payload hash.
    // Check this consumer's identity/allowlist as well: a valid unrelated pack
    // must never supply classic audio just because it has a matching pathname.
    pub(super) fn from_pack(
        pack: packs::Pack,
        specs: &[WaveSpec<'_>],
    ) -> Result<Self, String> {
        let manifest = pack.manifest();
        if manifest.id != PACK_ID
            || manifest.origin != ORIGIN
            || manifest.kind != packs::PackKind::Assets
            || !manifest.maps.is_empty()
        {
            return Err("unexpected classic sound pack identity".into());
        }
        let mut fingerprints = HashMap::new();
        for resource in &manifest.resources {
            let spec = specs.iter().find(|spec| spec.path == resource.path)
                .ok_or("unexpected classic sound resource path")?;
            if resource.id != resource_id(spec.path).to_string() {
                return Err("unexpected classic sound ContentId".into());
            }
            fingerprints.insert(spec.path.to_string(), (spec.size, spec.sha256.to_string()));
        }
        // This VFS contains only the immutable validated pack snapshot. It is
        // never exposed to Level, the world's VFS or non-classic sound lookup.
        let mut fs = vfs::Vfs::new();
        fs.push_back(PACK_ID, pack.mount());
        Ok(Self { fs, fingerprints })
    }

    pub(super) fn wave(&self, path: &str) -> Result<FrozenWave, &'static str> {
        let (size, sha256) = self.fingerprints.get(path)
            .ok_or("historical resource missing")?;
        verified_wave(&self.fs, &WaveSpec { path, size: *size, sha256 })
    }

    pub(super) fn pair(
        &self,
        spec: &WeaponSpec<'_>,
    ) -> Result<(FrozenWave, Option<FrozenWave>), &'static str> {
        let near = self.wave(spec.near_wave.path)?;
        let far = spec.distant.as_ref().map(|far| self.wave(far.wave.path)).transpose()?;
        Ok((near, far))
    }
}

// Derived size/SHA256 identities only. Original Valve depot 731, manifest
// 4730361390205743786 (2015-06-30), archives 048 and 054. Each allowlisted
// waveform was independently checked against its directory length and CRC32.
// No waveforms or historical scripts are included in the repository.
// Revolver was not present in this snapshot; keep its retained fallback.
const WAVES: &[WaveSpec<'static>] = &[
    WaveSpec { path: "sound/weapons/ak47/ak47-1-distant.wav", size: 203206, sha256: "260c3237643c74e4950ba2a96d1179228023f9e759235c43e4d1bc34f5ab95b9" },
    WaveSpec { path: "sound/weapons/ak47/ak47-1.wav", size: 288436, sha256: "a504312e586d54332bc42b533242bb7b57ba8704439138a624bf4edc18f71903" },
    WaveSpec { path: "sound/weapons/aug/aug-1-distant.wav", size: 205776, sha256: "06fd9cf7b7dbc9807b832dca4e61769d3ead333ab65235e2e702ebd88f121165" },
    WaveSpec { path: "sound/weapons/aug/aug-1.wav", size: 407788, sha256: "b28184046c10ff5d717771a7e877efadc9f59bce4afc95c6b09fbf970cd346ca" },
    WaveSpec { path: "sound/weapons/awp/awp1-distant.wav", size: 203206, sha256: "13f376ef9821a9c73978cabbea9ebb1b1adbdfdb27f108c412f0949da1cec864" },
    WaveSpec { path: "sound/weapons/awp/awp1.wav", size: 728752, sha256: "c93fb4714540f2895b1c89764b13238b78f5e53a7fa6e9478ee8da2102409032" },
    WaveSpec { path: "sound/weapons/bizon/bizon-1-distant.wav", size: 203206, sha256: "d0ef596be889eda229a241e35ae2cface35197f4fa1322dcfca448abfbb77f1b" },
    WaveSpec { path: "sound/weapons/bizon/bizon-1.wav", size: 248300, sha256: "c91682a2ff6110caf6827fb3503d8215cfa8c7073500ecec18c701daa4d3403f" },
    WaveSpec { path: "sound/weapons/cz75a/cz75a-1-distant.wav", size: 191940, sha256: "9b5d41885f0031c73c626f978cd279e115cd2f3c65e56c95251b57c7c80820ad" },
    WaveSpec { path: "sound/weapons/cz75a/cz75a-1.wav", size: 247542, sha256: "015a8969485640892238a8c29d978b4412483011a3cb48e71ac43295d3b7d77b" },
    WaveSpec { path: "sound/weapons/deagle/deagle-1-distant.wav", size: 203206, sha256: "c82550a5675a1ece25683f72562554db1dbeedacebd80bed1e04cb5075cc320e" },
    WaveSpec { path: "sound/weapons/deagle/deagle-1.wav", size: 394698, sha256: "c84c7a87cdeb04e106df58bf60137ee8eee70c91d3281c8c8c61618d9c99e5ef" },
    WaveSpec { path: "sound/weapons/elite/elite-1-distant.wav", size: 203206, sha256: "15d7a2f334a9e05318b21d8c49acd8b892e5227cd8770c327386e772d8885a1e" },
    WaveSpec { path: "sound/weapons/elite/elite-1.wav", size: 272972, sha256: "8ce1bb4d7375b216b18196d27aedc465f3085c85835cf1caa2addd13c7741bb2" },
    WaveSpec { path: "sound/weapons/famas/famas-1-distant.wav", size: 203206, sha256: "b385b8f39878ce8ff778f2edf46da789bf2bef0ae2441fd3dee189dd80daf87e" },
    WaveSpec { path: "sound/weapons/famas/famas-1.wav", size: 390780, sha256: "820ba90b1d05812105638081d0f24669f525743975c9897aa9339b4cc321375e" },
    WaveSpec { path: "sound/weapons/fiveseven/fiveseven-1-distant.wav", size: 203206, sha256: "b772ca4e9339516a16d87143a78b37f10a05a961c2a976ac8621eece05598709" },
    WaveSpec { path: "sound/weapons/fiveseven/fiveseven-1.wav", size: 256864, sha256: "85c88e25077d15bbcb4d53a348bd2bccc32a08bb534502a4276049e89af281d6" },
    WaveSpec { path: "sound/weapons/g3sg1/g3sg1-1-distant.wav", size: 203206, sha256: "be2545cb1c42318da3ccd0f4c5a1d8f263fcbc442ffe586a6d4ed4b2d94da6b3" },
    WaveSpec { path: "sound/weapons/g3sg1/g3sg1-1.wav", size: 331540, sha256: "62ad61182bf31b7175bf03eaac88628e7a1c5fdaa132f3e696739b8e64231e9d" },
    WaveSpec { path: "sound/weapons/galilar/galil-1-distant.wav", size: 203206, sha256: "3b73bbdb891488701cc7f83738352f68e15d1f28b8231b4d8b58d2679664b64c" },
    WaveSpec { path: "sound/weapons/galilar/galil-1.wav", size: 350844, sha256: "7571bc91913f9c401665e2105bdbc1f1be0d13aac21a4602f710a660bd51c442" },
    WaveSpec { path: "sound/weapons/glock18/glock18-1-distant.wav", size: 203206, sha256: "844205dbea8cc9391cbad2b72f411e23cc3297a2f1d406026279a074e6878360" },
    WaveSpec { path: "sound/weapons/glock18/glock18-1.wav", size: 195596, sha256: "8680abee797dceaa341820302c2fff5dc0d0445ac0ca99592be6e6ebf219556d" },
    WaveSpec { path: "sound/weapons/hkp2000/hkp2000-1-distant.wav", size: 203206, sha256: "993aed469338f6b66cf1f700eb99ab5f6f37f6493e8355e1b001950a54a78c86" },
    WaveSpec { path: "sound/weapons/hkp2000/hkp2000-1.wav", size: 384428, sha256: "f204d5995aec3b69ddbafac4b335a2e646f74c7cc6a77702a090b53ceb3bbfce" },
    WaveSpec { path: "sound/weapons/m249/m249-1-distant.wav", size: 205404, sha256: "2b71558c1e9f5a1b5162d204110cbd9ec7fb03ed6b53dbb344a5a8d109f8146e" },
    WaveSpec { path: "sound/weapons/m249/m249-1.wav", size: 331194, sha256: "1714d85b87446f1af014fecb621c1166cef08441a531fc3de9be5240d890703c" },
    WaveSpec { path: "sound/weapons/m4a1/m4a1-1-distant.wav", size: 23122, sha256: "fe23d5c351f5b095437c4326a715693cd380c8e81b1f0f8fa2793a2b9970f80c" },
    WaveSpec { path: "sound/weapons/m4a1/m4a1-1-single.wav", size: 247626, sha256: "c05556215073c7a6a9243efe517efdf08362dd22ca89a357cb6dc379309026c3" },
    WaveSpec { path: "sound/weapons/m4a1/m4a1-1.wav", size: 44438, sha256: "8548c3b0937bf07656c73021439b76a946a4a8bb339b208508af6e916f072195" },
    WaveSpec { path: "sound/weapons/m4a1/m4a1_unsil-1-distant.wav", size: 203206, sha256: "6127e9d080862b16aec0ceca6c0e6431ac54f4f0fdf38f1c7787da0be7671b50" },
    WaveSpec { path: "sound/weapons/m4a1/m4a1_unsil-1.wav", size: 211724, sha256: "9763983af9ef5951c541a14a6a9040d36ae2420026671c824369aaeb1226e793" },
    WaveSpec { path: "sound/weapons/mac10/mac10-1-distant.wav", size: 203206, sha256: "4e73ea7dbfa5ec12e4c5bb74cad1c0a8ef11b30222c6619a084e37a0d113e525" },
    WaveSpec { path: "sound/weapons/mac10/mac10-1.wav", size: 253390, sha256: "650c21ad4cd3a47eb17d09c4564f5cd289adc1d934d2eb78e12fc4868d7ec888" },
    WaveSpec { path: "sound/weapons/mag7/mag7-1-distant.wav", size: 203206, sha256: "82101e5e1f55c55dbc3d6203aea1f65fa23232b1257b6ccbd42ff298bba6a898" },
    WaveSpec { path: "sound/weapons/mag7/mag7-1.wav", size: 271872, sha256: "5e57ad339f0fbead233f2aa8ee215c313850b3c0fb827ce1b6ee7f7d4030a021" },
    WaveSpec { path: "sound/weapons/mp7/mp7-1-distant.wav", size: 203206, sha256: "fbbdfc95fdca54f4b8bf108c009b70955b56529c726d6189db5275f7e6e8ca1f" },
    WaveSpec { path: "sound/weapons/mp7/mp7-1.wav", size: 332074, sha256: "eaf15cfa8e59169ca9576e730965775eec1a7775277b19d286b165b013aa491c" },
    WaveSpec { path: "sound/weapons/mp9/mp9-1-distant.wav", size: 203206, sha256: "2ab5164dbf59674f6363ad4a5a0434318cee2020d778d62e0e51316fb1f60dcc" },
    WaveSpec { path: "sound/weapons/mp9/mp9-1.wav", size: 161420, sha256: "218880b4f192ccf29306beb0f4d3c68cf34e87d3232b17f033bbdf3defac9852" },
    WaveSpec { path: "sound/weapons/negev/negev-1-distant.wav", size: 203206, sha256: "1e5bbbf2f14d269b61f5e5c368e831544527f0c3f2c4b2d0982e5d18aa5159d9" },
    WaveSpec { path: "sound/weapons/negev/negev-1.wav", size: 329338, sha256: "869c2556051ae573276d213091ce7c12140e8c2a34bfb9b97fe6c743a8212c0c" },
    WaveSpec { path: "sound/weapons/nova/nova-1-distant.wav", size: 203206, sha256: "07ef0cecb524be3e8bbd2d4b23be609026e20d240ca4b525f4937d49cc1396f8" },
    WaveSpec { path: "sound/weapons/nova/nova-1.wav", size: 215904, sha256: "1ad12e6261d14904266a533b94cee444b5ee3208eda7eebf592a321df912eb2d" },
    WaveSpec { path: "sound/weapons/p250/p250-1-distant.wav", size: 203206, sha256: "bad8ddf7f4d0f7063a0cabd6996081e01c55a20e818c230177272988210e823f" },
    WaveSpec { path: "sound/weapons/p250/p250-1.wav", size: 206732, sha256: "01b10acbcb7d48e104a5c973599a623a2d8f3d93cb1c4f50d535c1176e2dd363" },
    WaveSpec { path: "sound/weapons/p90/p90-1-distant.wav", size: 203206, sha256: "2a3269724fc71d1aac5e397e5efbb455b26a991d2928fc8dbddeac7fddac40bb" },
    WaveSpec { path: "sound/weapons/p90/p90-1.wav", size: 248300, sha256: "0cbb602f0c4e686739061ea34b9ada773757bf0c3ce526251955243980ee087e" },
    WaveSpec { path: "sound/weapons/sawedoff/sawedoff-1-distant.wav", size: 203206, sha256: "560b844ee91c146619416cd0889a45e4497588c9feb09822a9e7264f853eb8c6" },
    WaveSpec { path: "sound/weapons/sawedoff/sawedoff-1.wav", size: 273056, sha256: "c413da2e61ad8fccf6b93e922d8fe2ff36cd71e42f5b464ec0df8e409652c58c" },
    WaveSpec { path: "sound/weapons/scar20/scar20_unsil-1-distant.wav", size: 203206, sha256: "4b5b6c1b385c20fa4bbe82022deadf5b09540a2324e6e9d51d4d1f3d37d35f2a" },
    WaveSpec { path: "sound/weapons/scar20/scar20_unsil-1.wav", size: 302274, sha256: "89fc663556fe372bdafd97481537a5fc3803ae9de58f2e535d9cba71923a3b25" },
    WaveSpec { path: "sound/weapons/sg556/sg556-1-distant.wav", size: 203206, sha256: "0dec8def6fd8fbc7c1ed608bede5f545369070bd103aff3acb188a28ea52add0" },
    WaveSpec { path: "sound/weapons/sg556/sg556-1.wav", size: 397542, sha256: "e3dfef911aeb36b1816e23c23f4891c82fbaef80f76e8d0bfecabd5f770509b3" },
    WaveSpec { path: "sound/weapons/ssg08/ssg08-1-distant.wav", size: 203206, sha256: "711541994cc93290732d7ec72abf11b6acce91337f087e807c62b5b08c4fe94b" },
    WaveSpec { path: "sound/weapons/ssg08/ssg08-1.wav", size: 252128, sha256: "33547c45fb5ddfb7057fe6dc97c693bda9274181940ec7756361fea76ece88c8" },
    WaveSpec { path: "sound/weapons/taser/taser_shoot.wav", size: 257448, sha256: "13125cdb85f84ab71ed3921fa55977288dfcafb8e36a414a83b1420468586748" },
    WaveSpec { path: "sound/weapons/tec9/tec9-1-distant.wav", size: 203206, sha256: "fd909ebda3a5a2d512d6701808161cd749df79353f064c70c6bb5390a6a511d6" },
    WaveSpec { path: "sound/weapons/tec9/tec9-1.wav", size: 272624, sha256: "e8b36e8f3dd68a7ef843e6f9750a208b13599ad63b679c346ef2edcd6cbea7ea" },
    WaveSpec { path: "sound/weapons/ump45/ump45-1-distant.wav", size: 203206, sha256: "1198ef9ff36f81d0b7fde6b58ed599edc370598233e81d82ff834049e265ee16" },
    WaveSpec { path: "sound/weapons/ump45/ump45-1.wav", size: 216090, sha256: "e5e1d3eec592dec89c88f897436962328ac6f5d7df3881328b5e9b776e823f7e" },
    WaveSpec { path: "sound/weapons/usp/usp1.wav", size: 44068, sha256: "ea96b9e40f5cebfc39272a6106029eecdb98bf867f301a661b651e055c55592a" },
    WaveSpec { path: "sound/weapons/usp/usp_unsil-1-distant.wav", size: 173596, sha256: "a4605b061f28a9101a0d3f265377d35dee290808135c2ae8b9e7f03884689cfb" },
    WaveSpec { path: "sound/weapons/usp/usp_unsil-1.wav", size: 196608, sha256: "3176347649f491669d715dd325eb40a6f69884245af7c3757cbf18d6d07e510f" },
    WaveSpec { path: "sound/weapons/xm1014/xm1014-1-distant.wav", size: 203206, sha256: "125900454190e419a69a8d87e8437df9b292f6a91d8f896660d4ce80b51a2863" },
    WaveSpec { path: "sound/weapons/xm1014/xm1014-1.wav", size: 277460, sha256: "b0925517fe2d003bb4c085e64c4166f1c4d4075263750fda6eff089b53481e54" },
];
