#!/usr/bin/env bash
# Fails if the repository tracks third-party game assets. Game content is read
# at runtime from the player's own installs (see docs/VISION.md, "No DMCA
# exposure"). Reference screenshots in docs/reference are the only images.
set -euo pipefail
cd "$(dirname "$0")/.."

banned='\.(csrpack|vpk|bsp|nav|vtf|vmt|mdl|vvd|vtx|phy|ani|pcf|wav|mp3|ogg|bik|webm|pbin|ttf|otf|uifont|jar|dds|tga|pak|ff|iwd|xpak|ipak|swf|gfx)$'
hits=$(git ls-files | grep -iE "$banned" || true)

# Images are only allowed as documentation references.
images=$(git ls-files | grep -iE '\.(png|jpe?g|gif|bmp)$' | grep -v '^docs/reference/' || true)

# Minecraft resource/data trees.
minecraft=$(git ls-files | grep -iE '(^|/)assets/minecraft/|(^|/)data/minecraft/' || true)

all=$(printf '%s\n%s\n%s\n' "$hits" "$images" "$minecraft" | sed '/^$/d')
if [ -n "$all" ]; then
    echo "Game assets must not be committed. Read them from the player's install or use a content pack:"
    echo "$all"
    exit 1
fi
echo "No game assets tracked ($(git ls-files | wc -l) files checked)."
