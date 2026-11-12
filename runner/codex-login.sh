#!/usr/bin/env bash
# Entra no Codex com a assinatura do ChatGPT dentro de um runner desta máquina. O login fica no
# volume do runner (codexN), não na imagem nem no .env, e o Codex renova o acesso sozinho.
#
# O Codex mostra um endereço e um código: abra o endereço em qualquer navegador, entre na sua
# conta e digite o código. A conta precisa permitir o login por código (ChatGPT → Configurações
# → Segurança). Quem usa a chave da API não precisa disto: `bash set-credentials.sh`.
#
# Uso (em runner/):  bash codex-login.sh [runner1]
set -euo pipefail
cd "$(dirname "$0")"
service="${1:-runner1}"
command -v docker >/dev/null || { echo "Falta o docker." >&2; exit 1; }
docker compose ps --services --status running 2>/dev/null | grep -Fxq "$service" \
  || { echo "O $service não está rodando. Suba com: docker compose up -d $service" >&2; exit 1; }
docker compose exec -u factory -e HOME=/home/factory "$service" codex login --device-auth
docker compose exec -u factory -e HOME=/home/factory "$service" codex login status
echo "Pronto. Para esta máquina usar o Codex, grave FACTORY_AGENT=codex no .env e rode: docker compose up -d $service"
