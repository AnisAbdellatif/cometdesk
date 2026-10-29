#!/bin/sh
# Builds the cometdesk-agent binaries the console uploads to the Comet, into dist/agents/
# (named as the target installers expect: linux-x86_64, linux-aarch64, windows-x86_64.exe).
#
# Linux builds are static musl binaries linked with rust-lld, so no C cross-toolchain is
# needed. Windows needs its import libraries: built when a linker for it is available,
# otherwise skipped with a note.
set -eu
cd "$(dirname "$0")/.."
OUT=dist/agents
mkdir -p "$OUT"
export CARGO_PROFILE_RELEASE_STRIP=symbols

build_linux() { # <rust target> <name>
	rustup target add "$1" >/dev/null
	env "CARGO_TARGET_$(echo "$1" | tr 'a-z-' 'A-Z_')_LINKER=rust-lld" \
		cargo build --release -q -p cometdesk-agent --target "$1"
	cp "target/$1/release/cometdesk-agent" "$OUT/$2"
	echo "built $OUT/$2"
}

build_linux x86_64-unknown-linux-musl linux-x86_64
build_linux aarch64-unknown-linux-musl linux-aarch64

if rustup target list --installed | grep -q x86_64-pc-windows-msvc &&
	cargo build --release -q -p cometdesk-agent --target x86_64-pc-windows-msvc 2>/dev/null; then
	cp target/x86_64-pc-windows-msvc/release/cometdesk-agent.exe "$OUT/windows-x86_64.exe"
	echo "built $OUT/windows-x86_64.exe"
else
	echo "skipped windows-x86_64.exe (build it on Windows: cargo build --release -p cometdesk-agent)"
fi
