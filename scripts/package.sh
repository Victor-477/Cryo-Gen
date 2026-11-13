#!/usr/bin/env bash
# Packages the release build of the editor into dist/CryoEngine-<tag>-<platform>.zip.
#
#   bash scripts/package.sh v0.11.0-beta macos-universal
#
# Expects `cargo build --release` to have been run already. On macOS the result is an .app bundle.
set -euo pipefail

TAG="${1:?usage: package.sh <tag> <platform-name>}"
NAME="${2:?usage: package.sh <tag> <platform-name>}"
VERSION="${TAG#v}"
cd "$(dirname "$0")/.."

OUT="dist/CryoEngine-$TAG-$NAME"
rm -rf "$OUT" "$OUT.zip"
mkdir -p "$OUT"

cat > "$OUT/README.txt" <<EOF
Cryo Engine $VERSION

Run the editor:
  Windows  double-click cryo.exe
  macOS    right-click "Cryo Engine.app" and choose Open (the app is not signed)
  Linux    ./cryo

On first launch the editor creates the example game in
~/CryoProjects/FrostpeakIsles and starts a short welcome tour.

Documentation: https://victor-477.github.io/Cryo-Gen/
Source code:   https://github.com/Victor-477/Cryo-Gen
EOF
[ -f LICENSE ] && cp LICENSE "$OUT/LICENSE.txt"

case "$NAME" in
  macos-*)
    APP="$OUT/Cryo Engine.app/Contents"
    mkdir -p "$APP/MacOS"
    cp target/release/cryo "$APP/MacOS/cryo"
    chmod +x "$APP/MacOS/cryo"
    cat > "$APP/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Cryo Engine</string>
  <key>CFBundleDisplayName</key><string>Cryo Engine</string>
  <key>CFBundleExecutable</key><string>cryo</string>
  <key>CFBundleIdentifier</key><string>io.github.victor-477.cryo-engine</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
EOF
    # ad-hoc signature so Apple Silicon agrees to run the merged binary
    codesign --force --deep --sign - "$OUT/Cryo Engine.app" 2>/dev/null || true
    (cd dist && ditto -c -k --keepParent "$(basename "$OUT")" "$(basename "$OUT").zip")
    ;;
  windows-*)
    cp target/release/cryo.exe "$OUT/cryo.exe"
    (cd dist && 7z a -tzip "$(basename "$OUT").zip" "$(basename "$OUT")" > /dev/null)
    ;;
  *)
    cp target/release/cryo "$OUT/cryo"
    chmod +x "$OUT/cryo"
    (cd dist && zip -qr "$(basename "$OUT").zip" "$(basename "$OUT")")
    ;;
esac

echo "Created $OUT.zip"
