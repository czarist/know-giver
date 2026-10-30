#!/usr/bin/env bash
# Native Linux tarball or macOS app bundle. Run on the target OS.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
platform="$(uname -s)"
architecture="$(uname -m)"
case "$platform" in
  Linux) package_os=linux ;;
  Darwin) package_os=macos ;;
  *) printf 'Unsupported native packaging platform: %s\n' "$platform" >&2; exit 1 ;;
esac
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)"
test -n "$version"
cargo build --locked --release -p know-giver
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
test -f "$target_dir/release/know-giver"
mkdir -p "$repo_root/dist"
stage="$(mktemp -d "$repo_root/dist/.package.XXXXXX")"
trap 'rm -rf -- "$stage"' EXIT

if [[ "$platform" == Darwin ]]; then
  bundle="$stage/Know Giver.app/Contents"
  mkdir -p "$bundle/MacOS" "$bundle/Resources"
  cp "$target_dir/release/know-giver" "$bundle/MacOS/"
  cp .env.example "$bundle/MacOS/"
  cp scripts/uninstall.sh "$bundle/Resources/uninstall.sh"
  chmod +x "$bundle/Resources/uninstall.sh"
  cat > "$bundle/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.knowgiver.desktop</string>
<key>CFBundleName</key><string>Know Giver</string>
<key>CFBundleExecutable</key><string>know-giver</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>NSMicrophoneUsageDescription</key><string>O microfone permite conversar com o tutor durante as aulas.</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
EOF
  plutil -lint "$bundle/Info.plist"
  cp packaging/unix/README.*.txt "$stage/"
else
  mkdir -p "$stage/KnowGiver"
  cp "$target_dir/release/know-giver" .env.example packaging/unix/README.*.txt scripts/uninstall.sh "$stage/KnowGiver/"
  chmod +x "$stage/KnowGiver/uninstall.sh"
  cp crates/ui/assets/icon.png "$stage/KnowGiver/know-giver.png"
  cat > "$stage/KnowGiver/know-giver.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Know Giver
Comment=Planos de estudo e aulas por voz para exames
Exec=know-giver
Icon=know-giver
Categories=Education;
Terminal=false
EOF
fi
archive="$repo_root/dist/KnowGiver-$version-$package_os-$architecture.tar.gz"
tar -czf "$archive" -C "$stage" .
printf 'Package: %s\n' "$archive"
