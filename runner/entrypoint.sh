#!/usr/bin/env bash
# Registra o runner na primeira subida e o mantém rodando. O registro fica no volume do
# contêiner (/home/factory/runner), então reiniciar não pede token novo.
set -euo pipefail
cd /home/factory/runner
ORG="${FACTORY_ORG:?defina FACTORY_ORG}"
NAME="${FACTORY_RUNNER_NAME:-factory-$(hostname)}"

if [ ! -f .runner ]; then
  : "${FACTORY_REGISTRATION_TOKEN:?primeira subida: defina FACTORY_REGISTRATION_TOKEN (README)}"
  # --no-default-labels: só o rótulo factory. Com os padrões (self-hosted, Linux, X64...) este
  # runner poderia pegar jobs de outros workflows da organização, como o deploy de produção.
  ./config.sh --unattended --url "https://github.com/$ORG" --token "$FACTORY_REGISTRATION_TOKEN" \
    --name "$NAME" --labels factory --no-default-labels --work _work --replace
fi
unset FACTORY_REGISTRATION_TOKEN
exec ./run.sh
