# Original CS:GO menu music

The menu currently has no music backend. This change loads the explicit
original CS:GO main-menu track from the player's mounted content and connects
the original `snd_menumusic_volume` slider to real stereo playback. Music gain
is independent of ordinary weapon/ambient gain, and still follows master volume
and focus muting. A single persistent front-end player spans menu pages and
stops on leaving the front end; returning Home creates a fresh player.

Content identity is `csgo_sound_music_valve_csgo_01_mainmenu`. The declaration
and audio remain runtime-only. Content packs can provide the same declaration
and wave through the mounted VFS; neither is embedded or copied to the repo.
The selection explicitly requires the original `valve_csgo_01` namespace and
its menu-volume convar. It never chooses the first/default music-kit index or
the CS2-named kit metadata retained by a newer install.

The installed declaration binds the main-menu MP3 to a music entry with volume
0.8 and the menu cue convar. Its update stack selects the SelectedMusic mixer;
the retained default mixer has SelectedMusic0.6 versus Weapons1 0.7. That ratio
is an initial backend normalization, not a measured full Source music mix.
The bounded native lighting packet also queried the public music convars:
`snd_menumusic_volume` reports current0.3 and **default1.0**; the fixed music
gain and `snd_gain` report1, with `Default_Mix` selected. New local settings
use the observed default1.0, not the archived personal value. This is a2023
Legacy default observation, not an independent2015 default measurement.

Complete music operators, DSP, dashboard gain, historical default volume,
loop boundaries and transition fades require independent observation. Round,
MVP and objective music controls remain hidden until they have real backends.

Frozen `e3227d6` passed the client suite, including parser gates and the real
rodio-sink regression for live/pending gain, music-only muting, master muting
and settings round-trip. The PLAY launcher and real-data probe/simulation passed
with zero reported failures. Its immutable executable then completed the actual
Home/match/Pause/Audio/Back/Resume/Home/Inferno lifecycle, exiting successfully
without errors or changes to the owner's prior configuration. Logs show exactly
two original-track starts, two real sink creations and two stops before loading
matches. These are actual installed decoding/lifecycle observations, not a PCM
onset or final-mix measurement.

The coordinating agent independently viewed the Audio, returned-Home and
Inferno captures. Audio has three readable original rows: Master Volume,
background audio and Main Menu Volume, with the last value1. There is no stray
controller panel. This is partial three-image review, not a claim to have
reviewed all eight images or physically dragged the music slider. The general
slider callback and persistence path is shared; exact interaction/focus playback
and historical mixing still benefit from the owner's next manual playtest.

Full workspace CI and the combined RELEASE gate passed before PR159 merged.
Frozen union3be64ff produced immutable76EAA58B and passed workspace tests,
DustII probe/simulation and all1,425 sound decodes. Its actual eight-image
Home/DustII/Pause/Audio/Back/Resume/Home/Inferno lifecycle exited0; root reviewed
all eight images, superseding the earlier partial-review scope for that later
packet only. Exactly2 track starts,2 actual sinks and2 stops before match loads
were observed with owner configuration unchanged. The newer RELEASE9197101C
Video roundtrip independently retains the same2/2/2 lifecycle. Neither packet
proves physical Alt-Tab/slider behavior, PCM onset or complete2015 audio fidelity.
