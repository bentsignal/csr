//! Bounded classic handling waves and declaration metadata; authored events stay intact.
use super::{Entry, FrozenWave, WaveSpec, historical::Historical, verified_wave};
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct Spec<'a> {
    binding: &'static str,
    historical: WaveSpec<'a>,
    installed: WaveSpec<'a>,
    volume: f32,
    channel: u8,
}

// Same exact event-to-path bindings in the shipped June 2015 and installed
// Legacy declarations. Pump PCM payloads differ, not merely RIFF headers.
// No scripts or assets are embedded.
// Keep MAG-7 back/forward atomic so a partial pack cannot mix their eras.
// Literal June 2015 declarations: all four use PITCH_NORM, CHAN_ITEM and
// SNDLVL_65dB, with no operator stacks, entry limit or explicit fade override.
// Depot 731 manifest 4730361390205743786, game_sounds_weapons.txt SHA1
// 8ecba5f796256eabca6068086f70bd0157e759fb (derived values only).
// Preserve the renderer's existing foley curve/mixer rather than guessing a
// historical mixing law. Only the verified group's volume/pitch/channel/limit
// change; installed/old-pack fallback keeps all installed metadata.
const NOVA: Spec<'static> = Spec {
    binding: "weapon_nova.pumpduringshot",
    volume: 0.3,
    channel: 3,
    historical: WaveSpec {
        path: "sound/weapons/nova/nova_pump.wav",
        size: 39236,
        sha256: "7dc2881cc8add49de540c42e0c5300e80548baa32be9d2d7dda594311c6cf68f",
    },
    installed: WaveSpec {
        path: "sound/weapons/nova/nova_pump.wav",
        size: 35914,
        sha256: "ae8b0f14117978612839be08ce3bf6c241604b6f04057714a6e527b5c263b9ba",
    },
};

const GROUPS: &[&[Spec<'static>]] = &[
    &[NOVA],
    // Draw and shotgun reload finish use Pump, while firing uses
    // PumpDuringShot. Both declarations name this same verified wave in 2015
    // and Legacy. Validate independently: an unknown alias must not disable
    // the other recognized binding (unlike MAG-7's atomic two-wave action).
    &[Spec { binding: "weapon_nova.pump", volume: 1.0, ..NOVA }],
    &[
        Spec {
            binding: "weapon_mag7.pumpback",
            volume: 1.0,
            channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/mag7/mag7_pump_back.wav",
                size: 22858,
                sha256: "452b27f399cd355744c50d7c01aa28d14935e56c005a08ff241c5dc73f008ea3",
            },
            installed: WaveSpec {
                path: "sound/weapons/mag7/mag7_pump_back.wav",
                size: 12244,
                sha256: "c1d42fa0fa679b76dd16e3738838b000fb0ca2596b7e7980c183ae768c6deb5b",
            },
        },
        Spec {
            binding: "weapon_mag7.pumpforward",
            volume: 1.0,
            channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/mag7/mag7_pump_forward.wav",
                size: 16422,
                sha256: "acdd8d4bae0d957f99495c1369d53ffa6202ffc89d05dcbe4310bc5f9f41f945",
            },
            installed: WaveSpec {
                path: "sound/weapons/mag7/mag7_pump_forward.wav",
                size: 9930,
                sha256: "0cebb88bea3723a9282f9e0c54428b94c57c69622857a7e8728c4ad8483d2cb0",
            },
        },
    ],
    // M4A4 and M4A1-S share draw/clip bindings. Only M4A4 invokes these
    // BoltBack/BoltForward entries; Legacy's M4A1S.* bolts and ClipHit have
    // no matching June 2015 declarations and must not be aliased to them.
    // Same script provenance above: PITCH_NORM, SNDLVL_65dB, no operator
    // stacks, entry limit or explicit fade override for all five entries.
    // Keep the recovered M4 set atomic, including its declaration metadata.
    &[
        Spec {
            binding: "weapon_m4a1.draw", volume: 0.5, channel: 6,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_draw.wav", size: 64994,
                sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_draw.wav", size: 90312,
                sha256: "f8a54f39f8efd71d8ef7f5703ec99fe7e439f0b8dcb3c1375fc382cfa950cec9",
            },
        },
        Spec {
            binding: "weapon_m4a1.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_clipout.wav", size: 15872,
                sha256: "a99c89083ce31767c915f62a1668b999330fc6222d1674144d0abb8dd9c656df",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_clipout.wav", size: 33120,
                sha256: "b3cdbb7d21f6e3285d7d76838dfe7378ce46a7112f2755ba003c4c2d32e05625",
            },
        },
        Spec {
            binding: "weapon_m4a1.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_clipin.wav", size: 13294,
                sha256: "71c849aeeb9b145035937f22624dda7267063e8ab31ade2951c978df30164fc7",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_clipin.wav", size: 36750,
                sha256: "654f310c41f97021006110af429922556fee149c51881c3d4a967c6793a17acc",
            },
        },
        Spec {
            binding: "weapon_m4a1.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_boltforward.wav", size: 21604,
                sha256: "da38a834809278b98df5ddf61e056518cf253ac7039f86be460686c824a53bee",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_boltforward.wav", size: 33120,
                sha256: "cdfecd48ca661ae5d2014499d570235089c916eea07a1f1b753fc9b5f5c34be6",
            },
        },
        Spec {
            binding: "weapon_m4a1.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_boltback.wav", size: 25870,
                sha256: "7b16e1f0fc354063d915a110de2cbe9bae0bd9d14c143c2e774bf8739227ab99",
            },
            installed: WaveSpec {
                path: "sound/weapons/m4a1/m4a1_boltback.wav", size: 33120,
                sha256: "1af86f38556f0ed0ec643530ced345cfc79fdf0b1407ee5cb26c2388c4bd3993",
            },
        },
    ],
];

// Remaining firearm bindings actually invoked by installed Legacy draw, reload
// and manual-action MDL events (including USP-S/CZ75 shared pistol entries).
// Same pinned June 2015 script/index as above; all selected original resources
// are verified against retained archives 048/054. Derived identities only.
// Keep each recovered firearm set atomic. Unknown later bindings and randomized
// shell insertions retain installed declarations. No event aliases are inferred.
// MAC-10 retains the exact BoltBack/BoltForward binding names: the two scripts
// explicitly name opposite paths, each guarded here by its own installed hash.
// All entries use PITCH_NORM/SNDLVL_65dB, no operator stacks or entry limits.
const ARSENAL_GROUPS: &[&[Spec<'static>]] = &[
    // aug
    &[
        Spec {
            binding: "weapon_aug.boltpull", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/aug/aug_boltpull.wav", size: 40304, sha256: "fe68427213992d10f5cd99d65e3ff948dd7767d3787b5492d72931ef91ffd67e" },
            installed: WaveSpec { path: "sound/weapons/aug/aug_boltpull.wav", size: 16204, sha256: "61f891311b73ce214c7abc9e87e234e1fde4b389348febedf66fbf4715d431e0" },
        },
        Spec {
            binding: "weapon_aug.boltrelease", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/aug/aug_boltrelease.wav", size: 18276, sha256: "1c2224f7c27d43e19a382d6ef376702c1ff08832864ebbf523b413bfbd5b740a" },
            installed: WaveSpec { path: "sound/weapons/aug/aug_boltrelease.wav", size: 36492, sha256: "eb21a2a5a4b0a1ac572507fc85bd29787f762ddba54a7893a946af4d81c187c9" },
        },
        Spec {
            binding: "weapon_aug.cliphit", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/aug/aug_cliphit.wav", size: 29240, sha256: "098347124316ca5d821e4d0e8865223598331c0b48d74cf1b3babb6e1c55d01c" },
            installed: WaveSpec { path: "sound/weapons/aug/aug_cliphit.wav", size: 29224, sha256: "ba4eb7efa65fbb59fb72790df3fadc028a3d84296c6f40bf91751356d82b0a3f" },
        },
        Spec {
            binding: "weapon_aug.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/aug/aug_clipin.wav", size: 12788, sha256: "8720924da2bb5c85eb9e022c8f7cd71afcee3d5ad64da64dcd31c5fb27e2d2b7" },
            installed: WaveSpec { path: "sound/weapons/aug/aug_clipin.wav", size: 29224, sha256: "51625c10593df25b79b7c14db82f4f2ff75ff33e2f2ff26b83cf653732799b3d" },
        },
        Spec {
            binding: "weapon_aug.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/aug/aug_clipout.wav", size: 11982, sha256: "1861dd0a5edc7d386a2332b0888f6bcf3168414df99de8116e7963231f3e33c6" },
            installed: WaveSpec { path: "sound/weapons/aug/aug_clipout.wav", size: 29224, sha256: "ba837f50093001310452231f9737a0062da4b825866cea51f1bc9224153d526b" },
        },
    ],
    // bizon
    &[
        Spec {
            binding: "weapon_bizon.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/bizon/bizon_boltback.wav", size: 19392, sha256: "aa241c9543cb2145e208771227c1d0f6c1b312e4bcb3071b7143eea907dd1ade" },
            installed: WaveSpec { path: "sound/weapons/bizon/bizon_boltback.wav", size: 18532, sha256: "9179694dd8e2069f836cc14c56d363c067d67cd5f4964baa7974efd25f19a883" },
        },
        Spec {
            binding: "weapon_bizon.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/bizon/bizon_boltforward.wav", size: 10616, sha256: "21fedc1c57c6ce51eaaa5d5fae7d1551896692ce49ee15bd07c0b40e8e2be63e" },
            installed: WaveSpec { path: "sound/weapons/bizon/bizon_boltforward.wav", size: 37828, sha256: "3d72088e48ce79ffd78da49ac28560c344091731ec475fef9b6977c4718c9efd" },
        },
        Spec {
            binding: "weapon_bizon.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/bizon/bizon_clipin.wav", size: 16660, sha256: "4bac3b1cca1079bcb6607999030ff129dc395522f1b998f4fadce01294ed042b" },
            installed: WaveSpec { path: "sound/weapons/bizon/bizon_clipin.wav", size: 18096, sha256: "74ab5a4d6ab81b12201dd302f93215fc6253668923a9762af5eb97015e03a80d" },
        },
        Spec {
            binding: "weapon_bizon.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/bizon/bizon_clipout.wav", size: 9156, sha256: "2b2ef787fd45c7cfc7ce4dd7aaf7dd9396c666665fdeeee0ef60db7597f2790f" },
            installed: WaveSpec { path: "sound/weapons/bizon/bizon_clipout.wav", size: 21444, sha256: "eb884d04878b3176931b362eb07b2194e19794ec76983701af3c87cef8e3f95b" },
        },
        Spec {
            binding: "weapon_bizon.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/bizon/bizon_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
            installed: WaveSpec { path: "sound/weapons/bizon/bizon_draw.wav", size: 65390, sha256: "0b1278b99f60da06b971d20dac4af2f6b066d7aabfe14b1108019b6d82d122c6" },
        },
    ],
    // deagle
    &[
        Spec {
            binding: "weapon_deagle.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/deagle/de_clipin.wav", size: 24628, sha256: "1b8eddb0e95b18360368d3837b596c7e86163c80db32b6742dda7285d156a245" },
            installed: WaveSpec { path: "sound/weapons/deagle/de_clipin.wav", size: 26854, sha256: "08fb1ce7081c662057fc1de00ce6ed90dc9aa4edbef3a0b1570725da6a36ff63" },
        },
        Spec {
            binding: "weapon_deagle.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/deagle/de_clipout.wav", size: 27478, sha256: "d2ad500c610e31514d193d8ea3219d9de4d74d79b1c991f7cfe59bec5499057a" },
            installed: WaveSpec { path: "sound/weapons/deagle/de_clipout.wav", size: 26136, sha256: "bbc80360af0d6c37c5751fd7fb98061bf34d6c89610a0a239bb603ee63fc5816" },
        },
        Spec {
            binding: "weapon_deagle.slideback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/deagle/de_slideback.wav", size: 13680, sha256: "23286e1a12c285e6772ab354d370441eeab44c1338d4b3931d37dfd0531faecf" },
            installed: WaveSpec { path: "sound/weapons/deagle/de_slideback.wav", size: 18666, sha256: "da0e82dd2736ea91cd4274a5b9372c57316db9b7015cc38e8a6abc8d47da3e56" },
        },
        Spec {
            binding: "weapon_deagle.slideforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/deagle/de_slideforward.wav", size: 11806, sha256: "8fcb24e05473700360b9a518b0dda271ff8ff1ae5252e22bf68970bf9cd5c9e4" },
            installed: WaveSpec { path: "sound/weapons/deagle/de_slideforward.wav", size: 13844, sha256: "dcb1b4e483baaf631195a4ce669fd31ca941e40ab452381bd16b298e71e34f1c" },
        },
    ],
    // elite
    &[
        Spec {
            binding: "weapon_elite.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/elite/elite_clipout.wav", size: 48264, sha256: "67aa964bf922b5e72b125896cc794a49f54f5bfeb5f807141bc75c972ceb3d0c" },
            installed: WaveSpec { path: "sound/weapons/elite/elite_clipout.wav", size: 48262, sha256: "0eedfb99b333875c8a8034d76d0b18d28139412f7b088870a3ccecef351ff3af" },
        },
        Spec {
            binding: "weapon_elite.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/elite/elite_draw.wav", size: 87350, sha256: "d0bfc71b2f4540efe73184c93010b67ffca9455237a8ac2aa7d74a8cc22126a4" },
            installed: WaveSpec { path: "sound/weapons/elite/elite_draw.wav", size: 110294, sha256: "9eb5e8a8d1bb2487823478711b4be53470ebb90ed7175b65933787693a510ff3" },
        },
        Spec {
            binding: "weapon_elite.lclipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/elite/elite_leftclipin.wav", size: 27520, sha256: "922f7c50ec8aef3af9039071dcd1833fb77e8934168b5435df57b0ba39a52dbf" },
            installed: WaveSpec { path: "sound/weapons/elite/elite_leftclipin.wav", size: 22898, sha256: "e242d5130bac59e69a80a39af19bc906ed9e6e69d1c6594cfe935ded2d0c9ef6" },
        },
        Spec {
            binding: "weapon_elite.rclipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/elite/elite_rightclipin.wav", size: 27520, sha256: "922f7c50ec8aef3af9039071dcd1833fb77e8934168b5435df57b0ba39a52dbf" },
            installed: WaveSpec { path: "sound/weapons/elite/elite_rightclipin.wav", size: 24334, sha256: "929ef71e41efac64927c0a2b5e3e1f07965b68d955cd4da146a54701df039f48" },
        },
        Spec {
            binding: "weapon_elite.sliderelease", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/elite/elite_sliderelease.wav", size: 29534, sha256: "6a7114bd0ff2bca4e6f752283074dc1e6383a4667e14448fc9909583ece64c9e" },
            installed: WaveSpec { path: "sound/weapons/elite/elite_sliderelease.wav", size: 43934, sha256: "16191ae43f7337f3b9f4bbaef413eff86a0ccd595ba27ba240045eb8a8867178" },
        },
    ],
    // famas
    &[
        Spec {
            binding: "weapon_famas.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/famas/famas_boltback.wav", size: 14392, sha256: "de2aab13f5ae113d6e3324bcf17644e41ce5be7d3a61d55bafddddbcca13df3d" },
            installed: WaveSpec { path: "sound/weapons/famas/famas_boltback.wav", size: 19574, sha256: "76460bba813e8e20c13a9cc5bb3599465bf5b860ea0903124c410e69c5196fa5" },
        },
        Spec {
            binding: "weapon_famas.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/famas/famas_boltforward.wav", size: 15480, sha256: "f569a01908ee3386d67f3cc55e3084139c29a66713209a9bdf8ec3c4767babe3" },
            installed: WaveSpec { path: "sound/weapons/famas/famas_boltforward.wav", size: 29204, sha256: "8618798a02f39445d647ce1209763841e0b64dd24ca7c8b6aeaf7b89e23592cd" },
        },
        Spec {
            binding: "weapon_famas.cliphit", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/famas/famas_cliphit.wav", size: 29240, sha256: "f2aa5ee9f288c4aed58d9c67381964ce0bf728c46383ec7f3505786b569d3615" },
            installed: WaveSpec { path: "sound/weapons/famas/famas_cliphit.wav", size: 27344, sha256: "6022928c10fae331a9ba195924d8d99af89f5af06d8fdd8ab1052b3496c85890" },
        },
        Spec {
            binding: "weapon_famas.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/famas/famas_clipin.wav", size: 24632, sha256: "e6802fd5c383c415c27c9c629cab188e7c1c10c486e7981823cb3ec1c92cc077" },
            installed: WaveSpec { path: "sound/weapons/famas/famas_clipin.wav", size: 32276, sha256: "08291c316569ff798a639f119cf426d2f9cf304a6cc7ba4345808555f0b31c70" },
        },
        Spec {
            binding: "weapon_famas.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/famas/famas_clipout.wav", size: 10932, sha256: "3a6eec9d5d5c459ec5691e4f2c2bc5f2f27f0d18bb1064044983e4a597236e1c" },
            installed: WaveSpec { path: "sound/weapons/famas/famas_clipout.wav", size: 41598, sha256: "6cb4bd486afb117561e5b62657f44e120f007755e2501305d60ed2db6138f3eb" },
        },
        Spec {
            binding: "weapon_famas.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/famas/famas_draw.wav", size: 40064, sha256: "7f59d30c417e6362d8195492c87ea66b275653936752c1bc414e545cb5e73f88" },
            installed: WaveSpec { path: "sound/weapons/famas/famas_draw.wav", size: 70266, sha256: "01cf81cbfe7e9721da0591f963c7acdb69af7e19d1d943d069a9dadd3522ef90" },
        },
    ],
    // fiveseven
    &[
        Spec {
            binding: "weapon_fiveseven.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/fiveseven/fiveseven_clipin.wav", size: 28750, sha256: "0ca22448cbdc5b995415a84598e684b98fcf5345e86622b72e3a1a021f9d41e1" },
            installed: WaveSpec { path: "sound/weapons/fiveseven/fiveseven_clipin.wav", size: 23204, sha256: "ae789dd10ba2afab7fe193d86482c301fd90cec0a9ac5f6a399cd379746efe55" },
        },
        Spec {
            binding: "weapon_fiveseven.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/fiveseven/fiveseven_clipout.wav", size: 19400, sha256: "42668a97f035b71b401a93e6ae115c2cb31b9b980965891ff07f5915779d03f4" },
            installed: WaveSpec { path: "sound/weapons/fiveseven/fiveseven_clipout.wav", size: 14408, sha256: "d37f89e7ca65f30b67a4c50132e353f0a481d611e140f4fda05d4f26fd301fa8" },
        },
        Spec {
            binding: "weapon_fiveseven.slideback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/fiveseven/fiveseven_slideback.wav", size: 13680, sha256: "23286e1a12c285e6772ab354d370441eeab44c1338d4b3931d37dfd0531faecf" },
            installed: WaveSpec { path: "sound/weapons/fiveseven/fiveseven_slideback.wav", size: 18206, sha256: "e5bb321721fcfe9b6aeaeb0d93f27125a0b6b7832508ec87efe94557c08178a1" },
        },
        Spec {
            binding: "weapon_fiveseven.sliderelease", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/fiveseven/fiveseven_sliderelease.wav", size: 28540, sha256: "c6a5bb62618782f9edf4dbf0b84011c398c54c72d2cdbac524c41dc59d876e83" },
            installed: WaveSpec { path: "sound/weapons/fiveseven/fiveseven_sliderelease.wav", size: 18290, sha256: "241493e17366e28bdf6c04af0522e164a7a8312a87e6d6650947743fcb06c37c" },
        },
    ],
    // g3sg1
    &[
        Spec {
            binding: "weapon_g3sg1.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_clipin.wav", size: 21218, sha256: "2eaf0b594613af9e1249912f4ddac4fa5669f360d828e76a5d235dc07c6dedf8" },
            installed: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_clipin.wav", size: 33196, sha256: "316246da039a7371a28fd3e4d0714ec67d4956f09e024e3cbfee9d5cc25b671b" },
        },
        Spec {
            binding: "weapon_g3sg1.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_clipout.wav", size: 18880, sha256: "aab575584f8c2a3198c5a1f59fa2351be3804d1132b1ad3ac12b98258e991e79" },
            installed: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_clipout.wav", size: 38708, sha256: "b8bb10c8db6283a153a26877e054619db8d4ad96ad7d808e567a63e017dd980e" },
        },
        Spec {
            binding: "weapon_g3sg1.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_draw.wav", size: 39972, sha256: "40f2a76edcaf2d0e5ab767c79b21a783dad4009659d5fedf29ec4b9723d24ec3" },
            installed: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_draw.wav", size: 72128, sha256: "b6d4a376f31a599d7db5230627ccf8ff096357852aae1367bb6639ab505900f2" },
        },
        Spec {
            binding: "weapon_g3sg1.slideback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_slideback.wav", size: 23484, sha256: "8b02c5cb96872aa4406fdf98d164fd6eacde6a1db39cde8c6565ff0d72a0f2de" },
            installed: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_slideback.wav", size: 16658, sha256: "1d2a1053b72294cc4165b8c9b55bb9aa5de95da618cfba29bf51bdda1a5942d1" },
        },
        Spec {
            binding: "weapon_g3sg1.slideforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_slideforward.wav", size: 32734, sha256: "e8aa8019c9a58fc55feb19d41826ce3868862cae8925fb880b56480baade32e1" },
            installed: WaveSpec { path: "sound/weapons/g3sg1/g3sg1_slideforward.wav", size: 51130, sha256: "557dfc57af298a86d22fc5324358da6cd1364ddca5fbe3355ab453c7e588481c" },
        },
    ],
    // galilar
    &[
        Spec {
            binding: "weapon_galilar.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/galilar/galil_boltback.wav", size: 14748, sha256: "f51516534dbe83651ce700eee7d5b83e13421950b2ca68ca554518956ffd6735" },
            installed: WaveSpec { path: "sound/weapons/galilar/galil_boltback.wav", size: 15558, sha256: "a68d8154d553d826805c3565e7225b7b654753d7c3e61e15bd925f56162aee07" },
        },
        Spec {
            binding: "weapon_galilar.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/galilar/galil_boltforward.wav", size: 14052, sha256: "cf162a5d50c0e3caf52dbd3ff72b59cd9785f20346d644bb37d3de3393a5fb68" },
            installed: WaveSpec { path: "sound/weapons/galilar/galil_boltforward.wav", size: 21754, sha256: "6a3b4d6dfe0b8db017c2e137c71915a5226e465669eb8824ba71ee803b3614ec" },
        },
        Spec {
            binding: "weapon_galilar.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/galilar/galil_clipin.wav", size: 15732, sha256: "e5d7c44b01d2c90bc52aba78aa783a186747ffc90f8043efe33db311583c5227" },
            installed: WaveSpec { path: "sound/weapons/galilar/galil_clipin.wav", size: 29158, sha256: "b277933de7ebabcb89ec6d365729d66b758e0f4a4350a7d797a197c940e87103" },
        },
        Spec {
            binding: "weapon_galilar.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/galilar/galil_clipout.wav", size: 11272, sha256: "ff93a18e37733014bd08990c57b47741a773b246379e5f4d6b4a1b7954cfda23" },
            installed: WaveSpec { path: "sound/weapons/galilar/galil_clipout.wav", size: 20478, sha256: "2cfeb3d460ceb5a9a798b0a7c3e9ac8b830ab68069843d44dd10173326ee057a" },
        },
        Spec {
            binding: "weapon_galilar.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/galilar/galil_draw.wav", size: 39944, sha256: "21a43a83750b42b0786cc1149cee9ef895b0a057b124f7f708fb4c32bae351b5" },
            installed: WaveSpec { path: "sound/weapons/galilar/galil_draw.wav", size: 99346, sha256: "350ff907f5b617642333ff04c9c62090d23b85a95a382077528dcc4ed5153673" },
        },
    ],
    // glock
    &[
        Spec {
            binding: "weapon_glock.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/glock18/glock_clipin.wav", size: 13330, sha256: "02bcd10f0537ff77a19556121fa212d68aed54da04c07594d5fdc4dc1d0a1dcc" },
            installed: WaveSpec { path: "sound/weapons/glock18/glock_clipin.wav", size: 26616, sha256: "c92fb13e153ea91594fc63733c5d2950da45021d7eff609da6ac3e32ef9bca51" },
        },
        Spec {
            binding: "weapon_glock.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/glock18/glock_clipout.wav", size: 26012, sha256: "57f63c4a1754292f2cdd47202be9587cebcd67857f505c3650b147476ba23360" },
            installed: WaveSpec { path: "sound/weapons/glock18/glock_clipout.wav", size: 26616, sha256: "092f655ba5f88d52da4195458ad6778952184c8a1d328582c1ff02043300e72e" },
        },
        Spec {
            binding: "weapon_glock.slideback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/glock18/glock_slideback.wav", size: 6862, sha256: "82a4a239c6cdd1fd06ef3477292302f63705d598d13312d4127dbf0d8acff6ea" },
            installed: WaveSpec { path: "sound/weapons/glock18/glock_slideback.wav", size: 22094, sha256: "7cc666bb91cb688ca686f8289c85048dbf4cdcdb434b896cd9598716312f4988" },
        },
        Spec {
            binding: "weapon_glock.sliderelease", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/glock18/glock_sliderelease.wav", size: 29534, sha256: "fbe6e8b544881012baa2289ac037eeb3801a8bf42e6b52bbf1e2b832d81ca7c6" },
            installed: WaveSpec { path: "sound/weapons/glock18/glock_sliderelease.wav", size: 29762, sha256: "415174b884ef70dc565e5ac114795fb37bfd47ea97e978c687a8c99b437facb4" },
        },
    ],
    // hkp2000
    &[
        Spec {
            binding: "weapon_hkp2000.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/hkp2000/hkp2000_clipin.wav", size: 26608, sha256: "952343f5eecbe00984cd93dca5e9db171235ff6da7d962c8eaab1de5c9b79a7e" },
            installed: WaveSpec { path: "sound/weapons/hkp2000/hkp2000_clipin.wav", size: 22954, sha256: "3f2db1c867d9a3dcbb7ab7d5984002e01a3c6a20ed52676a13513e9858e97d04" },
        },
        Spec {
            binding: "weapon_hkp2000.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/hkp2000/hkp2000_clipout.wav", size: 17144, sha256: "05f818652345f48c86db91524a07f7abd6265d4efacdbe81e18d447e9c5bc134" },
            installed: WaveSpec { path: "sound/weapons/hkp2000/hkp2000_clipout.wav", size: 22732, sha256: "634bcd4629f6f2d0969e1859472043c5ec5812fd7a3ca87a600f6b2733a5d65f" },
        },
        Spec {
            binding: "weapon_hkp2000.slideback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/hkp2000/hkp2000_slideback.wav", size: 12048, sha256: "28c8c47af0c576f3f21bd5330342cb7166d3a9ab8757c68c30d9d1f6e252711a" },
            installed: WaveSpec { path: "sound/weapons/hkp2000/hkp2000_slideback.wav", size: 28880, sha256: "193c30e5373f9367c8b4808ced2fff31edaa03a68e4fc124406e21358eb7f420" },
        },
        Spec {
            binding: "weapon_hkp2000.sliderelease", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/hkp2000/hkp2000_sliderelease.wav", size: 12916, sha256: "0e9a4013a0cdc2c7b2051db4d6d9bf11d2ff0088fc57c20fcb3b6d79eac35eed" },
            installed: WaveSpec { path: "sound/weapons/hkp2000/hkp2000_sliderelease.wav", size: 13840, sha256: "78fcd53de9cb37818777570261926a7045cf9f7b22b0b61dfa2656b15db42956" },
        },
    ],
    // mac10
    &[
        Spec {
            binding: "weapon_mac10.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mac10/mac10_boltback.wav", size: 15124, sha256: "cfbb92cd5ff1c8860b5cc8f92d362d06c39527e4f64e038b00fa039f2150b12c" },
            installed: WaveSpec { path: "sound/weapons/mac10/mac10_boltforward.wav", size: 14788, sha256: "6788cb059cfaec4fd0430967297913d02d4245ef4e080d209f14e367a53f032a" },
        },
        Spec {
            binding: "weapon_mac10.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mac10/mac10_boltforward.wav", size: 5724, sha256: "0eb0ce2d8dcb28b89e0360644fb9d2d3696885e5dc566c1449c6ff7c43831027" },
            installed: WaveSpec { path: "sound/weapons/mac10/mac10_boltback.wav", size: 38976, sha256: "9c7bbb72054bdc75b267623bfade088f28e8f98f31f5c092c6d7033b7a3ff483" },
        },
        Spec {
            binding: "weapon_mac10.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mac10/mac10_clipin.wav", size: 30204, sha256: "d97b6b2e1c5061cb745445560c463f3da80ded41f2168caac325fbf17b0de7d8" },
            installed: WaveSpec { path: "sound/weapons/mac10/mac10_clipin.wav", size: 19784, sha256: "29bbfc2dbe04e870672e101b73b13d9cf81f3d34cf8c9361b8eacb700ac3c6f8" },
        },
        Spec {
            binding: "weapon_mac10.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mac10/mac10_clipout.wav", size: 14644, sha256: "205cbedf0863a68edce2c119e5006361c8b42f4bee470a3ac5e5f0a032af3bc6" },
            installed: WaveSpec { path: "sound/weapons/mac10/mac10_clipout.wav", size: 26666, sha256: "aa9b990d82fa520374d59bc8d8c01e453becfa416af5ed7adbee41d80be297a9" },
        },
        Spec {
            binding: "weapon_mac10.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/mac10/mac10_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
            installed: WaveSpec { path: "sound/weapons/mac10/mac10_draw.wav", size: 57926, sha256: "4f5233fa1375de850b7f275b0152ae063459ee611b2509d7edef7f3110521ea9" },
        },
    ],
    // mp7
    &[
        Spec {
            binding: "weapon_mp7.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mp7/mp7_clipin.wav", size: 23244, sha256: "1245e58970a77be2728492de4771b9cfe25737e68397c4ead1acf4a9244b1730" },
            installed: WaveSpec { path: "sound/weapons/mp7/mp7_clipin.wav", size: 44144, sha256: "eab2fac37b6deb8a108f2ea77eeb572492f7454566d034e28c16f41059f565b1" },
        },
        Spec {
            binding: "weapon_mp7.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mp7/mp7_clipout.wav", size: 26374, sha256: "908d26fbd4c7d5276eeaa8a188a1db5458f6c74a447cb5c347f2a72b2762131a" },
            installed: WaveSpec { path: "sound/weapons/mp7/mp7_clipout.wav", size: 49770, sha256: "3af5b3c395034f42394b95d58201b26fb3a747676e9f8843372ae1fdfd80437a" },
        },
        Spec {
            binding: "weapon_mp7.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/mp7/mp7_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
            installed: WaveSpec { path: "sound/weapons/mp7/mp7_draw.wav", size: 64994, sha256: "cb2f756cb3db885406e7ad2770fd7818d5da1f0850109c248c824f692e2ca568" },
        },
        Spec {
            binding: "weapon_mp7.slideback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mp7/mp7_slideback.wav", size: 13208, sha256: "e96d66df75268912dc686b5fcd853ffa90cfa4829242c74be76408d936d7ffdb" },
            installed: WaveSpec { path: "sound/weapons/mp7/mp7_slideback.wav", size: 13208, sha256: "906fddc42ff836c14064b3dfb898732fab6c54eb43db10e028ea1ac6302c820b" },
        },
        Spec {
            binding: "weapon_mp7.slideforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mp7/mp7_slideforward.wav", size: 10308, sha256: "426eac2507312dd875e6b9ee0a1237300f1c3583d1d85a2ad6871f4e5c3874a5" },
            installed: WaveSpec { path: "sound/weapons/mp7/mp7_slideforward.wav", size: 14794, sha256: "18470f62fad3b1f43a0168143cc4e3cc936acf9f4ded4cf0ce25ddb09adb90da" },
        },
    ],
    // mp9
    &[
        Spec {
            binding: "weapon_mp9.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mp9/mp9_boltback.wav", size: 19448, sha256: "efcf0b094c93ed89aecc2625b08a6b9e0268e72eb741c9c022a8839127d76e57" },
            installed: WaveSpec { path: "sound/weapons/mp9/mp9_boltback.wav", size: 11888, sha256: "2240fa7224b3c1ce6fccbe5a2d9829a1c8eb49b676f2966a6417a0a200a01746" },
        },
        Spec {
            binding: "weapon_mp9.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mp9/mp9_boltforward.wav", size: 10560, sha256: "a792e0da350873e8fd35e38f7f18043b0da4466482db726fd868d6adf95e5472" },
            installed: WaveSpec { path: "sound/weapons/mp9/mp9_boltforward.wav", size: 10308, sha256: "42ee0aabfacab281caded354aac9ebec295a8f4dee04866b4daa6f938572e1bc" },
        },
        Spec {
            binding: "weapon_mp9.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mp9/mp9_clipin.wav", size: 17644, sha256: "b602cf186ff5c47848c4ef3d05fa1ccc558f7438ccb30e580119283653faa256" },
            installed: WaveSpec { path: "sound/weapons/mp9/mp9_clipin.wav", size: 17644, sha256: "b1b462c833408c3e80bbdb0cfcc2db9126aadb6a54235411fd2de8c90d0ec75d" },
        },
        Spec {
            binding: "weapon_mp9.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/mp9/mp9_clipout.wav", size: 17214, sha256: "48d27bccb6045c30fe73f66c87f210322f78874912c90c514dc6894fb7ccca95" },
            installed: WaveSpec { path: "sound/weapons/mp9/mp9_clipout.wav", size: 27606, sha256: "70e68a9e7c5e564072690d70e11f4b2591ea56bf873d8a8750f133fba8d8486d" },
        },
        Spec {
            binding: "weapon_mp9.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/mp9/mp9_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
            installed: WaveSpec { path: "sound/weapons/mp9/mp9_draw.wav", size: 44144, sha256: "2fd747cb022ed77fa57c524f3c058d54f668888f6062db855a10330e9faf6342" },
        },
    ],
    // p250
    &[
        Spec {
            binding: "weapon_p250.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p250/p250_clipin.wav", size: 26656, sha256: "53ee1e9e4d37ffd1776f4f6afaf83d87f1c59639d15323e48ce814b6316a7bef" },
            installed: WaveSpec { path: "sound/weapons/p250/p250_clipin.wav", size: 26656, sha256: "3e93980d70b2502736b11fcc9e2b287441fe4524032b2c29e7eacd18b66496d0" },
        },
        Spec {
            binding: "weapon_p250.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p250/p250_clipout.wav", size: 20284, sha256: "ee6ca34ff4327fec60897a99dab9980f5ea59cf3054bf6bbfbe4bf302efe51dc" },
            installed: WaveSpec { path: "sound/weapons/p250/p250_clipout.wav", size: 38444, sha256: "48c726473846c4daefff3cd7dbe90979dcfeacb5d72b73c278d39ac6ad03a5fc" },
        },
        Spec {
            binding: "weapon_p250.slideback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p250/p250_slideback.wav", size: 13308, sha256: "b203eac4e4b9581d90f4ac38f1ae7d752d07e36f9d4c5935959fc4ad4fb91288" },
            installed: WaveSpec { path: "sound/weapons/p250/p250_slideback.wav", size: 13306, sha256: "e9f83fdc01266c728ff192d1587d4fc0475a4cc77b7c7d299349d4eb03e9e319" },
        },
        Spec {
            binding: "weapon_p250.sliderelease", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p250/p250_sliderelease.wav", size: 23800, sha256: "d3277b9928004de7e76dd3cabc39f75dc2460ce47b7748cef780cacf69ae8bac" },
            installed: WaveSpec { path: "sound/weapons/p250/p250_sliderelease.wav", size: 13422, sha256: "74b5521ee3193ea5ec2f948e2ae260f922563240f0bd4641dd45e2deb65f4c4c" },
        },
    ],
    // p90
    &[
        Spec {
            binding: "weapon_p90.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p90/p90_boltback.wav", size: 15136, sha256: "0881852f9d7384c78efc2cf9261f9ce2cab24cc9db9825ce2605011dbac126b7" },
            installed: WaveSpec { path: "sound/weapons/p90/p90_boltback.wav", size: 15136, sha256: "c4fa42aea5820b4721e5861ef0aff9f90c536e6f5bc5d3a4e18ea7e6b1149aed" },
        },
        Spec {
            binding: "weapon_p90.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p90/p90_boltforward.wav", size: 15476, sha256: "30261a94402caa05f61cbbb320fce2a46c549828005ee285cc0d8fe95f41be79" },
            installed: WaveSpec { path: "sound/weapons/p90/p90_boltforward.wav", size: 15474, sha256: "7493f5dcbdade7a83577cdae3ad20d0e5df42265dd8cb9cb85cffb797b6d1f98" },
        },
        Spec {
            binding: "weapon_p90.cliphit", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p90/p90_cliphit.wav", size: 10364, sha256: "b27199fa902cb760f56613c4a2a5ac4ee131ec15604e1026693ae043be4539f2" },
            installed: WaveSpec { path: "sound/weapons/p90/p90_cliphit.wav", size: 10364, sha256: "6d746de28931e1c8699e1799f5557be6c540c0919e6d25eacafd38ad922c3ea3" },
        },
        Spec {
            binding: "weapon_p90.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p90/p90_clipin.wav", size: 38688, sha256: "13b1861945a61200cbc0780614420489f10172d01157a15fae192893e7e481b2" },
            installed: WaveSpec { path: "sound/weapons/p90/p90_clipin.wav", size: 38728, sha256: "9b23a4df93cc76300034e67e0e8cdfda7aad60b4da2dacbcf4059b118aa15c2f" },
        },
        Spec {
            binding: "weapon_p90.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p90/p90_clipout.wav", size: 27524, sha256: "f919848ffff721ed11a58b8afbd7cde84280e9031dd7fd9bb47f6d7fe40e3462" },
            installed: WaveSpec { path: "sound/weapons/p90/p90_clipout.wav", size: 20812, sha256: "b322d19bf02ebbe577637cffefa0e3d28a6303109e29bea343bbb9533fbd1517" },
        },
        Spec {
            binding: "weapon_p90.cliprelease", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/p90/p90_cliprelease.wav", size: 20244, sha256: "e6dd3dea4bee46209002e0d349629e223b044eb84d7e251caeea3617c8ef5284" },
            installed: WaveSpec { path: "sound/weapons/p90/p90_cliprelease.wav", size: 20244, sha256: "83ea231715211d47269ad5ccecfa9f23cfa4e43cf610b8dd787675780b2aa4b4" },
        },
        Spec {
            binding: "weapon_p90.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/p90/p90_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
            installed: WaveSpec { path: "sound/weapons/p90/p90_draw.wav", size: 64994, sha256: "580949d6428bbf9c94bbfff353f10cb026033797186401a7438cd4d5ddc67c10" },
        },
    ],
    // sawedoff
    &[
        Spec {
            binding: "weapon_sawedoff.pump", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/sawedoff/sawedoff_pump.wav", size: 39236, sha256: "16ee52381a16d6d2125f427e6fc7905d1d069101faad6d55ccdee2bb9fd67459" },
            installed: WaveSpec { path: "sound/weapons/sawedoff/sawedoff_pump.wav", size: 39234, sha256: "52a19ccab8907393686999b1594f0ace696d3fa4155e7b61f29886f3680ec0fc" },
        },
        Spec {
            binding: "weapon_sawedoff.pumpduringshot", volume: 0.3, channel: 3,
            historical: WaveSpec { path: "sound/weapons/sawedoff/sawedoff_pump.wav", size: 39236, sha256: "16ee52381a16d6d2125f427e6fc7905d1d069101faad6d55ccdee2bb9fd67459" },
            installed: WaveSpec { path: "sound/weapons/sawedoff/sawedoff_pump.wav", size: 39234, sha256: "52a19ccab8907393686999b1594f0ace696d3fa4155e7b61f29886f3680ec0fc" },
        },
    ],
    // scar20
    &[
        Spec {
            binding: "weapon_scar20.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/scar20/scar20_boltback.wav", size: 11036, sha256: "4951279cbf2b7f5ee9e80683c5ef2bbd035d6ed39b2feff5c140bf3ae7d5da8d" },
            installed: WaveSpec { path: "sound/weapons/scar20/scar20_boltback.wav", size: 19682, sha256: "954fe1f1259fe197c8d421621544522163dda5f6136917c51eec2bdc05bc791d" },
        },
        Spec {
            binding: "weapon_scar20.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/scar20/scar20_boltforward.wav", size: 14400, sha256: "6a09cd5dd81426855fab0c2a3e518c92670f678f771c60625117b951893f27fe" },
            installed: WaveSpec { path: "sound/weapons/scar20/scar20_boltforward.wav", size: 25326, sha256: "60fa3d3c2e1edfdb8586f9d9ade24ccd914e349cb7ea858c237fa9bb14146d89" },
        },
        Spec {
            binding: "weapon_scar20.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/scar20/scar20_clipin.wav", size: 15220, sha256: "aba85e11493f2bda018830aa3804cac5bc0868a0e6e58fc8a101b8856b01c62d" },
            installed: WaveSpec { path: "sound/weapons/scar20/scar20_clipin.wav", size: 36018, sha256: "9fc69d7fb9a9f36d68255ec6a1d1db63a581f7905821667c1c93ac6cccd2ca8b" },
        },
        Spec {
            binding: "weapon_scar20.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/scar20/scar20_clipout.wav", size: 18068, sha256: "1057e1e4f4b090d19bbad896a28fdd00748bd5a35afa8439bffb6a9b1dde5e24" },
            installed: WaveSpec { path: "sound/weapons/scar20/scar20_clipout.wav", size: 73300, sha256: "beb74719fec5eae85b862ed3624608d88871ae6280bc6725672467343e40416e" },
        },
        Spec {
            binding: "weapon_scar20.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/scar20/scar20_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
            installed: WaveSpec { path: "sound/weapons/scar20/scar20_draw.wav", size: 73300, sha256: "fa12321975d07cf2baaf6ed227e2b51d49b3dd6b90b4f43c342ea859e67e463d" },
        },
    ],
    // sg556
    &[
        Spec {
            binding: "weapon_sg556.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/sg556/sg556_boltback.wav", size: 14764, sha256: "9fde44b7fd4687ed2fa78499a435f8002556588bbdecdf6ceb838b67673f62f5" },
            installed: WaveSpec { path: "sound/weapons/sg556/sg556_boltback.wav", size: 25794, sha256: "3950414b8de5fe3f11f68180fa46291d40578f707b867d2c63c354dced3ce653" },
        },
        Spec {
            binding: "weapon_sg556.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/sg556/sg556_boltforward.wav", size: 14100, sha256: "ade0d9c9e095430b5cbf93550df190bd241a7433f872072b552ed836707c50cf" },
            installed: WaveSpec { path: "sound/weapons/sg556/sg556_boltforward.wav", size: 30716, sha256: "b2552c2b97a763a0174a8199d977359b129baed24896f1267d12281ba9e80cdf" },
        },
        Spec {
            binding: "weapon_sg556.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/sg556/sg556_clipin.wav", size: 10692, sha256: "f00eb3f343397202a7e9c8f38c4d176eddd1bbbc1d3265810283c3b3c4003a5e" },
            installed: WaveSpec { path: "sound/weapons/sg556/sg556_clipin.wav", size: 35952, sha256: "575c8f857fec5a14f490d60800f5f9ee949f8499986e352b8264f847d161e31d" },
        },
        Spec {
            binding: "weapon_sg556.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/sg556/sg556_clipout.wav", size: 10264, sha256: "4e557df04e5f602f84414324a088bcfc48f34db3739e7f9623e32f08f6592e0b" },
            installed: WaveSpec { path: "sound/weapons/sg556/sg556_clipout.wav", size: 34344, sha256: "61c9eda327f56715c47bf136396fd51bf2fe36c453869d5cfb647ec8b85d79e1" },
        },
        Spec {
            binding: "weapon_sg556.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/sg556/sg556_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
            installed: WaveSpec { path: "sound/weapons/sg556/sg556_draw.wav", size: 88320, sha256: "7f55b3150de22ca496e7df65027d50b8f1efbc809909219d8aa0a6b64fc68b35" },
        },
    ],
    // ssg08
    &[
        Spec {
            binding: "weapon_ssg08.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/ssg08/ssg08_boltback.wav", size: 29408, sha256: "2ee1d6f1cc4e2b4e3933ef09e285c0b1a09f1f8130d9f4b8afe94281e989f2b8" },
            installed: WaveSpec { path: "sound/weapons/ssg08/ssg08_boltback.wav", size: 29406, sha256: "dc25f98e9cbe2796ac00169b6b2d66a77bd0515ef6860494fcbd80021364542d" },
        },
        Spec {
            binding: "weapon_ssg08.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/ssg08/ssg08_boltforward.wav", size: 18978, sha256: "8a9ec313fdd18eb719c3b87fe6d2cdd0387acc2fea03d8358cb19cee718fd11b" },
            installed: WaveSpec { path: "sound/weapons/ssg08/ssg08_boltforward.wav", size: 29406, sha256: "6dba05235b25573db76f388d4a1dcabb185d37855327c9d2a3574dcfbff5a886" },
        },
        Spec {
            binding: "weapon_ssg08.cliphit", volume: 1.0, channel: 6,
            historical: WaveSpec { path: "sound/weapons/ssg08/ssg08_cliphit.wav", size: 23544, sha256: "584cc4dacca962c55f924b87ec3c8713379a95bb00d8dabd113eaa68358221e3" },
            installed: WaveSpec { path: "sound/weapons/ssg08/ssg08_cliphit.wav", size: 25288, sha256: "fa4e1906bb9346b00915d1273260dbc0948b94a70899f22e422d9cb63cc84454" },
        },
        Spec {
            binding: "weapon_ssg08.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/ssg08/ssg08_clipin.wav", size: 23702, sha256: "6a80c20efe94534d3b2ebd607dc6123f73b5db6215c11c145d0616d0b4f9bc1a" },
            installed: WaveSpec { path: "sound/weapons/ssg08/ssg08_clipin.wav", size: 63438, sha256: "e8ea8cd4ae495792d099435686f73444a36d65d6ed469242d73731b7e61c47fb" },
        },
        Spec {
            binding: "weapon_ssg08.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/ssg08/ssg08_clipout.wav", size: 39220, sha256: "0ce56fd955a223f4133834a8cfc99f1f0ab751283a84f1e03580134b8a9077e1" },
            installed: WaveSpec { path: "sound/weapons/ssg08/ssg08_clipout.wav", size: 44258, sha256: "9f2bb0b1e138f6b776ed3ad4c826ffa2620a5ca2070c0f91db79b70b5cf361a3" },
        },
        Spec {
            binding: "weapon_ssg08.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/ssg08/ssg08_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
            installed: WaveSpec { path: "sound/weapons/ssg08/ssg08_draw.wav", size: 88244, sha256: "adc7adcae05f2fdb46d06a4a45b7723598855e45557c1b9a2786393374924277" },
        },
    ],
    // tec9
    &[
        Spec {
            binding: "weapon_tec9.boltpull", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/tec9/tec9_boltpull.wav", size: 14060, sha256: "91d89123bedd7d3d562a68cae928533a9880072a427a4b0d376fc3a8bdfd6e82" },
            installed: WaveSpec { path: "sound/weapons/tec9/tec9_boltpull.wav", size: 22094, sha256: "5f65e01818f663270e6f19e42e515a1c58e99a7c71d52af4062c7fff4ac10e79" },
        },
        Spec {
            binding: "weapon_tec9.boltrelease", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/tec9/tec9_boltrelease.wav", size: 15184, sha256: "2f8d3b8fdd0d62dfc12aa93d4446df76e26350b00ba4f6499e518182eab41fdf" },
            installed: WaveSpec { path: "sound/weapons/tec9/tec9_boltrelease.wav", size: 44144, sha256: "7f76697f750fe8a848358513f70759eb8db4f5bff99d4401ce7697ab743f622d" },
        },
        Spec {
            binding: "weapon_tec9.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/tec9/tec9_clipin.wav", size: 20492, sha256: "679aaed586bf22fa1a15c269065a9500817c78f34dd0c8d33966faa98644a7ef" },
            installed: WaveSpec { path: "sound/weapons/tec9/tec9_clipin.wav", size: 33120, sha256: "ab69dc1be62e47923ff90ef93d10a43e4a22eda547bb67663455c2827598c1c4" },
        },
        Spec {
            binding: "weapon_tec9.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/tec9/tec9_clipout.wav", size: 10332, sha256: "acd2e6fe8eead628012e11e43f656bce259065d3e36b83a3b6adbbd1fc67a012" },
            installed: WaveSpec { path: "sound/weapons/tec9/tec9_clipout.wav", size: 30362, sha256: "6b0bd6f2aaba314ea08773fbe0b0bb62f4d780554e718a07688525c4ac4497e2" },
        },
        Spec {
            binding: "weapon_tec9.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/tec9/tec9_draw.wav", size: 87350, sha256: "d0bfc71b2f4540efe73184c93010b67ffca9455237a8ac2aa7d74a8cc22126a4" },
            installed: WaveSpec { path: "sound/weapons/tec9/tec9_draw.wav", size: 88244, sha256: "3a33d1740dd2538985fe06c84a14e09f3de6280a15a1a68d512f364a72e82820" },
        },
    ],
    // ump45
    &[
        Spec {
            binding: "weapon_ump45.boltback", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/ump45/ump45_boltback.wav", size: 11888, sha256: "cad26c15eb8ae6930fcb3ca3e9a7c09906fe22fe2cd29fd94c827f3d17f0f558" },
            installed: WaveSpec { path: "sound/weapons/ump45/ump45_boltback.wav", size: 15836, sha256: "89fc3b723745353971f9c0216c0364a0bd1483bf139c94e17a3fc1ad13ae260a" },
        },
        Spec {
            binding: "weapon_ump45.boltforward", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/ump45/ump45_boltforward.wav", size: 10308, sha256: "426eac2507312dd875e6b9ee0a1237300f1c3583d1d85a2ad6871f4e5c3874a5" },
            installed: WaveSpec { path: "sound/weapons/ump45/ump45_boltforward.wav", size: 12372, sha256: "99d25d9d2ecd0f325716037ed8b876cc2432958ab15c67d7a9cd99fd532b240f" },
        },
        Spec {
            binding: "weapon_ump45.clipin", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/ump45/ump45_clipin.wav", size: 30880, sha256: "1608e5d2d3e164acbbcf75e0fafb50068fc56d13eac108590c3048bbf5f8680b" },
            installed: WaveSpec { path: "sound/weapons/ump45/ump45_clipin.wav", size: 30880, sha256: "512405aea6ed3df5eb231269e5db26fe6cb738a119dba4b1ea568bd796d1cdf2" },
        },
        Spec {
            binding: "weapon_ump45.clipout", volume: 1.0, channel: 3,
            historical: WaveSpec { path: "sound/weapons/ump45/ump45_clipout.wav", size: 26680, sha256: "ca439877727a1626fc46eba68fe58eb0722a7c9744bce12ad65feffce8b846d7" },
            installed: WaveSpec { path: "sound/weapons/ump45/ump45_clipout.wav", size: 26678, sha256: "29e46b4596819d21eafd5a8a16304f93cc42fb2ee196f9a969dbd47b5cbc0a4b" },
        },
        Spec {
            binding: "weapon_ump45.draw", volume: 0.5, channel: 6,
            historical: WaveSpec { path: "sound/weapons/ump45/ump45_draw.wav", size: 65254, sha256: "df6c18afb58fe7992a9b7a381a8f0929e671a047ec844e532e0a318e1165abc9" },
            installed: WaveSpec { path: "sound/weapons/ump45/ump45_draw.wav", size: 71706, sha256: "748323004616184afbdf2cca029f3b60c1f9868524015aebc783164e4a39b6d4" },
        },
    ],
];

struct RetainedSpec<'a> {
    binding: &'static str,
    wave: WaveSpec<'a>,
    volume: f32,
    channel: u8,
}

// These four installed AK waves are byte-identical to archive 054 in the
// same June 2015 depot (directory length/CRC and full SHA256 checked).
// Reuse the verified installed bytes; no duplicate private assets required.
// Draw/BoltPull use CHAN_STATIC, clips CHAN_ITEM. All use PITCH_NORM and
// have no entry-limit stack. The shared M4 Draw is also CHAN_STATIC.
const RETAINED_RIFLES: &[RetainedSpec<'static>] = &[
    RetainedSpec {
        binding: "weapon_ak47.draw", volume: 0.5, channel: 6,
        wave: WaveSpec {
            path: "sound/weapons/ak47/ak47_draw.wav", size: 39884,
            sha256: "8118407efcf748c119e459d6dd87369fa714a36863dc11e735dc59c857376cd2",
        },
    },
    RetainedSpec {
        binding: "weapon_ak47.boltpull", volume: 1.0, channel: 6,
        wave: WaveSpec {
            path: "sound/weapons/ak47/ak47_boltpull.wav", size: 28846,
            sha256: "d6253073be8c071eb6112cdbf8212ac58f377dd7c5ee623c48b4b673dec5f36c",
        },
    },
    RetainedSpec {
        binding: "weapon_ak47.clipout", volume: 1.0, channel: 3,
        wave: WaveSpec {
            path: "sound/weapons/ak47/ak47_clipout.wav", size: 16706,
            sha256: "17953c6c9e57d9c050169830c18cd46edc7e55467c7b690298f43c4a7e0d72ec",
        },
    },
    RetainedSpec {
        binding: "weapon_ak47.clipin", volume: 1.0, channel: 3,
        wave: WaveSpec {
            path: "sound/weapons/ak47/ak47_clipin.wav", size: 15724,
            sha256: "bbf5d5429d709d422d2488189e7375e99437b88c7ef7d579e65edd2e4cdf825c",
        },
    },
];

// These additional installed waves are byte-identical to the independently
// verified June 2015 originals. Freeze verified installed bytes without adding
// redundant pack resources. Each exact binding remains independently guarded.
const RETAINED_ARSENAL: &[RetainedSpec<'static>] = &[
    RetainedSpec {
        binding: "weapon_aug.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/aug/aug_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
    },
    RetainedSpec {
        binding: "weapon_deagle.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/deagle/de_draw.wav", size: 87350, sha256: "d0bfc71b2f4540efe73184c93010b67ffca9455237a8ac2aa7d74a8cc22126a4" },
    },
    RetainedSpec {
        binding: "weapon_fiveseven.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/fiveseven/fiveseven_draw.wav", size: 87350, sha256: "d0bfc71b2f4540efe73184c93010b67ffca9455237a8ac2aa7d74a8cc22126a4" },
    },
    RetainedSpec {
        binding: "weapon_glock.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/glock18/glock_draw.wav", size: 87350, sha256: "d0bfc71b2f4540efe73184c93010b67ffca9455237a8ac2aa7d74a8cc22126a4" },
    },
    RetainedSpec {
        binding: "weapon_hkp2000.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/hkp2000/hkp2000_draw.wav", size: 87350, sha256: "d0bfc71b2f4540efe73184c93010b67ffca9455237a8ac2aa7d74a8cc22126a4" },
    },
    RetainedSpec {
        binding: "weapon_m249.boxin", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/m249/m249_boxin.wav", size: 69744, sha256: "5aece17058345807ab637de063f9c000467429944b2d2a249808e3b546dbd12f" },
    },
    RetainedSpec {
        binding: "weapon_m249.boxout", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/m249/m249_boxout.wav", size: 18008, sha256: "79cd64fbc2c1ca2ed201c7337f23b1aeca90d55d46cb4243fe562dabf59329cd" },
    },
    RetainedSpec {
        binding: "weapon_m249.chain", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/m249/m249_chain.wav", size: 262146, sha256: "d76f8538bad4fcb89a3da3a9702524bafa7e2d81715901e042eb3820463a8360" },
    },
    RetainedSpec {
        binding: "weapon_m249.coverdown", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/m249/m249_coverdown.wav", size: 20646, sha256: "ed896e433d9682b59b19346a0f57babfe92f6357025b56a8a0b75613e67f907a" },
    },
    RetainedSpec {
        binding: "weapon_m249.coverup", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/m249/m249_coverup.wav", size: 23752, sha256: "e3ac542997bf3dafad2a180589b1efbfd7ec846961eb4db2f416908dcd3e85ed" },
    },
    RetainedSpec {
        binding: "weapon_m249.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/m249/m249_draw.wav", size: 25892, sha256: "d4891a35bb3c044966489d9a5c9ba755f213ea4d5511c7cc170f685ba99d5704" },
    },
    RetainedSpec {
        binding: "weapon_m249.pump", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/m249/m249_pump.wav", size: 37472, sha256: "8517f774f0864849e2e6a6a6fdb467d508dad7fde34d9729f8a314de3c0d6183" },
    },
    RetainedSpec {
        binding: "weapon_negev.boxin", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/negev/negev_boxin.wav", size: 69644, sha256: "f79eeb3780afc8d812f3592e2815a16fdd746119ef372c20d99721704673e4ab" },
    },
    RetainedSpec {
        binding: "weapon_negev.boxout", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/negev/negev_boxout.wav", size: 17908, sha256: "654f7d813a84a302587fbaf9bc87ca503814d81c80291d8f4dfcae2d4fcf2f80" },
    },
    RetainedSpec {
        binding: "weapon_negev.chain", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/negev/negev_chain.wav", size: 55540, sha256: "75dcbf3ee2605b336cb74cc106c75a2ac8ef182b070a26ee11320776eb22135e" },
    },
    RetainedSpec {
        binding: "weapon_negev.coverdown", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/negev/negev_coverdown.wav", size: 20554, sha256: "5891d26ce1148e8816f46b627e41ef1fdfae60d6ee9b4d2b9bd5ba2682324b97" },
    },
    RetainedSpec {
        binding: "weapon_negev.coverup", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/negev/negev_coverup.wav", size: 23660, sha256: "6c4020a8bb09c2bd6bb6fdf72c2a0764e44e5752cae9838b00f71b41d7636ea2" },
    },
    RetainedSpec {
        binding: "weapon_negev.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/negev/negev_draw.wav", size: 25892, sha256: "d4891a35bb3c044966489d9a5c9ba755f213ea4d5511c7cc170f685ba99d5704" },
    },
    RetainedSpec {
        binding: "weapon_negev.pump", volume: 1.0, channel: 3,
        wave: WaveSpec { path: "sound/weapons/negev/negev_pump.wav", size: 37472, sha256: "8517f774f0864849e2e6a6a6fdb467d508dad7fde34d9729f8a314de3c0d6183" },
    },
    RetainedSpec {
        binding: "weapon_p250.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/p250/p250_draw.wav", size: 87350, sha256: "d0bfc71b2f4540efe73184c93010b67ffca9455237a8ac2aa7d74a8cc22126a4" },
    },
    RetainedSpec {
        binding: "weapon_sawedoff.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/sawedoff/sawedoff_draw.wav", size: 40124, sha256: "fe404396657d679a8a801320bbddeea89dc896571440257275b9529af3200538" },
    },
    RetainedSpec {
        binding: "weapon_xm1014.draw", volume: 0.5, channel: 6,
        wave: WaveSpec { path: "sound/weapons/xm1014/xm1014_draw.wav", size: 64994, sha256: "1a8bf18c3171df1d2c2a62f1f8b1935717acf5d2149abce1beb5d2dba9649897" },
    },
];

pub(super) fn waves() -> impl Iterator<Item = &'static WaveSpec<'static>> {
    GROUPS.iter().chain(ARSENAL_GROUPS).flat_map(|group| group.iter().map(|spec| &spec.historical))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tests::{private_pack, wav};
    use sha2::{Digest, Sha256};
    use std::{collections::BTreeMap, sync::Arc};

    fn entries(bindings: &[&str]) -> HashMap<String, Entry> {
        bindings.iter().map(|binding| {
            let doc = kv::parse(&format!(r#""{binding}" {{
                "wave" "weapons/test/current.wav" "channel" "CHAN_ITEM"
                "volume" "0.5, 1.0" "pitch" "97, 105" "soundlevel" "SNDLVL_65dB"
                "operator_stacks" {{
                    "start_stack" {{ "import_stack" "CS_limit_start" }}
                    "update_stack" {{ "import_stack" "CS_update_foley" }}
                }}
            }}"#)).unwrap();
            let entry = super::super::super::parse_entry(doc.block(binding).unwrap()).unwrap();
            (binding.to_string(), entry)
        }).collect()
    }

    #[test]
    fn nova_aliases_restore_metadata_only_with_their_verified_wave() {
        let old = wav(7000);
        let current = wav(1000);
        let old_hash = format!("{:x}", Sha256::digest(&old));
        let current_hash = format!("{:x}", Sha256::digest(&current));
        let primary = Spec {
            binding: "weapon_nova.pumpduringshot",
            historical: WaveSpec { path: "sound/old_distant.wav", size: old.len(), sha256: &old_hash },
            installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
            ..NOVA
        };
        let alias = Spec {
            historical: primary.historical, installed: primary.installed,
            ..GROUPS[1][0]
        };
        // Missing/changed aliases must not disable the primary; likewise a
        // missing primary cannot disable a separately recognized draw/reload.
        for case in ["valid", "missing alias", "changed alias", "missing primary", "changed bytes", "corrupt old", "old pack"] {
            let payload = if case == "corrupt old" { wav(6999) } else { old.clone() };
            let historical = Historical::from_pack(
                // The near resource stands in for the existing shots; old
                // packs remain valid without the optional handling resource.
                private_pack(&current, (case != "old pack").then_some(payload.as_slice()), "classic2015", false),
                &[WaveSpec { path: "sound/old_near.wav", size: current.len(), sha256: &current_hash }, primary.historical],
            ).unwrap();
            let mut fs = vfs::Vfs::new();
            fs.push_back("installed", vfs::Mount::Memory(BTreeMap::from([
                (primary.installed.path.into(), Arc::from(if case == "changed bytes" { wav(999) } else { current.clone() })),
            ])));
            let mut entries = entries(&[primary.binding, alias.binding]);
            match case {
                "missing alias" => { entries.remove(alias.binding); }
                "changed alias" => { entries.get_mut(alias.binding).unwrap().waves[0] = "sound/custom.wav".into(); }
                "missing primary" => { entries.remove(primary.binding); }
                _ => {}
            }
            let before = entries.clone();
            install_groups(&fs, &mut entries, &historical, &[&[primary], &[alias]]);
            for (binding, mut entry) in entries {
                let expected = match binding.as_str() {
                    "weapon_nova.pumpduringshot" => matches!(case, "valid" | "missing alias" | "changed alias"),
                    _ => matches!(case, "valid" | "missing primary"),
                };
                assert_eq!(entry.frozen_wave.is_some(), expected, "{case}: {binding}");
                if let Some(wave) = entry.frozen_wave.take() {
                    assert_eq!(wave.bytes.as_ref(), old);
                    assert_eq!(wave.channels, 1);
                }
                let mut expected_entry = before[&binding].clone();
                if expected {
                    let volume = if binding == primary.binding { 0.3 } else { 1.0 };
                    expected_entry.volume = (volume, volume);
                    expected_entry.pitch = (100.0, 100.0);
                    expected_entry.channel = 3; // CHAN_ITEM
                    expected_entry.limit = None;
                }
                assert_eq!(format!("{entry:?}"), format!("{expected_entry:?}"), "metadata: {case}: {binding}");
            }
        }
    }

    #[test]
    fn mag_pumps_still_publish_only_as_a_complete_verified_pair() {
        let old_back = wav(7000);
        let old_forward = wav(-8000);
        let current = wav(1000);
        let back_hash = format!("{:x}", Sha256::digest(&old_back));
        let forward_hash = format!("{:x}", Sha256::digest(&old_forward));
        let current_hash = format!("{:x}", Sha256::digest(&current));
        let specs = [Spec {
            binding: "weapon_mag7.pumpback",
            historical: WaveSpec { path: "sound/old_near.wav", size: old_back.len(), sha256: &back_hash },
            installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
            ..GROUPS[2][0]
        }, Spec {
            binding: "weapon_mag7.pumpforward",
            historical: WaveSpec { path: "sound/old_distant.wav", size: old_forward.len(), sha256: &forward_hash },
            installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
            ..GROUPS[2][1]
        }];
        for case in ["valid", "missing partner", "corrupt partner", "changed binding"] {
            let far = if case == "corrupt partner" { wav(-7999) } else { old_forward.clone() };
            let historical = Historical::from_pack(
                private_pack(&old_back, (case != "missing partner").then_some(far.as_slice()), "classic2015", false),
                &[specs[0].historical, specs[1].historical],
            ).unwrap();
            let mut fs = vfs::Vfs::new();
            fs.push_back("installed", vfs::Mount::Memory(BTreeMap::from([
                (specs[0].installed.path.into(), Arc::from(current.clone())),
            ])));
            let mut entries = entries(&[specs[0].binding, specs[1].binding]);
            entries.get_mut(specs[0].binding).unwrap().pitch = (150.0, 150.0);
            for spec in &specs { entries.get_mut(spec.binding).unwrap().channel = 6; }
            if case == "changed binding" { entries.get_mut(specs[1].binding).unwrap().waves.clear(); }
            let before = entries.clone();
            install_groups(&fs, &mut entries, &historical, &[&specs]);
            for (spec, expected) in specs.iter().zip([&old_back, &old_forward]) {
                let mut entry = entries[spec.binding].clone();
                let wave = entry.frozen_wave.take();
                assert_eq!(wave.is_some(), case == "valid", "{case}");
                if let Some(wave) = wave { assert_eq!(wave.bytes.as_ref(), expected.as_slice()); }
                let mut expected_entry = before[spec.binding].clone();
                if case == "valid" {
                    expected_entry.volume = (1.0, 1.0);
                    expected_entry.pitch = (100.0, 100.0);
                    expected_entry.channel = 3;
                    expected_entry.limit = None;
                }
                assert_eq!(format!("{entry:?}"), format!("{expected_entry:?}"), "metadata: {case}: {}", spec.binding);
            }
        }
    }

    #[test]
    fn retained_ak_metadata_requires_the_recognized_binding_and_identical_bytes() {
        let current = wav(1000);
        let hash = format!("{:x}", Sha256::digest(&current));
        assert_eq!(RETAINED_RIFLES.len(), 4);
        for (stock, (binding, volume, channel)) in RETAINED_RIFLES.iter().zip([
            ("weapon_ak47.draw", 0.5, 6),
            ("weapon_ak47.boltpull", 1.0, 6),
            ("weapon_ak47.clipout", 1.0, 3),
            ("weapon_ak47.clipin", 1.0, 3),
        ]) {
            assert_eq!((stock.binding, stock.volume, stock.channel), (binding, volume, channel));
            let spec = RetainedSpec {
                binding, volume: stock.volume, channel: stock.channel,
                wave: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &hash },
            };
            for case in ["valid", "changed bytes", "missing bytes", "changed binding", "multiple waves", "distant", "wrong category", "already frozen"] {
                let mut fs = vfs::Vfs::new();
                if case != "missing bytes" {
                    fs.push_back("installed", vfs::Mount::Memory(BTreeMap::from([
                        (spec.wave.path.into(), Arc::from(if case == "changed bytes" { wav(999) } else { current.clone() })),
                    ])));
                }
                let mut entries = entries(&[binding]);
                let entry = entries.get_mut(binding).unwrap();
                entry.channel = channel;
                match case {
                    "changed binding" => entry.waves[0] = "sound/custom.wav".into(),
                    "multiple waves" => entry.waves.push(spec.wave.path.into()),
                    "distant" => entry.distant = Some("custom.distant".into()),
                    "wrong category" => entry.category = super::super::super::Category::Weapon,
                    "already frozen" => entry.frozen_wave = Some(FrozenWave { bytes: Arc::from(current.clone()), channels: 1 }),
                    _ => {}
                }
                let mut expected = entry.clone();
                install_retained_rifles(&fs, &mut entries, std::slice::from_ref(&spec));
                let mut actual = entries[binding].clone();
                if case == "valid" {
                    assert_eq!(actual.frozen_wave.take().unwrap().bytes.as_ref(), current);
                    expected.volume = (volume, volume);
                    expected.pitch = (100.0, 100.0);
                    expected.channel = channel;
                    expected.limit = None;
                }
                assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{binding}: {case}");
            }
        }
    }

    #[test]
    fn m4_handling_restores_metadata_only_as_a_complete_verified_set() {
        let old = wav(7000);
        let current = wav(1000);
        let old_hash = format!("{:x}", Sha256::digest(&old));
        let current_hash = format!("{:x}", Sha256::digest(&current));
        let expected = [
            ("weapon_m4a1.draw", 0.5, 6),
            ("weapon_m4a1.clipout", 1.0, 3),
            ("weapon_m4a1.clipin", 1.0, 3),
            ("weapon_m4a1.boltforward", 1.0, 3),
            ("weapon_m4a1.boltback", 1.0, 3),
        ];
        assert_eq!(GROUPS[3].len(), expected.len());
        let specs: Vec<_> = GROUPS[3].iter().zip(expected).enumerate().map(|(i, (spec, expected))| {
            assert_eq!((spec.binding, spec.volume, spec.channel), expected);
            Spec {
                historical: WaveSpec { path: if i == 4 { "sound/old_distant.wav" } else { "sound/old_near.wav" }, size: old.len(), sha256: &old_hash },
                installed: WaveSpec { path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash },
                ..*spec
            }
        }).collect();
        for case in ["valid", "old pack", "corrupt partner", "changed bytes", "changed binding", "missing binding"] {
            let far = if case == "corrupt partner" { wav(6999) } else { old.clone() };
            let historical = Historical::from_pack(
                private_pack(&old, (case != "old pack").then_some(far.as_slice()), "classic2015", false),
                &[specs[0].historical, specs[4].historical],
            ).unwrap();
            let mut fs = vfs::Vfs::new();
            fs.push_back("installed", vfs::Mount::Memory(BTreeMap::from([
                (specs[0].installed.path.into(), Arc::from(if case == "changed bytes" { wav(999) } else { current.clone() })),
            ])));
            let mut bindings: Vec<_> = specs.iter().map(|spec| spec.binding).collect();
            // The actual M4A1-S model invokes its own bolts and the newer
            // ClipHit; these must survive even when the recovered set succeeds.
            bindings.extend(["weapon_m4a1s.boltback", "weapon_m4a1s.boltforward", "weapon_m4a1.cliphit"]);
            let mut entries = entries(&bindings);
            if case == "changed binding" { entries.get_mut(specs[4].binding).unwrap().waves.clear(); }
            if case == "missing binding" { entries.remove(specs[4].binding); }
            let before = entries.clone();
            install_groups(&fs, &mut entries, &historical, &[&specs]);
            for (binding, mut entry) in entries {
                let mut expected_entry = before[&binding].clone();
                if case == "valid" {
                    if let Some((_, volume, channel)) = expected.iter().find(|(name, _, _)| *name == binding) {
                        assert_eq!(entry.frozen_wave.take().unwrap().bytes.as_ref(), old);
                        expected_entry.volume = (*volume, *volume);
                        expected_entry.pitch = (100.0, 100.0);
                        expected_entry.channel = *channel;
                        expected_entry.limit = None;
                    }
                }
                assert_eq!(format!("{entry:?}"), format!("{expected_entry:?}"), "{binding}: {case}");
            }
        }
    }

    #[test]
    fn arsenal_group_requires_both_verified_waves_before_restoring_metadata() {
        let original = [wav(7000), wav(-8000)];
        let current = wav(1000);
        let hashes: Vec<_> = original.iter().map(|wave| format!("{:x}", Sha256::digest(wave))).collect();
        let current_hash = format!("{:x}", Sha256::digest(&current));
        // One representative recovered draw/bolt group, with different
        // historical volumes/channels. Unknown later bindings stay installed.
        let stock = [ARSENAL_GROUPS[1][0], ARSENAL_GROUPS[1][4]];
        let specs: Vec<_> = stock.iter().enumerate().map(|(i, spec)| Spec {
            historical: WaveSpec {
                path: ["sound/old_near.wav", "sound/old_distant.wav"][i],
                size: original[i].len(), sha256: &hashes[i],
            },
            installed: WaveSpec {
                path: "sound/weapons/test/current.wav", size: current.len(), sha256: &current_hash,
            },
            ..*spec
        }).collect();
        for case in ["valid", "partial pack", "corrupt partner", "changed installed", "changed binding"] {
            let partner = if case == "corrupt partner" { wav(-7999) } else { original[1].clone() };
            let historical = Historical::from_pack(
                private_pack(&original[0], (case != "partial pack").then_some(partner.as_slice()), "classic2015", false),
                &[specs[0].historical, specs[1].historical],
            ).unwrap();
            let mut fs = vfs::Vfs::new();
            fs.push_back("installed", vfs::Mount::Memory(BTreeMap::from([
                (specs[0].installed.path.into(), Arc::from(if case == "changed installed" { wav(999) } else { current.clone() })),
            ])));
            let unknown = "weapon_mp5.draw";
            let mut entries = entries(&[specs[0].binding, specs[1].binding, unknown]);
            if case == "changed binding" { entries.get_mut(specs[1].binding).unwrap().waves.clear(); }
            let before = entries.clone();
            install_groups(&fs, &mut entries, &historical, &[&specs]);
            for (binding, mut entry) in entries {
                let mut expected = before[&binding].clone();
                if case == "valid" {
                    if let Some(i) = specs.iter().position(|spec| spec.binding == binding) {
                        assert_eq!(entry.frozen_wave.take().unwrap().bytes.as_ref(), original[i]);
                        expected.volume = (specs[i].volume, specs[i].volume);
                        expected.pitch = (100.0, 100.0);
                        expected.channel = specs[i].channel;
                        expected.limit = None;
                    }
                }
                assert_eq!(format!("{entry:?}"), format!("{expected:?}"), "{case}: {binding}");
            }
        }
    }

    #[test]
    fn arsenal_constants_remain_bounded_and_exclude_unknown_later_bindings() {
        use std::collections::HashSet;
        let recovered: Vec<_> = ARSENAL_GROUPS.iter().flat_map(|group| group.iter()).collect();
        assert_eq!(ARSENAL_GROUPS.len(), 21);
        assert_eq!(recovered.len(), 101);
        assert_eq!(recovered.iter().map(|spec| spec.historical.path).collect::<HashSet<_>>().len(), 100);
        assert_eq!(RETAINED_ARSENAL.len(), 22);
        let bindings: HashSet<_> = recovered.iter().map(|spec| spec.binding)
            .chain(RETAINED_ARSENAL.iter().map(|spec| spec.binding)).collect();
        assert_eq!(bindings.len(), 123);
        for unknown in ["weapon_mp5.draw", "weapon_revolver.draw", "weapon_sawedoff.insertshell", "weapon_xm1014.insertshell",
            "weapon_m4a1s.boltback", "weapon_awp.boltback"] {
            assert!(!bindings.contains(unknown), "{unknown}");
        }
        for spec in recovered {
            assert!(spec.historical.path.starts_with("sound/weapons/"));
            assert!(spec.installed.path.starts_with("sound/weapons/"));
            assert_eq!(spec.historical.sha256.len(), 64);
            assert_eq!(spec.installed.sha256.len(), 64);
            assert!(spec.historical.size < (2 << 20) && spec.installed.size < (2 << 20));
            assert!(matches!(spec.channel, 3 | 6));
            assert!([0.3, 0.5, 1.0].contains(&spec.volume));
        }
    }

    #[test]
    fn installed_mode_leaves_all_handling_metadata_untouched() {
        let bindings: Vec<_> = GROUPS.iter().chain(ARSENAL_GROUPS).flat_map(|group| group.iter().map(|spec| spec.binding))
            .chain(RETAINED_RIFLES.iter().chain(RETAINED_ARSENAL).map(|spec| spec.binding)).collect();
        let mut entries = entries(&bindings);
        entries.get_mut("weapon_mag7.pumpback").unwrap().pitch = (150.0, 150.0);
        let before = entries.clone();
        super::super::Profile::load(&vfs::Vfs::new(), &mut entries, super::super::SoundEra::InstalledLegacy);
        for (binding, entry) in entries {
            assert_eq!(format!("{entry:?}"), format!("{:?}", before[&binding]), "{binding}");
        }
    }
}

fn verified_group(
    fs: &vfs::Vfs,
    entries: &HashMap<String, Entry>,
    historical: &Historical,
    specs: &[Spec<'_>],
) -> Result<Vec<FrozenWave>, &'static str> {
    specs.iter().map(|spec| {
        // Refuse changed installs/selected-pack overrides instead of applying
        // a historical wave to an unrecognized installed sound declaration.
        verified_installed(fs, entries, spec.binding, &spec.installed)?;
        historical.wave(spec.historical.path)
    }).collect()
}

fn verified_installed(
    fs: &vfs::Vfs,
    entries: &HashMap<String, Entry>,
    binding: &str,
    wave: &WaveSpec<'_>,
) -> Result<FrozenWave, &'static str> {
    let entry = entries.get(binding).ok_or("installed handling entry missing")?;
    if entry.waves.len() != 1 || entry.waves[0] != wave.path
        || entry.distant.is_some() || entry.frozen_wave.is_some()
        || entry.category != super::super::Category::Foley
    {
        return Err("installed handling binding differs");
    }
    verified_wave(fs, wave)
}

fn restore(entry: &mut Entry, volume: f32, channel: u8, wave: FrozenWave) {
    entry.volume = (volume, volume);
    entry.pitch = (100.0, 100.0);
    entry.channel = channel;
    entry.limit = None;
    entry.frozen_wave = Some(wave);
}

pub(super) fn install(
    fs: &vfs::Vfs,
    entries: &mut HashMap<String, Entry>,
    historical: &Historical,
) {
    install_groups(fs, entries, historical, GROUPS);
    install_groups(fs, entries, historical, ARSENAL_GROUPS);
    // The caller admits handling only in Classic2015 with a valid private
    // pack. Older packs also permit independently verified identical-byte
    // metadata restoration; missing recovered resources retain their entire group.
    install_retained_rifles(fs, entries, RETAINED_RIFLES);
    install_retained_rifles(fs, entries, RETAINED_ARSENAL);
}

fn install_retained_rifles(
    fs: &vfs::Vfs,
    entries: &mut HashMap<String, Entry>,
    specs: &[RetainedSpec<'_>],
) {
    for spec in specs {
        if let Ok(wave) = verified_installed(fs, entries, spec.binding, &spec.wave) {
            if let Some(entry) = entries.get_mut(spec.binding) {
                restore(entry, spec.volume, spec.channel, wave);
            }
        }
    }
}

fn install_groups(
    fs: &vfs::Vfs,
    entries: &mut HashMap<String, Entry>,
    historical: &Historical,
    groups: &[&[Spec<'_>]],
) {
    for group in groups {
        // Validate every group member before publishing any of them. An old
        // shot-only pack, absent/invalid resource or changed declaration leaves
        // all installed handling in this group untouched.
        if let Ok(waves) = verified_group(fs, entries, historical, group) {
            for (spec, wave) in group.iter().zip(waves) {
                if let Some(entry) = entries.get_mut(spec.binding) {
                    // Existing foley dispatch uses these bindings directly.
                    // Publish historical metadata only with the verified wave;
                    // failed groups never alter installed entries.
                    restore(entry, spec.volume, spec.channel, wave);
                }
            }
        }
    }
}
