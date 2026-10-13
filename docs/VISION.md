# Vision

This is the owner's intent for the project, recorded so nobody has to explain
it again. Read it before planning any work. If a task conflicts with it, stop
and ask. [ROADMAP.md](ROADMAP.md) is how we get there.

## The goal

A **full rewrite of the original Counter-Strike: Global Offensive in Rust on
Bevy**, which the owner and their friends play together. It is part of the
wider movement of rewriting old games in Bevy. Because they share an engine,
those rewrites can be **crossed together**: MW2 guns in CS:GO, CS:GO in
Minecraft, and so on (see [STITCHING.md](STITCHING.md)).

This repository is a fork of hawk's original project (credited as "Mr.
Claude"). The owner picked it up on 2026-10-11, after it had been idle for
about a week.

## Who it is for

The owner and their friends. Friends will not spend 15 minutes setting
anything up. **Every feature is judged by friction.** The target is under two
minutes from a Discord link to being in a game, with no terminal, no manual
file copying and no reading.

## Requirements

### 1. Distribution: one download, then done

- Friends get one installer (or exe) from GitHub Releases.
- The **only prerequisite** is CS:GO Legacy installed through Steam (now its
  own Steam app, ID 4465480, installed to `steamapps\common\csgo legacy`).
- The installer **finds CS:GO Legacy automatically** in every Steam library.
  If it cannot, the player picks the folder. Nothing else is asked.
- Nice to have: the game shows up in the Steam library so it can be launched
  from Steam (as a non-Steam shortcut with artwork; see the roadmap).
- **Steam is not needed to play.** Steam is only used to install CS:GO Legacy
  and, optionally, as a launcher.

### 2. Updates without new installers

- Every build comes from **GitHub Actions CI** and is published to
  **GitHub Releases**.
- The installed game **updates itself** from GitHub Releases. The owner never
  sends friends a new installer.

### 3. No DMCA exposure

- The public repository and the public releases contain **no copyrighted game
  assets** (Valve, Activision, Mojang or anyone else) and **no leaked source
  code**.
- Game content is read at runtime from the player's own installs.
- This rules out the leaked CS:GO source. Using it is the surest way to get
  the repository taken down, which defeats the purpose. The project stays
  clean-room.

### 4. Private sideloading for friends

- The owner wants to give friends extra content that cannot be public (for
  example the **Intervention from MW2**, or Minecraft content). It is shared
  outside the repository: dropped in Discord, or pushed by our own server
  when they join.
- **Install friction must be near zero:** drag the file onto the game (or
  double-click it) and it works. Better still, joining a server that uses a
  pack downloads it automatically.
- These packs are never committed, never attached to public releases and
  never fetched by CI.

### 5. Play together: servers and a server browser

- The owner can **host servers** that they and their friends join, either
  on their own PC or in the cloud.
- **128 tick.** Servers run at 128 ticks per second, as the community
  servers did. 128-tick CS:GO feels better than 64.
- A **community server browser** lists them, as in CS:GO, plus direct
  connect.
- Works without Steam.

### 6. Easy to mod and to cross with other Bevy rewrites

- Bringing in content from other Bevy rewrites (MW2 / IW4L, Minecraft, Skate 3,
  Spider-Man 2, anything people reverse-engineer) is a first-class feature,
  not a hack. Mods bring **mechanics as well as assets**.
- Nothing from other games lives in this repo. Other people build versions
  of their rewrites that fit our **connection layer** (a stable mod API),
  and players bring those in themselves. That keeps legal responsibility with
  whoever distributes the mod. (The previous author's vendored Minecraft code
  was removed on 2026-10-11 for this reason; it remains in git history.)
- Mods follow the same low-friction rule as sideloading: one file, dropped
  in, done, and shared automatically between friends on a server.

### 7. Look and feel: early CS:GO, measured 1:1

- The menu is the **classic Scaleform UI**: the Flash-based menu CS:GO used
  from 2012 until August 2018, shown in the owner's screenshots. It is not
  the Panorama UI that replaced it and ships in CS:GO Legacy. (This file
  first called it "2018 Panorama"; research showed the screenshots are
  Scaleform. See [research/2018-era.md](research/2018-era.md) §0.)
  References:
  - [reference/menu-2018-home.png](reference/menu-2018-home.png): main menu,
    about August 2018 (the "A New Horizon" / Horizon Case news post).
  - [reference/menu-2018-case-opening.png](reference/menu-2018-case-opening.png):
    case opening (Chroma Case).
- **Pixel-perfect is the target.** Measure against the references; do not
  eyeball it.
- **Old everything:** the old sounds, guns, maps (old Inferno, old Nuke, old
  Dust II, ...; many are on the Steam Workshop) and the old case-opening
  experience. It should be identical to how it felt back then. **No creative
  liberties.** Research the original (footage, old builds' files, other
  restoration projects) and reproduce it.

What the reference screenshots show, as a checklist:

- **Top bar:** full width, dark, centred tabs with thin separators: home
  icon, PLAY, INVENTORY, WATCH, AWARDS, OPTIONS, power icon.
- **Background:** blurred blue-grey vignette with a large faded CS:GO
  watermark on the left.
- **Left column, top:** "Prime Account" badge above a profile card: avatar,
  name, a row of medals/coins with an arrow, commend counts (friendly,
  teacher, leader), profile rank ("Private Rank 3"), an XP bar labelled
  "Rank 4 + Drop", and the competitive skill group emblem ("Gold Nova III")
  with a trophy win count.
- **Left column, bottom:** friends panel with tabs (friends count, mail,
  recent players count, groups count), "Friends" heading, add-friend button,
  and a two-column list of avatars with name and status (green for in-game,
  with rich presence). The case-opening shot shows an earlier single-column
  "Friends · N Online" variant.
- **Right column, top:** Overwatch case banner ("DOWNLOAD EVIDENCE"), then a
  store carousel (StatTrak offer with discount badge and countdown, new
  case with "View On Market", Name Tag with price) with page dots and arrows.
- **Right column, bottom:** News panel embedding the blog (BLOG / UPDATES /
  RESOURCES / COMMUNITY tabs, post title, date, body, image), with player
  count, workshop queue and map tiles alongside.
- **Case opening:** horizontal reel of item cards with a gold centre line,
  "Unlocking Container..." status bar, and "Items that might be in this case"
  in a grid of cards: weapon render, name/finish, and a rarity colour strip
  (blue, purple, pink, red, gold "Rare Special Item" with a ? coin).

Online-only elements (Overwatch, store prices, news, friends) are recreated
locally. Friends become our own server's player list; news can come from our
GitHub releases; the store becomes the local case/cosmetic system.

Owner clarification from the 2026-10-12 desktop playtest: the baseline gameplay,
weapon sounds, movement, animation and HUD should feel like opening CS:GO in
2015. In particular, use the original AK-47 firing sound and the correct
silenced M4A1-S sound. Keep the supplied classic Scaleform menu references.
Later quality-of-life additions follow a faithful base. Installed 2023 Legacy
data is a development reference, not proof that a sound or mechanic matches
2015; historical content still follows the runtime-only content rules.

The follow-up playtest reinforced that a generally plausible CS:GO imitation
is insufficient. Reproduce the original movement, jumping, gun behavior and
audio mixing in Rust/Bevy, while preserving the shared-engine connection layer.
**Surfing must match CS:GO too:** ramp contact, air control, sliding, corners,
seams and speed retention need measured regression coverage at the relevant
tick rate and recorded server movement settings. Surf servers' settings can
differ from competitive defaults; an undocumented "surf feel" preset is not
proof of parity. Correct waveforms alone do not establish original sound tone,
EQ, voice overlap, speaker routing or DSP. The owner need not supply clips
before this research proceeds.

### 8. VR cross-play (later, but designed for now)

- The owner plays in **VR on a Quest** (through Steam Link or a similar
  PC-VR bridge, so the PC renders through OpenXR). Friends play on flat
  screens **in the same match**.
- Flat players see the VR player's **real head and hand movements**. Guns
  and all mechanics work the same in VR.
- This is planned for after multiplayer, but the network protocol and player
  state must carry tracked head/hand poses from day one so VR does not need
  a rewrite.

## Working agreements

- **No human-scale time estimates.** Work is done by parallel agents.
  Describe plans as phases, dependencies and parallel tracks. Never say
  "six months to two years"; the first playable multiplayer build is a
  matter of days.
- **Friction is a bug.** Any step a friend has to take is a candidate for
  removal.
- **Agents own git.** Agents commit, push, open and merge PRs and tag
  releases. The owner never does (see AGENTS.md).
- **No code signing.** Installers stay unsigned; players click through
  SmartScreen once ("More info → Run anyway"), and the docs say so.
- **Fidelity is measured.** Compare against references and the installed game,
  not memory.
