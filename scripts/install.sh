#!/usr/bin/env bash
# Installs or updates Know Giver for the current user (no administrator
# rights), from the latest GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.sh | bash
#
# macOS (Apple silicon): "Know Giver.app" in ~/Applications.
# Linux (x86_64): the app in ~/.local/opt/KnowGiver, the command in
# ~/.local/bin and an entry in the applications menu.
# A first install asks about a desktop shortcut, login startup, and study
# reminders. Startup and reminders are off unless selected. The program
# folder gets uninstall.sh, which removes the app and asks before deleting
# study data.
#
# Your study data is not touched. The app itself runs this script to update,
# with KG_ARCHIVE (a package it already downloaded and checked) and
# KG_LAUNCH=1 (open the new version afterwards; no questions asked).
# KG_DESKTOP_SHORTCUT=1 or 0 answers the shortcut question in advance.
# KG_STARTUP=1 or 0 and KG_STUDY_REMINDERS=1 or 0 answer their first-install
# questions in advance.
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

# Yes or no from the terminal, also when this script comes through a pipe.
# No answer (no terminal, or an update run by the app) means no.
ask() {
  local answer=""
  if [[ "${KG_LAUNCH:-}" != 1 ]] && { exec 3</dev/tty; } 2>/dev/null; then
    read -r -p "$1 [y/N] " answer <&3 || true
    exec 3<&-
  fi
  [[ "$answer" =~ ^[yYsS] ]]
}

# Whether to add a desktop shortcut: asked once, on the first install.
want_shortcut() {
  case "${KG_DESKTOP_SHORTCUT:-}" in
    1) return 0 ;;
    0) return 1 ;;
  esac
  ask "Create a desktop shortcut?"
}

want_startup() {
  case "${KG_STARTUP:-}" in
    1) return 0 ;;
    0) return 1 ;;
  esac
  ask "Start Know Giver automatically when you sign in?"
}

want_study_reminders() {
  case "${KG_STUDY_REMINDERS:-}" in
    1) return 0 ;;
    0) return 1 ;;
  esac
  ask "Receive study reminders every 3 hours while Know Giver is running, including in the tray?"
}

xml_escape() {
  printf '%s' "$1" | sed \
    -e 's/&/\&amp;/g' \
    -e 's/</\&lt;/g' \
    -e 's/>/\&gt;/g' \
    -e 's/"/\&quot;/g' \
    -e "s/'/\&apos;/g"
}

desktop_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//%/%%}"
  printf '%s' "$value"
}

collect_install_choices() {
  startup_choice=0
  reminder_choice=0
  if [[ "$first_install" == 1 ]]; then
    if want_shortcut; then shortcut_choice=1; else shortcut_choice=0; fi
    if want_startup; then startup_choice=1; fi
    if want_study_reminders; then reminder_choice=1; fi
  else
    shortcut_choice=0
  fi
}

tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT

archive="${KG_ARCHIVE:-}"
if [[ -z "$archive" ]]; then
  printf 'Downloading %s\n' "$asset"
  if ! curl -fL --progress-bar -o "$tmp/$asset" "$base/$asset" ||
     ! curl -fsSL -o "$tmp/SHA256SUMS.txt" "$base/SHA256SUMS.txt"; then
    printf 'No Know Giver release with %s is published yet. See https://github.com/%s/releases\n' \
      "$asset" "$repo" >&2
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
  app="$apps/Know Giver.app"
  first_install=1
  [[ -d "$app" ]] && first_install=0
  collect_install_choices
  mkdir -p "$apps"
  rm -rf -- "$app.new"
  mv "$tmp/x/Know Giver.app" "$app.new"
  rm -rf -- "$app"
  mv "$app.new" "$app"
  if [[ $shortcut_choice == 1 ]]; then
    ln -sfn "$app" "$HOME/Desktop/Know Giver"
  fi
  if [[ $reminder_choice == 1 ]]; then
    touch "$app/Contents/MacOS/.study-reminders-enabled"
  fi
  if [[ $startup_choice == 1 ]]; then
    launch_agents="$HOME/Library/LaunchAgents"
    mkdir -p "$launch_agents"
    executable="$(xml_escape "$app/Contents/MacOS/know-giver")"
    cat > "$launch_agents/com.knowgiver.desktop.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>com.knowgiver.desktop</string>
<key>ProgramArguments</key><array><string>$executable</string><string>--background</string></array>
<key>RunAtLoad</key><true/>
</dict></plist>
EOF
    if ! launchctl bootstrap "gui/$(id -u)" "$launch_agents/com.knowgiver.desktop.plist"; then
      printf 'The login item was saved, but could not be started in this session. It will run at the next login.\n' >&2
    fi
  fi
  printf 'Installed: %s\n' "$app"
  printf 'To uninstall: "%s/Contents/Resources/uninstall.sh"\n' "$app"
  if [[ "${KG_LAUNCH:-}" == 1 ]]; then
    open -n "$app"
  fi
else
  dest="$HOME/.local/opt/KnowGiver"
  bin="$HOME/.local/bin"
  menu="$HOME/.local/share/applications"
  icons="$HOME/.local/share/icons/hicolor/256x256/apps"
  first_install=1
  [[ -x "$dest/know-giver" ]] && first_install=0
  collect_install_choices
  mkdir -p "$dest" "$bin" "$menu" "$icons"
  # Replaced by renaming, so a running copy keeps working until it closes.
  cp "$tmp/x/KnowGiver/know-giver" "$dest/know-giver.new"
  chmod +x "$dest/know-giver.new"
  mv -f "$dest/know-giver.new" "$dest/know-giver"
  # Read-me files in each app language (one README.txt before v0.33.1).
  rm -f -- "$dest"/README*.txt
  cp "$tmp/x/KnowGiver/"README*.txt "$tmp/x/KnowGiver/.env.example" "$dest/"
  if [[ -f "$tmp/x/KnowGiver/uninstall.sh" ]]; then
    cp "$tmp/x/KnowGiver/uninstall.sh" "$dest/uninstall.sh"
    chmod +x "$dest/uninstall.sh"
  fi
  cp "$tmp/x/KnowGiver/know-giver.png" "$icons/know-giver.png"
  ln -sf "$dest/know-giver" "$bin/know-giver"
  entry="[Desktop Entry]
Type=Application
Name=Know Giver
Comment=Study plans and voice lessons for exams
Exec=$dest/know-giver
Icon=know-giver
Categories=Education;
Terminal=false"
  printf '%s\n' "$entry" > "$menu/know-giver.desktop"
  if [[ $shortcut_choice == 1 ]]; then
    desktop="$(xdg-user-dir DESKTOP 2>/dev/null || printf '%s' "$HOME/Desktop")"
    mkdir -p "$desktop"
    printf '%s\n' "$entry" > "$desktop/know-giver.desktop"
    chmod +x "$desktop/know-giver.desktop"
    # GNOME asks before opening an untrusted launcher.
    gio set "$desktop/know-giver.desktop" metadata::trusted true 2>/dev/null || true
  fi
  if [[ $reminder_choice == 1 ]]; then
    touch "$dest/.study-reminders-enabled"
  fi
  if [[ $startup_choice == 1 ]]; then
    autostart="$HOME/.config/autostart"
    mkdir -p "$autostart"
    executable="$(desktop_escape "$dest/know-giver")"
    cat > "$autostart/know-giver.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Know Giver
Comment=Study plans and voice lessons for exams
Exec="$executable" --background
Icon=know-giver
Categories=Education;
Terminal=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
EOF
  fi
  printf 'Installed: %s (command: know-giver)\n' "$dest"
  printf 'To uninstall: %s/uninstall.sh\n' "$dest"
  case ":$PATH:" in
    *":$bin:"*) ;;
    *) printf 'Add %s to your PATH to run know-giver from a terminal.\n' "$bin" ;;
  esac
  if [[ "${KG_LAUNCH:-}" == 1 ]]; then
    nohup "$dest/know-giver" >/dev/null 2>&1 &
  fi
fi
