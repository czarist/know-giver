<p align="center">
  <img src="crates/ui/assets/icon.png" width="128" height="128" alt="Know Giver 圖示：新月下的狒狒形象托特神版畫，翡翠色方塊">
</p>

<h1 align="center">Know Giver</h1>

<h3 align="center">你的考試。你的計劃。一位會和你對話的老師。</h3>

<p align="center">
  <a href="README.md">English</a> · <a href="README.pt-BR.md">Português (Brasil)</a> · <a href="README.ja.md">日本語</a> · <a href="README.zh-Hans.md">简体中文</a> · <a href="README.zh-Hant.md">繁體中文</a>
</p>

<p align="center">
  <b>為考試製定學習計劃的桌面程式，完全用 Rust 編寫。</b><br>
  放入考試公告或大綱的 PDF，得到按周安排的計劃，並和一位會傾聽、講解、出題的老師一起學習。
</p>

<p align="center">
  <a href="https://www.rust-lang.org"><img alt="Pure Rust, 2024 edition" src="https://img.shields.io/badge/pure%20Rust-2024-3f8f7c?style=flat-square&logo=rust&logoColor=white"></a>
  <a href="https://github.com/emilk/egui"><img alt="UI: egui" src="https://img.shields.io/badge/UI-egui%200.36-2c6b5c?style=flat-square"></a>
  <a href="https://www.sqlite.org"><img alt="Storage: SQLite" src="https://img.shields.io/badge/storage-SQLite-2c6b5c?style=flat-square&logo=sqlite&logoColor=white"></a>
  <a href="https://typst.app"><img alt="Handouts: Typst" src="https://img.shields.io/badge/handouts-Typst-2c6b5c?style=flat-square&logo=typst&logoColor=white"></a>
  <img alt="Speech to text: Whisper and Parakeet" src="https://img.shields.io/badge/speech-Whisper%20%C2%B7%20Parakeet-2c6b5c?style=flat-square&logo=huggingface&logoColor=white">
  <a href="https://github.com/rhasspy/piper"><img alt="Voices: Piper and Kokoro" src="https://img.shields.io/badge/voices-Piper%20%C2%B7%20Kokoro-2c6b5c?style=flat-square"></a>
  <img alt="AI: OpenAI-compatible APIs and GitHub Copilot" src="https://img.shields.io/badge/AI-OpenAI--compatible-2c6b5c?style=flat-square&logo=ollama&logoColor=white">
  <img alt="Sign in: ChatGPT, Claude Code, GitHub Copilot" src="https://img.shields.io/badge/Sign%20in-ChatGPT%20%C2%B7%20Claude%20Code%20%C2%B7%20Copilot-2c6b5c?style=flat-square&logo=openid&logoColor=white">
  <a href="https://github.com/ggml-org/llama.cpp"><img alt="OCR runtime: llama.cpp" src="https://img.shields.io/badge/OCR-llama.cpp-2c6b5c?style=flat-square"></a>
  <a href="https://www.vulkan.org/"><img alt="GPU acceleration: Vulkan" src="https://img.shields.io/badge/GPU-Vulkan-2c6b5c?style=flat-square&logo=vulkan&logoColor=white"></a>
  <img alt="Windows" src="https://img.shields.io/badge/-Windows-2c6b5c?style=flat-square&logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCIgZmlsbD0id2hpdGUiPjxwYXRoIGQ9Ik0wIDBoMTEuNHYxMS40SDB6TTEyLjYgMEgyNHYxMS40SDEyLjZ6TTAgMTIuNmgxMS40VjI0SDB6TTEyLjYgMTIuNkgyNFYyNEgxMi42eiIvPjwvc3ZnPg%3D%3D&logoColor=white&labelColor=111111">
  <img alt="macOS" src="https://img.shields.io/badge/-macOS-2c6b5c?style=flat-square&logo=apple&logoColor=white&labelColor=111111">
  <img alt="Linux" src="https://img.shields.io/badge/-Linux-2c6b5c?style=flat-square&logo=linux&logoColor=white&labelColor=111111">
  <a href="#license"><img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-2c6b5c?style=flat-square&logo=opensourceinitiative&logoColor=white"></a>
  <a href="https://github.com/czarist/know-giver/tags"><img alt="Latest version" src="https://img.shields.io/github/v/tag/czarist/know-giver?sort=semver&label=version&color=2c6b5c&style=flat-square&logo=github&logoColor=white"></a>
  <img alt="Languages: Portuguese, English, Japanese, Simplified and Traditional Chinese" src="https://img.shields.io/badge/languages-pt--BR%20%C2%B7%20en%20%C2%B7%20ja%20%C2%B7%20zh--Hans%20%C2%B7%20zh--Hant-2c6b5c?style=flat-square">
  <img alt="Local-first voice" src="https://img.shields.io/badge/voice-local--first-2c6b5c?style=flat-square">
  <a href="https://github.com/czarist/know-giver/actions/workflows/packages.yml"><img alt="Native packages build" src="https://img.shields.io/github/actions/workflow/status/czarist/know-giver/packages.yml?label=packages&style=flat-square&logo=githubactions&logoColor=white"></a>
</p>

<br>

<p align="center">
  <img src="docs/images/zh-Hant/lesson.png" alt="課堂畫面：老師的肖像和字幕、寫有要點的板書、學生的麥克風" width="100%">
</p>

<p align="center">
  <a href="#從考試公告到學習計劃">公告 → 計劃</a> ·
  <a href="#課堂">課堂</a> ·
  <a href="#板書">板書</a> ·
  <a href="#閱讀並標記講義">講義</a> ·
  <a href="#練習複習講義">練習</a> ·
  <a href="#主觀題與作文">主觀題與作文</a> ·
  <a href="#首次設定">首次設定</a> ·
  <a href="#隱私">隱私</a> ·
  <a href="#開始使用">開始使用</a>
</p>

<br>

## 從考試公告到學習計劃

考試公告在你的電腦上讀取，只有考試大綱會傳送給 AI。

- **選擇職位。** 有些考試招考多個職位。Know Giver 會找出這些職位，問你報考哪一個，計劃只包含公共科目和*你的*職位的專業科目。
- **按你的時間安排。** 根據距離考試的週數和每週學習時長，按周、科目和知識點制定計劃。可以同時管理多個考試。
- **掃描版 PDF。** 沒有文字的頁面可以由你設定的圖片模型轉寫。

<p align="center">
  <img src="docs/images/zh-Hant/edital-roles.png" alt="從包含多個職位的考試公告中選擇職位" width="80%">
</p>

## 課堂

老師按照課程安排一步步講解知識點：每一步講到講義的要點，確認你理解後再繼續，並覆蓋測驗會考的所有內容。最後有總結和測驗提示。可選擇「快速」「標準」「完整」三種長度。

**中文課堂目前透過文字進行。** 語音（本地語音識別和朗讀）目前支援葡萄牙語和英語，中文語音將在之後支援。也可以在設定中使用雲端語音服務。

AI 可以使用任何相容 OpenAI 的模型（Z.ai、DeepSeek、Qwen、Kimi、Anthropic、OpenAI、Gemini、Ollama 等），給老師的指令（提示詞）會跟隨程式語言使用中文。

## 板書

除了要點，老師還會寫**公式**、**流程圖**、**對比表**、**時間線**、**思維導圖**、**考試提示**和**例子**，並展示 Wikipedia 的**圖片**（只有搜尋詞會離開你的電腦）。

<p align="center">
  <img src="docs/images/zh-Hant/whiteboard.png" alt="板書專案：公式、流程圖、表格、時間線、思維導圖和考試提示" width="80%">
</p>

四位老師：**何靜**、**李悅**、**任浩**、**歐陽文**。會呼吸、眨眼、點頭的水墨肖像。

<p align="center">
  <img src="docs/images/teachers.png" alt="四位老師的狀態：休息、傾聽、思考、講話" width="70%">
</p>

## 閱讀並標記講義

生成的講義在程式內開啟，頁面與 PDF 完全相同。拖動文字即可標記，點選標記可寫筆記，側邊列表可跳到每處標記。程式會記住讀到哪裡，也可以儲存帶標記的 PDF。

<p align="center">
  <img src="docs/images/zh-Hant/reader.png" alt="閱讀講義：標記的段落、筆記和閱讀進度" width="100%">
</p>

## 練習、複習、講義

- **測驗**：考試風格的題目和解析；上傳歷年真題後會有真題。
- **錯題本**：答錯的題會一直保留到答對為止，並出現在下一次測驗的開頭。
- **間隔複習（SM-2）**：知識點會在你快要忘記之前回來。
- **真題與資料**：歷年真題（附答案）和資料會按公告範圍篩選，並說明擱置的原因。
- **講義**：按知識點、科目或整個計劃生成，含圖片、練習和答案的 PDF。
- **進度**：各科目的進展和實際學習時間。

## 模擬考試

依簡章格式的一整場考試，限時作答。**從簡章讀取**會找出每科題數、考試時間、選擇題還是是非題、
答錯是否倒扣、有沒有作文；應用程式會把這些數字與簡章原文核對，每一項你都可以調整。**產生模擬考試**
會依出題機關的風格為每個科目出新題，分布在計畫的各個主題上（你加入的歷屆試題會作為風格參考），
簡章有申論考試時還會附上作文題。

可以**在螢幕上**作答（時鐘在右上角），也可以**列印**：PDF 包含雙欄試題本、有行號的作文紙，以及
四角有定位標記的答案卡。在紙上作答後，把劃記結果填入應用程式的答案卡再交卷。離開畫面或關閉應用程式時
計時不會停，同一時間只能進行一場考試。評分就是依儲存的答案和簡章配分做的簡單計算，依科目顯示
答對、答錯、未答、用時以及是否超時。答錯的題進入錯題本，作文送到「主觀題與作文」拍照評分，
答案與解析在下方展開。還新增了 10 個挑戰。

<table>
  <tr>
    <td><img src="docs/images/zh-Hant/mock.png" alt="模擬考試：從簡章讀取的形式、產生按鈕和以往成績"></td>
    <td><img src="docs/images/zh-Hant/mock-sitting.png" alt="考試進行中：右上角的時鐘、螢幕上的題目和答案卡"></td>
    <td><img src="docs/images/zh-Hant/mock-result.png" alt="成績：各科得分，答對、答錯、未答，以及用時"></td>
  </tr>
</table>

## 挑戰與成就

**成就**頁面共有 36 個系列、**164 個挑戰**，每個等級都能獲得一項原創成就（翡翠色、新月，與應用
程式標誌相同的版畫風格）。每個計劃中：課堂、測驗、答對與滿分、反敗為勝的錯題、已學主題與考試
公告的覆蓋、達標的週、已掌握的主題與科目、複習、歷年真題、講義與標註，以及考試公告包含時的
主觀題。所有計劃合計：學習時長與天數、建立的計劃，以及成就收藏本身。第一步、反敗為勝、計劃啟動和
一週達標保留各自的名字。

每個系列佔一行：下一級、進度（2/3）、「第 3 級，共 11 級」，以及前往課堂、測驗或複習的按鈕。
「今天」頁面低調地顯示下一個挑戰。展示櫃包含所有成就，可按已獲得或未解鎖篩選，滑鼠停留可查看
詳情；等級以邊框顏色和一到三個圓點表示。成就只由程式碼中的規則根據已儲存的學習紀錄發放，絕不由
AI 決定；每個計劃的進度各自獨立。此功能推出前的學習同樣計入；如果歷史紀錄無法確定日期，卡片會
註明。提示只在課堂或測驗結束後出現一次。沒有積分，沒有排行榜，也沒有會中斷的連續紀錄。

<table>
  <tr>
    <td><img src="docs/images/zh-Hant/challenges.png" alt="成就：每個系列一行，顯示等級、進度和前往下一節課堂或測驗的按鈕"></td>
    <td><img src="docs/images/zh-Hant/showcase.png" alt="成就展示櫃：已獲得的為彩色並附日期，未解鎖的為灰色並附進度"></td>
  </tr>
</table>

## 主觀題與作文

考試公告中有作文或主觀題考試時，會出現**主觀題與作文**區域。

- **根據計劃出題**：議論文、專業主觀題或案例分析。動筆前會顯示評分標準和分值（來自公告、你自己設定，或 Know Giver 的通用標準）。
- **手寫作答並上傳照片**：JPEG 或 PNG。太暗、模糊或拍不全的照片會要求重拍。
- **核對讀取結果**：照片和轉寫並排顯示。你的書寫錯誤會保留，不確定的字詞不會扣分。
- **形成性評分**：按標準估計的分數、經過核對的原文引用、優點、問題和建議、三個優先事項。

預設由 PaddleOCR-VL 1.5（0.9B）透過隨應用程式提供的 [🦙 llama.cpp](https://github.com/ggml-org/llama.cpp) `llama-server` v0.8.0 在本機讀取照片；首次需從 Hugging Face 下載約 1.82 GB 的模型。在設定 → AI 中明確選擇外部圖片 API 後，照片才會傳送給該服務商。Copilot 僅接受文字，因此一律使用本機 OCR。可透過**測試圖片讀取**確認所選方式。最低硬體需求和手寫辨識準確率尚未測試；評分前請務必核對轉寫。Windows/Linux 套件包含 🔺 [Vulkan](https://www.vulkan.org/) 執行環境，偵測到相容 GPU 時會優先使用；沒有裝置或 GPU 啟動失敗時退回 CPU。macOS 使用 CPU。按字數要求的中文作文，字數要求會寫在題目說明中。
Windows 安裝程式包含所需的 C++ 執行階段和 Vulkan 載入器；使用 Vulkan 仍需安裝相容的顯示卡驅動程式。

<table>
  <tr>
    <td><img src="docs/images/zh-Hant/writing-check.png" alt="核對轉寫：照片旁邊是可編輯的文字，並列出不確定的詞"></td>
    <td><img src="docs/images/zh-Hant/writing.png" alt="已評分的作文：簡章中的評分標準、每項的估計分數與引文、問題與建議"></td>
  </tr>
</table>

<table>
  <tr>
    <td><img src="docs/images/zh-Hant/today.png" alt="今天：連續天數、待複習與下一步"></td>
    <td><img src="docs/images/zh-Hant/board.png" alt="歷屆試題：與計畫對應的題目，超出範圍的題目另外列出"></td>
  </tr>
</table>

## 首次設定

首先有五個簡短步驟：語言、名字、老師、AI 服務及 API 金鑰或帳戶登入、國家或地區及同意條款。可以從 22 個 AI 服務商中選擇（**Z.ai** 價效比高，**Claude** 準確度高），並附有建立金鑰的連結。之後有一段簡短的介面導覽。

GitHub Copilot 可透過官方 Copilot CLI 使用 GitHub 帳戶登入，不需要 API 金鑰。CLI 通常將登入憑證儲存在作業系統鑰匙圈；如果鑰匙圈無法使用，可能改用明文設定檔（`~/.copilot/config.json`）。Know Giver 不會將憑證儲存在程式資料庫中。此整合僅支援文字，不支援圖片。

**使用你已有的帳號登入。** AI 步驟首先顯示「使用帳號登入」，適合已訂閱聊天方案但從未建立過 API 金鑰的人：**ChatGPT**（OpenAI 官方的「Sign in with ChatGPT」，課堂計入你的 ChatGPT 方案上限）、**Claude**（透過 Anthropic 官方程式 Claude Code，使用你的 Claude Pro 或 Max 帳號；Know Giver 可以幫你安裝）以及 **GitHub Copilot**。每項都在其所屬公司的頁面上登入，Know Giver 不會看到你的密碼。Know Giver 與 OpenAI、Anthropic 和 GitHub 沒有關聯。

<p align="center">
  <img src="docs/images/zh-Hant/setup-ai-account.png" alt="AI 步驟的「使用帳號登入」分頁：ChatGPT、Claude 和 GitHub Copilot，各有登入按鈕" width="60%">
</p>

**免費或付費。** 除了「使用帳號登入」，AI 步驟還有兩個分頁。「付費」顯示推薦的服務（CP 值高的 Z.ai、準確度高的 Claude）；「免費」列出有免費方案的服務（Gemini、Mistral、OpenRouter、Groq、NVIDIA、GitHub Models，以及在你電腦上執行的 Ollama 或 LM Studio），每項顯示方案包含的模型、如何使用你傳送的內容（有些免費方案會用於訓練）以及限額與規則的連結。

**你所在地區的備考方式。** 課堂、測驗、講義與申論題會依照你所在地區的備考方式進行：臺灣是國家考試的測驗題與申論題；香港是 HKDSE 與公務員考試的形式；中國大陸是行测與申论；日本是五選一與按字數寫的小論文；巴西、美國與英國也各有其題型。考試簡章與歷屆試題始終優先。

<p align="center">
  <img src="docs/images/zh-Hant/setup-ai-free.png" alt="AI 步驟的「免費」分頁：有免費方案的服務、包含的模型與資料使用方式" width="60%">
</p>

<table>
  <tr>
    <td><img src="docs/images/zh-Hant/setup-ai.png" alt="首次設定的 AI 步驟"></td>
    <td><img src="docs/images/zh-Hant/setup-terms.png" alt="首次設定的條款步驟"></td>
  </tr>
</table>

<p align="center">
  <img src="docs/images/zh-Hant/tour.png" alt="導覽：側邊欄標出「課堂」，旁邊有說明該頁面的卡片" width="80%">
</p>

## 隱私

- 語音識別和朗讀在你的電腦上執行，錄音不會離開電腦。
- 資料（SQLite 資料庫、模型、日誌）儲存在 `%LOCALAPPDATA%\KnowGiver`。
- 預設情況下，手寫照片在本機辨識，不會離開這台電腦。只有在設定 → AI 中明確選擇外部圖片 API 時，照片才會傳送給相應服務商；Copilot 一律使用本機 OCR。傳送前程式會顯示目的地。照片儲存在本機資料資料夾，刪除作答時一併刪除。掃描 PDF 頁面僅在你要求 OCR 時傳送給已設定的圖片模型。首次安裝本機 OCR 會從 Hugging Face 下載約 1.82 GB。
- 開啟時，程式會向 GitHub 查詢最新版本；只傳送這個請求，可在 設定 → 關於與更新 中關閉。
- 沒有賬戶、遙測或廣告。

Know Giver 在臺灣、香港、新加坡、巴西、美國、歐盟、英國和日本向公眾提供；臺灣與香港的條款見[使用條款](docs/legal/terms-of-use.zh-Hant.md)和[隱私權政策](docs/legal/privacy-policy.zh-Hant.md)，也會在程式中顯示。在其他國家和地區，僅限經書面授權的合作夥伴依[合作夥伴存取條款](docs/legal/partner-access.zh-Hant.md)使用。

## 安裝

每個版本都會發布為 [GitHub Release](https://github.com/czarist/know-giver/releases/latest)，提供 Windows（x64）、macOS（Apple 晶片）與 Linux（x86_64）的安裝包。下面的指令會下載最新版本，用 Release 的檢查碼核對後，為目前的使用者安裝，不需要系統管理員權限。你的計畫與進度都會保留。

**Windows**（PowerShell）：

```powershell
irm https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.ps1 | iex
```

**macOS**（終端機）：

```bash
curl -fsSL https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.sh | bash
```

**Linux**（終端機）：

```bash
curl -fsSL https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.sh | bash
```

Windows 安裝到 `%LOCALAPPDATA%\Programs\KnowGiver`，並加入開始功能表；macOS 安裝到 `~/Applications/Know Giver.app`；Linux 安裝到 `~/.local/opt/KnowGiver`，並提供 `know-giver` 指令與應用程式選單項目。

如果尚未安裝 Claude Code，首次安裝時還會詢問是否用 Anthropic 官方安裝程式一併安裝，以便使用 Claude 訂閱。之後也可以在 AI 步驟中安裝。

首次安裝時會詢問是否建立**桌面捷徑**，更新時保留先前的選擇。**解除安裝**：Windows 在 設定 → 應用程式 → Know Giver（也可用開始功能表中的「Uninstall Know Giver」，或程式資料夾中的 `unins000.exe`）；macOS 執行 `"~/Applications/Know Giver.app/Contents/Resources/uninstall.sh"`；Linux 執行 `~/.local/opt/KnowGiver/uninstall.sh`。解除安裝程式會詢問是否一併刪除學習資料，除非你選擇「是」，否則會保留。

**更新：** 有新版本時，程式頂端會出現一條細列，以綠色的下載按鈕提示更新。程式會下載、核對、安裝，然後重新開啟。可在 設定 → 關於與更新 中關閉檢查。

## 開始使用

```bash
cargo run --release
```

首次啟動時會開啟首次設定。需要 Rust 1.95 或更新版本。Windows 安裝包：

關閉視窗後，程式會繼續留在系統匣中：按兩下圖示可重新開啟視窗，按一下則顯示選單（開啟或結束）。程式執行時再次開啟 Know Giver，會回到同一個視窗；始終只有一個執行個體和一個系統匣圖示。安裝時可選擇是否隨系統啟動，以及是否接收學習提醒（預設每 3 小時一次，可在設定中改為 30 分鐘到 8 小時；兩項預設皆關閉）。提醒是螢幕角落的一個 Know Giver 小視窗，列出進度最慢的科目，並提供開始學習的按鈕。當你離開程式時（在系統匣中、最小化或位於其他視窗之後）才會提醒，課堂、測驗或複習進行中不會提醒，並且僅在目前計劃仍有未完成科目時顯示。

```powershell
powershell -ExecutionPolicy Bypass -File scripts\package.ps1
```

開發相關內容請參閱 [English README](README.md#development)（技術文件為英文）。

## 許可證

可選擇依 [MIT](LICENSE-MIT) 或 [Apache License 2.0](LICENSE-APACHE) 使用（`MIT OR Apache-2.0`）。第三方字型、肖像、語音和標誌各有其許可（見 [English README 的 Credits](README.md#credits)）。報告安全漏洞請參閱 [SECURITY.md](SECURITY.md)。
