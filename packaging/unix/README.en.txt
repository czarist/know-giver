Know Giver — study plans and voice lessons for any exam

INSTALL
The simplest way installs and updates for your user, without administrator rights:
  curl -fsSL https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.sh | bash
macOS: goes to ~/Applications/Know Giver.app. The app is not signed by Apple yet.
Allow the microphone when the system asks.
Linux: goes to ~/.local/opt/KnowGiver, with the know-giver command and a menu
entry. Without the script, extract the package and run ./know-giver in KnowGiver.

GETTING STARTED
The first run asks for your language and region, your name, a teacher, the AI
service with your key, and agreement to the terms. In Settings, download the voice
models; they run on your computer.

UPDATES
When a new version is out, a bar at the top of the app offers it.
Settings > About and updates turns the check off.

YOUR DATA
macOS: ~/Library/Application Support/KnowGiver. Linux: ~/.local/share/KnowGiver.
KG_DATA_DIR picks another folder. Never share a .env file.

UNINSTALL
macOS: "~/Applications/Know Giver.app/Contents/Resources/uninstall.sh"
Linux: ~/.local/opt/KnowGiver/uninstall.sh
It asks before deleting your study data.

Code, help and news: https://github.com/czarist/know-giver
