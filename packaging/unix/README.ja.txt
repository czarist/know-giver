Know Giver — あらゆる試験のための学習プランと音声レッスン

インストール
管理者権限なしで、お使いのユーザーにインストール・アップデートできます：
  curl -fsSL https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.sh | bash
macOS：~/Applications/Know Giver.app に入ります。まだ Apple の署名はありません。
マイクの使用を求められたら許可してください。
Linux：~/.local/opt/KnowGiver に入り、know-giver コマンドとメニュー項目ができます。
スクリプトを使わない場合は、パッケージを展開して KnowGiver フォルダーの ./know-giver を実行します。

はじめかた
初回設定で、言語と国・地域、名前、先生、AIサービスとキー、規約への同意を選びます。
設定で音声モデルをダウンロードします（お使いのコンピューターで動きます）。
日本語のレッスンは、現在は文字でのやり取りです。

アップデート
新しいバージョンが出ると、アプリの上部にバーが表示されます。
設定 > アプリについて・アップデート で確認をオフにできます。

データ
macOS：~/Library/Application Support/KnowGiver。Linux：~/.local/share/KnowGiver。
KG_DATA_DIR で別のフォルダーを指定できます。.env ファイルは共有しないでください。

アンインストール
macOS："~/Applications/Know Giver.app/Contents/Resources/uninstall.sh"
Linux：~/.local/opt/KnowGiver/uninstall.sh
学習データを削除する前に確認します。

コード、ヘルプ、新機能：https://github.com/czarist/know-giver
