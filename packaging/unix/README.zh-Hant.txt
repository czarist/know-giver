Know Giver — 準備任何考試的學習計畫與語音課堂

安裝
最簡單的方式，為目前的使用者安裝與更新，不需要系統管理員權限：
  curl -fsSL https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.sh | bash
macOS：安裝到 ~/Applications/Know Giver.app。程式尚未經過 Apple 簽署。系統要求時請允許使用麥克風。
Linux：安裝到 ~/.local/opt/KnowGiver，並提供 know-giver 指令與選單項目。
不用指令碼時，解壓縮安裝包，在 KnowGiver 資料夾中執行 ./know-giver。

開始使用
首次設定會詢問語言與地區、你的名字、老師、AI 服務與金鑰，以及是否同意條款。
在「設定」中下載語音模型，它們在你的電腦上執行。中文課堂目前以文字進行。

更新
有新版本時，程式頂端會出現提示列。可在 設定 > 關於與更新 中關閉檢查。

你的資料
macOS：~/Library/Application Support/KnowGiver。Linux：~/.local/share/KnowGiver。
KG_DATA_DIR 可指定其他資料夾。請勿分享 .env 檔案。

解除安裝
macOS："~/Applications/Know Giver.app/Contents/Resources/uninstall.sh"
Linux：~/.local/opt/KnowGiver/uninstall.sh
刪除學習資料前會先詢問。

程式碼、說明與更新內容：https://github.com/czarist/know-giver
