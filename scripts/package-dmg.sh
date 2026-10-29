#!/bin/sh
# Packages the console as dist/GLKVM-<version>-<arch>.dmg: a GLKVM.app bundle (with the
# agent programs from dist/agents in Resources, built by scripts/build-agents.sh if missing)
# ad-hoc signed, inside a disk image with an Applications shortcut.
# Set CODESIGN_IDENTITY to a Developer ID to sign for distribution instead of ad-hoc.
set -eu
cd "$(dirname "$0")/.."

NAME=GLKVM
ID=rs.glkvm.console
VERSION=$(cargo metadata --no-deps --format-version 1 |
	python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="glkvm-gui"))')
ARCH=$(uname -m)
OUT=dist/$NAME-$VERSION-$ARCH.dmg
STAGE=target/dmg
APP=$STAGE/dmg-root/$NAME.app

ls dist/agents/* >/dev/null 2>&1 || scripts/build-agents.sh
cargo build --release -q -p glkvm-gui

rm -rf "$STAGE"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/glkvm-gui "$APP/Contents/MacOS/$NAME"
cp -R dist/agents "$APP/Contents/Resources/agents"
[ -f apps/glkvm-gui/AppIcon.icns ] && cp apps/glkvm-gui/AppIcon.icns "$APP/Contents/Resources/"

cat >"$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
	<key>CFBundleName</key><string>$NAME</string>
	<key>CFBundleDisplayName</key><string>$NAME</string>
	<key>CFBundleIdentifier</key><string>$ID</string>
	<key>CFBundleExecutable</key><string>$NAME</string>
	<key>CFBundleIconFile</key><string>AppIcon</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>$VERSION</string>
	<key>CFBundleVersion</key><string>$VERSION</string>
	<key>LSMinimumSystemVersion</key><string>12.0</string>
	<key>NSHighResolutionCapable</key><true/>
	<key>NSLocalNetworkUsageDescription</key><string>Connects to your GL.iNet Comet KVM on the local network.</string>
</dict></plist>
PLIST

IDENTITY=${CODESIGN_IDENTITY:--}
if [ "$IDENTITY" = "-" ]; then
	# Ad-hoc (local dev): one-shot deep sign, no hardened runtime. Unchanged.
	codesign --force --deep --sign - "$APP"
else
	# Distribution: hardened runtime + a secure timestamp, signing nested Mach-O code
	# inside-out first and then the app bundle. No --deep (Apple deprecates it and it
	# skips the per-item options). This makes the bundle notarization-ready; notarization
	# itself (notarytool submit + stapler) must run elsewhere -- not done here.
	# (No entitlements in the repo, so none are passed; add --entitlements here if that
	# changes.)
	find "$APP" -type f | while IFS= read -r f; do
		case "$(file -b "$f")" in
		*Mach-O*) codesign --force --options runtime --timestamp --sign "$IDENTITY" "$f" ;;
		esac
	done
	codesign --force --options runtime --timestamp --sign "$IDENTITY" "$APP"
fi
ln -s /Applications "$STAGE/dmg-root/Applications"
rm -f "$OUT"
hdiutil create -quiet -volname "$NAME" -srcfolder "$STAGE/dmg-root" -format UDZO -ov "$OUT"
echo "built $OUT"
