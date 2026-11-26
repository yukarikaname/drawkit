"""Fail when a view uses a literal that the shared table does not translate.

The catalog is generated from shared/strings.json, so a new `Text("...")` without a row would build
and then show English in every language. This walker only looks at the argument positions that take
a LocalizedStringKey, which keeps SF Symbol names and identifiers out of the way.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "shared/apple/Localizable.xcstrings"

PATTERNS = [
    r'\bText\("((?:[^"\\]|\\.)*)"\)',
    r'\bLabel\("((?:[^"\\]|\\.)*)",',
    r'\.help\("((?:[^"\\]|\\.)*)"\)',
    r'\bButton\("((?:[^"\\]|\\.)*)"\)',
    r'\bToggle\("((?:[^"\\]|\\.)*)",',
    r'\bPicker\("((?:[^"\\]|\\.)*)",',
    r'\bSection\("((?:[^"\\]|\\.)*)"\)',
    r'\bTextField\("((?:[^"\\]|\\.)*)",',
    r'\bMenu\("((?:[^"\\]|\\.)*)"\)',
    r'\bDisclosureGroup\("((?:[^"\\]|\\.)*)"\)',
    r'\boption\("((?:[^"\\]|\\.)*)",',
    r'\bPenCheck\(title: "((?:[^"\\]|\\.)*)",',
    r'\blightRow\("((?:[^"\\]|\\.)*)",',
    r'\bPanelCard\("((?:[^"\\]|\\.)*)"\)',
    r'\.navigationTitle\("((?:[^"\\]|\\.)*)"\)',
    r'\.accessibilityLabel\("((?:[^"\\]|\\.)*)"\)',
    r'\.alert\("((?:[^"\\]|\\.)*)"',
    r'\.confirmationDialog\("((?:[^"\\]|\\.)*)"',
    r'\bLabeledContent\("((?:[^"\\]|\\.)*)"',
]

# Symbols, numbers and brand names read the same in every language.
PLAIN = re.compile(r"^$|^[^A-Za-z]+$|^[A-Z]$|^DrawKit$|^MAP$|^OVERVIEW$")


def ignored(key):
    """Symbols and brand names read the same everywhere; interpolated keys carry a format."""
    return bool(PLAIN.search(key)) or "\\(" in key or "%@" in key


def main():
    rows = json.loads(CATALOG.read_text())["strings"]
    missing = {}
    for path in [
        *(ROOT / "app-macos/Sources/DrawKit").rglob("*.swift"),
        *(ROOT / "app-ios/Sources/DrawKit").rglob("*.swift"),
    ]:
        text = path.read_text()
        for pattern in PATTERNS:
            for match in re.finditer(pattern, text):
                key = match.group(1)
                if key in rows or ignored(key):
                    continue
                missing.setdefault(key, set()).add(path.name)
    if missing:
        for key, files in sorted(missing.items()):
            print(f"missing translation: {key!r} in {', '.join(sorted(files))}",
                  file=sys.stderr)
        return 1
    print(f"{len(rows)} rows, every literal found")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
