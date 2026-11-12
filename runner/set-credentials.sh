#!/usr/bin/env bash
# Guarda no runner as credenciais que valem para todos os repositórios atendidos por ele: a do
# Claude Code e o token do FG Tracker. Com elas aqui, um repositório novo só precisa do
# .github/workflows/factory.yml, sem segredos.
#
# Os valores são lidos sem aparecer na tela, vão só para o .env desta pasta (ignorado pelo git,
# modo 600) e não são impressos. Enter sem digitar nada mantém o que já está gravado.
#
# Uso (em runner/):  bash set-credentials.sh
set -euo pipefail
cd "$(dirname "$0")"
[ -f .env ] || { echo "Não achei o .env. Crie a partir do .env.example primeiro (README)." >&2; exit 1; }
command -v docker >/dev/null || { echo "Falta o docker." >&2; exit 1; }

ask() {  # ask <VARIÁVEL> <pergunta>
  local value
  printf '%s\n(Enter mantém o atual; nada aparece na tela): ' "$2"
  IFS= read -rs value; echo
  value="${value//[[:space:]]/}"
  [ -n "$value" ] || return 0
  local tmp; tmp="$(mktemp .env.XXXXXX)"; chmod 600 "$tmp"
  grep -v "^$1=" .env > "$tmp" || true
  printf '%s=%s\n' "$1" "$value" >> "$tmp"
  mv "$tmp" .env
  echo "  $1 gravado."
}

ask CLAUDE_CODE_OAUTH_TOKEN 'Token do Claude Code (o que `claude setup-token` imprime). Quem usa a chave da API deixa em branco'
ask ANTHROPIC_API_KEY 'Chave da API da Anthropic. Quem usa o token acima deixa em branco'
ask FG_TRACKER_TOKEN 'Token do FG Tracker criado pelo modelo "FG Factory", restrito a uma organização'
chmod 600 .env

running="$(docker compose ps --services --status running 2>/dev/null || true)"
if [ -n "$running" ]; then
  echo "Reiniciando os runners em execução: $(echo $running) ..."
  # shellcheck disable=SC2086
  docker compose up -d --force-recreate $running >/dev/null
fi
echo "Pronto. Os repositórios atendidos por esta máquina não precisam mais dos segredos do Claude e do FG Tracker."
