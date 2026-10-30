Know Giver — 面向任何考试的学习计划和语音课堂

安装
最简单的方式，为当前用户安装和更新，不需要管理员权限：
  curl -fsSL https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.sh | bash
macOS：安装到 ~/Applications/Know Giver.app。应用尚未经过 Apple 签名。系统请求时请允许使用麦克风。
Linux：安装到 ~/.local/opt/KnowGiver，并提供 know-giver 命令和菜单项。
不用脚本时，解压安装包，在 KnowGiver 文件夹中运行 ./know-giver。

开始使用
首次设置会询问语言和地区、你的名字、老师、AI 服务和密钥，以及是否同意条款。
在“设置”中下载语音模型，它们在你的电脑上运行。中文课堂目前通过文字进行。

更新
有新版本时，应用顶部会出现提示栏。可在 设置 > 关于与更新 中关闭检查。

你的数据
macOS：~/Library/Application Support/KnowGiver。Linux：~/.local/share/KnowGiver。
KG_DATA_DIR 可指定其他文件夹。请勿分享 .env 文件。

卸载
macOS："~/Applications/Know Giver.app/Contents/Resources/uninstall.sh"
Linux：~/.local/opt/KnowGiver/uninstall.sh
删除学习数据前会先询问。

代码、帮助和更新内容：https://github.com/czarist/know-giver
