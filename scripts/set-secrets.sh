#!/usr/bin/env bash
# Grava os segredos da factory num repositório participante. Os valores são lidos sem
# aparecer na tela e vão direto para o GitHub; não ficam em arquivo nem no histórico do shell.
#
# Uso:  bash scripts/set-secrets.sh minha-org/meu-repo [app-id chave.pem]
#
# Os dois últimos são opcionais: o id e o arquivo da chave privada do GitHub App da factory,
# que dão acesso aos outros repositórios da organização (README, "Vários repositórios").
#
# Antes:
#   - Claude:      rode `claude setup-token` numa máquina logada e copie o token.
#   - FG Tracker:  crie um token SÓ DE LEITURA (Telemetria, Issues, Releases, Alertas: ler).
set -euo pipefail
REPO="${1:?Informe o repositório, ex.: minha-org/meu-repo}"
command -v gh >/dev/null || { echo "Falta o gh." >&2; exit 1; }

ask() { # nome do segredo, texto
  local value
  printf '%s (nada aparece na tela; Enter vazio pula): ' "$2"
  IFS= read -rs value; echo
  value="${value//[[:space:]]/}"
  if [ -z "$value" ]; then echo "  pulado: $1"; return; fi
  printf '%s' "$value" | gh secret set "$1" --repo "$REPO"
  unset value
}
ask FACTORY_CLAUDE_TOKEN     "Token do Claude (claude setup-token)"
ask FACTORY_ANTHROPIC_API_KEY "Ou a chave da API da Anthropic, se não usa o token acima"
ask FACTORY_FG_TRACKER_TOKEN "Token do FG Tracker, só de leitura"
if [ -n "${2:-}" ] && [ -n "${3:-}" ]; then
  [ -r "$3" ] || { echo "Não consigo ler $3." >&2; exit 1; }
  printf '%s' "$2" | gh secret set FACTORY_APP_ID --repo "$REPO"
  gh secret set FACTORY_APP_PRIVATE_KEY --repo "$REPO" < "$3"
fi
echo "Segredos de $REPO:"; gh secret list --repo "$REPO" | grep -E '^FACTORY_' || echo "  nenhum"
