#!/bin/sh
# Regenerate the Apple string artifacts from shared/strings.json. The Xcode schemes call this
# before the build, so the catalog in each target's source folder always matches the table.

set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
python3 scripts/gen-apple-strings.py
