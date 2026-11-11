#!/usr/bin/env bash
# Builds Tarugo.app (universal binary arm64 + x86_64) and the .dmg in target/packages/.
# Usage: packaging/macos/bundle.sh VERSION   (on macOS, after building both targets in release)
set -euo pipefail
version="$1"
cd "$(dirname "$0")/../.."

app=target/macos/Tarugo.app
rm -rf target/macos
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" target/packages

lipo -create -output "$app/Contents/MacOS/tarugo" \
    target/aarch64-apple-darwin/release/tarugo \
    target/x86_64-apple-darwin/release/tarugo
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"

iconset=target/macos/tarugo.iconset
mkdir -p "$iconset"
for s in 16 32 128 256 512; do
    cp "assets/tarugo-$s.png" "$iconset/icon_${s}x${s}.png"
done
cp assets/tarugo-32.png "$iconset/icon_16x16@2x.png"
cp assets/tarugo-64.png "$iconset/icon_32x32@2x.png"
cp assets/tarugo-256.png "$iconset/icon_128x128@2x.png"
cp assets/tarugo-512.png "$iconset/icon_256x256@2x.png"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/tarugo.icns"

# No developer account: ad hoc signature (on Apple Silicon every binary must be signed).
codesign --force --deep --sign - "$app"

dmg=target/macos/dmg
mkdir -p "$dmg"
cp -R "$app" "$dmg/"
ln -s /Applications "$dmg/Applications"
hdiutil create -volname "Tarugo $version" -srcfolder "$dmg" -ov -format UDZO \
    "target/packages/Tarugo-$version-macos-universal.dmg"
