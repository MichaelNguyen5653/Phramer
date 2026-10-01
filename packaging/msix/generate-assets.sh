#!/usr/bin/env bash
# Regenerates the MSIX visual assets in Assets/ from the app icon. Run by hand
# after the icon changes; the results are committed, so CI needs no image
# tooling. Needs ImageMagick.
#
# Names follow the MSIX resource-qualifier convention; makepri indexes them
# and Windows picks the scale or target size it needs. Every file must stay
# under 200 KB, a Windows App Certification Kit requirement.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
app="$here/../../data/img/app"
out="$here/Assets"
master="$app/appicon-512.png"
mkdir -p "$out"

# $1 output, $2 canvas size, $3 icon size within it
render() {
    convert "$master" -filter Lanczos -resize "$3x$3" \
        -background none -gravity center -extent "$2x$2" \
        -strip PNG32:"$out/$1"
}

for scale in 100 200 400; do
    f=$((scale))
    render "StoreLogo.scale-$scale.png" $((50 * f / 100)) $((42 * f / 100))
    render "Square44x44Logo.scale-$scale.png" $((44 * f / 100)) $((36 * f / 100))
    render "Square150x150Logo.scale-$scale.png" $((150 * f / 100)) $((100 * f / 100))
done

# Taskbar, Start list and title bar sizes. The unplated copies are what
# keeps Windows from drawing a coloured backplate behind the icon.
for size in 16 24 32 48 256; do
    render "Square44x44Logo.targetsize-$size.png" "$size" "$size"
    cp "$out/Square44x44Logo.targetsize-$size.png" \
        "$out/Square44x44Logo.targetsize-${size}_altform-unplated.png"
done

for f in "$out"/*.png; do
    if [ "$(stat -c %s "$f")" -ge 204800 ]; then
        echo "$f is 200 KB or more" >&2
        exit 1
    fi
done
