#!/bin/sh
# Builds the dmg, uploads it as a GitHub release on the tap repo, and bumps the cask
# (version + sha256) in the tap checkout. Does not commit/push the tap: review, then push.
# Env: TAP_DIR (default ~/dev/homebrew-tap), TAP_REPO (default AnisAbdellatif/homebrew-tap).
set -eu
cd "$(dirname "$0")/.."

TAP_DIR=${TAP_DIR:-$HOME/dev/homebrew-tap}
TAP_REPO=${TAP_REPO:-AnisAbdellatif/homebrew-tap}
CASK=$TAP_DIR/Casks/cometdesk.rb

scripts/package-dmg.sh
DMG=$(ls -t dist/CometDesk-*-arm64.dmg | head -1)
VERSION=$(basename "$DMG" | sed 's/^CometDesk-\(.*\)-arm64\.dmg$/\1/')
SHA=$(shasum -a 256 "$DMG" | cut -d' ' -f1)
TAG=cometdesk-v$VERSION

gh release create "$TAG" "$DMG" --repo "$TAP_REPO" --title "CometDesk $VERSION" \
	--notes "CometDesk $VERSION (arm64, ad-hoc signed)" ||
	gh release upload "$TAG" "$DMG" --repo "$TAP_REPO" --clobber

sed -i '' -e "s/^  version \".*\"/  version \"$VERSION\"/" -e "s/^  sha256 \".*\"/  sha256 \"$SHA\"/" "$CASK"
echo "cask bumped to $VERSION ($SHA); commit and push $TAP_DIR"
