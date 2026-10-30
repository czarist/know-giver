#!/usr/bin/env bash
# Installs or updates Know Giver for the current user (no administrator
# rights), from the latest GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.sh | bash
#
# macOS (Apple silicon): "Know Giver.app" in ~/Applications.
# Linux (x86_64): the app in ~/.local/opt/KnowGiver, the command in
# ~/.local/bin and an entry in the applications menu.
#
# Your study data is not touched. The app itself runs this script to update,
# with KG_ARCHIVE (a package it already downloaded and checked) and
# KG_LAUNCH=1 (open the new version afterwards).
set -euo pipefail

repo="czarist/know-giver"
base="https://github.com/$repo/releases/latest/download"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) asset="KnowGiver-linux-x86_64.tar.gz" ;;
  Darwin-arm64) asset="KnowGiver-macos-arm64.tar.gz" ;;
  *)
    printf 'No Know Giver package for %s %s yet. Build it from source: https://github.com/%s\n' \
      "$(uname -s)" "$(uname -m)" "$repo" >&2
    exit 1
    ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT

archive="${KG_ARCHIVE:-}"
if [[ -z "$archive" ]]; then
  printf 'Downloading %s\n' "$asset"
  if ! curl -fL --progress-bar -o "$tmp/$asset" "$base/$asset" ||
     ! curl -fsSL -o "$tmp/SHA256SUMS.txt" "$base/SHA256SUMS.txt"; then
    printf 'No Know Giver release with %s is published yet. See https://github.com/%s/releases
'       "$asset" "$repo" >&2
    exit 1
  fi
  expected="$(awk -v f="$asset" '$2 == f || $2 == "*" f { print tolower($1) }' "$tmp/SHA256SUMS.txt")"
  if command -v sha256sum >/dev/null; then
    actual="$(sha256sum "$tmp/$asset" | awk '{ print $1 }')"
  else
    actual="$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')"
  fi
  if [[ -z "$expected" || "$expected" != "$actual" ]]; then
    printf 'Checksum mismatch for %s; nothing was installed.\n' "$asset" >&2
    exit 1
  fi
  archive="$tmp/$asset"
fi

mkdir -p "$tmp/x"
tar -xzf "$archive" -C "$tmp/x"

if [[ "$(uname -s)" == Darwin ]]; then
  apps="$HOME/Applications"
  mkdir -p "$apps"
  rm -rf -- "$apps/Know Giver.app.new"
  mv "$tmp/x/Know Giver.app" "$apps/Know Giver.app.new"
  rm -rf -- "$apps/Know Giver.app"
  mv "$apps/Know Giver.app.new" "$apps/Know Giver.app"
  printf 'Installed: %s\n' "$apps/Know Giver.app"
  if [[ "${KG_LAUNCH:-}" == 1 ]]; then
    open -n "$apps/Know Giver.app"
  fi
else
  dest="$HOME/.local/opt/KnowGiver"
  bin="$HOME/.local/bin"
  menu="$HOME/.local/share/applications"
  icons="$HOME/.local/share/icons/hicolor/256x256/apps"
  mkdir -p "$dest" "$bin" "$menu" "$icons"
  # Replaced by renaming, so a running copy keeps working until it closes.
  cp "$tmp/x/KnowGiver/know-giver" "$dest/know-giver.new"
  chmod +x "$dest/know-giver.new"
  mv -f "$dest/know-giver.new" "$dest/know-giver"
  cp "$tmp/x/KnowGiver/README.txt" "$tmp/x/KnowGiver/.env.example" "$dest/"
  cp "$tmp/x/KnowGiver/know-giver.png" "$icons/know-giver.png"
  ln -sf "$dest/know-giver" "$bin/know-giver"
  cat > "$menu/know-giver.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Know Giver
Comment=Study plans and voice lessons for exams
Exec=$dest/know-giver
Icon=know-giver
Categories=Education;
Terminal=false
EOF
  printf 'Installed: %s (command: know-giver)\n' "$dest"
  case ":$PATH:" in
    *":$bin:"*) ;;
    *) printf 'Add %s to your PATH to run know-giver from a terminal.\n' "$bin" ;;
  esac
  if [[ "${KG_LAUNCH:-}" == 1 ]]; then
    nohup "$dest/know-giver" >/dev/null 2>&1 &
  fi
fi
