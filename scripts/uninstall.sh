#!/usr/bin/env bash
# Removes Know Giver as installed by install.sh: the app, its command, menu
# entry, icon and desktop shortcut. Study data (plans, progress, speech and
# voice models) is kept unless you say otherwise.
#
# macOS: "~/Applications/Know Giver.app/Contents/Resources/uninstall.sh"
# Linux: ~/.local/opt/KnowGiver/uninstall.sh
set -euo pipefail

# Yes or no from the terminal; no answer means no.
ask() {
  local answer=""
  if { exec 3</dev/tty; } 2>/dev/null; then
    read -r -p "$1 [y/N] " answer <&3 || true
    exec 3<&-
  fi
  [[ "$answer" =~ ^[yYsS] ]]
}

if [[ "$(uname -s)" == Darwin ]]; then
  rm -rf -- "$HOME/Applications/Know Giver.app"
  if [[ -L "$HOME/Desktop/Know Giver" ]]; then
    rm -f -- "$HOME/Desktop/Know Giver"
  fi
  data="$HOME/Library/Application Support/KnowGiver"
else
  rm -rf -- "$HOME/.local/opt/KnowGiver"
  rm -f -- "$HOME/.local/bin/know-giver" \
    "$HOME/.local/share/applications/know-giver.desktop" \
    "$HOME/.local/share/icons/hicolor/256x256/apps/know-giver.png"
  desktop="$(xdg-user-dir DESKTOP 2>/dev/null || printf '%s' "$HOME/Desktop")"
  rm -f -- "$desktop/know-giver.desktop"
  data="${XDG_DATA_HOME:-$HOME/.local/share}/KnowGiver"
fi
printf 'Know Giver was removed.\n'

if [[ -d "$data" ]]; then
  if ask "Also delete your study data (plans, progress, voice and speech models) in $data?"; then
    rm -rf -- "$data"
    printf 'Study data deleted.\n'
  else
    printf 'Your study data stays in %s.\n' "$data"
  fi
fi
