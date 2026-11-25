#!/bin/sh
# Keep every source file within the 500-line rule. This is the same check CI runs.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

find crates app-macos/Sources app-ios/Sources \
     app-android/app/src/main/kotlin app-windows app-linux/src shared/swift \
     -type f \( -name '*.rs' -o -name '*.swift' -o -name '*.kt' -o -name '*.cs' \) -print0 |
    xargs -0 wc -l |
    awk '$2 != "total" && $1 > 500 { print $0; bad = 1 } END { exit bad }'
