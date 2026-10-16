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
Complete music operators, DSP, dashboard gain, historical default volume,
loop boundaries and transition fades require independent observation. Round,
MVP and objective music controls remain hidden until they have real backends.

Verification is pending. Parser gates and a real sink regression cover content
selection and live/pending gain changes. Actual installed decoding, rendered
Audio widgets, persistence, focus, single-player lifetime and match transitions
must pass before this is offered as a playable change. A working original
track and slider will not establish complete 2015 audio fidelity.
