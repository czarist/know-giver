Know Giver — planos de estudo e aulas por voz para qualquer prova

INSTALAR
O jeito mais simples instala e atualiza para o seu usuário, sem administrador:
  curl -fsSL https://raw.githubusercontent.com/czarist/know-giver/main/scripts/install.sh | bash
macOS: vai para ~/Applications/Know Giver.app. O app ainda não é assinado pela
Apple. Autorize o microfone quando o sistema pedir.
Linux: vai para ~/.local/opt/KnowGiver, com o comando know-giver e um atalho no
menu. Sem o script, extraia o pacote e execute ./know-giver na pasta KnowGiver.

PRIMEIROS PASSOS
O primeiro acesso pede idioma e região, seu nome, um professor, o serviço de IA
com a sua chave e o aceite dos termos. Em Configurações, baixe os modelos de voz;
eles rodam no seu computador.

ATUALIZAÇÕES
Quando sai uma versão nova, uma barra no topo do app oferece a atualização.
Configurações > Sobre e atualizações desliga a busca.

SEUS DADOS
macOS: ~/Library/Application Support/KnowGiver. Linux: ~/.local/share/KnowGiver.
KG_DATA_DIR escolhe outra pasta. Nunca compartilhe um arquivo .env.

DESINSTALAR
macOS: "~/Applications/Know Giver.app/Contents/Resources/uninstall.sh"
Linux: ~/.local/opt/KnowGiver/uninstall.sh
Ele pergunta antes de apagar os dados de estudo.

Código, ajuda e novidades: https://github.com/czarist/know-giver
