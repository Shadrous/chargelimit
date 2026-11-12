#!/bin/sh
# Builds target/release/chargelimit and dist/chargelimit-<version>.pkg
set -e
cd "$(dirname "$0")"
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

cargo build --release
strip target/release/chargelimit

rm -rf build dist && mkdir -p build/root/usr/local/bin build/root/Library/LaunchDaemons dist
cp target/release/chargelimit build/root/usr/local/bin/
cp packaging/local.chargelimit.plist build/root/Library/LaunchDaemons/
chmod +x packaging/scripts/postinstall
xattr -cr build/root packaging/scripts

COPYFILE_DISABLE=1 pkgbuild --root build/root --scripts packaging/scripts \
  --identifier local.chargelimit --version "$VERSION" \
  --install-location / "dist/chargelimit-$VERSION.pkg"
cp packaging/uninstall.sh dist/
echo "built dist/chargelimit-$VERSION.pkg"
