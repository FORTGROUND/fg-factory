#!/usr/bin/env bash
# Transforma esta máquina num runner da factory. Rode como o usuário DEDICADO da factory (não a
# sua conta do dia a dia), numa máquina sem credenciais de produção. Veja o README.
#
# Uso:  bash setup-runner.sh <token-de-registro> <organização> [quantidade]
#
#   token-de-registro  um owner gera com:
#                        gh api -X POST orgs/<organização>/actions/runners/registration-token --jq .token
#                      (vale 1 hora)
#   organização        a organização do GitHub (ou dono/repositório, numa conta pessoal)
#   quantidade         quantas issues esta máquina faz ao mesmo tempo (padrão 1)
#
# Cada runner executa um job por vez: a quantidade é o limite de trabalho simultâneo da máquina.
set -euo pipefail
TOKEN="${1:?Informe o token de registro}"
ORG="${2:?Informe a organização (ou dono/repositório)}"
COUNT="${3:-1}"
BASE="$HOME/factory-runners"

fail() { echo "ERRO: $*" >&2; exit 1; }

# 1. A máquina não pode ter acesso à produção. A mesma checagem roda no início de cada job.
found=()
for path in "$HOME/.aws/credentials" "$HOME/.aws/config" "$HOME/.ssh/id_rsa" "$HOME/.ssh/id_ed25519" "$HOME/.kube/config" "$HOME/.config/gcloud"; do
  if [ -e "$path" ]; then found+=("$path"); fi
done
if [ "${#found[@]}" -gt 0 ]; then
  fail "este usuário tem credenciais (${found[*]}). Use um usuário dedicado, sem elas."
fi

# 2. Ferramentas que todo job usa. As de cada produto (Rust, pnpm...) ficam por conta da máquina.
missing=()
for tool in git gh jq curl tar node npx claude; do
  command -v "$tool" >/dev/null || missing+=("$tool")
done
if [ "${#missing[@]}" -gt 0 ]; then
  fail "instale antes: ${missing[*]} (Claude Code: npm install -g @anthropic-ai/claude-code)"
fi

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)  PKG="linux-x64" ;;
  Linux-aarch64) PKG="linux-arm64" ;;
  Darwin-arm64)  PKG="osx-arm64" ;;
  Darwin-x86_64) PKG="osx-x64" ;;
  *) fail "sistema não suportado: $(uname -s) $(uname -m)" ;;
esac
VERSION="$(curl -fsSL https://api.github.com/repos/actions/runner/releases/latest | jq -r .tag_name | sed 's/^v//')"
[ -n "$VERSION" ] || fail "não descobri a versão do runner"
ARCHIVE="actions-runner-$PKG-$VERSION.tar.gz"
mkdir -p "$BASE"
curl -fsSL -o "$BASE/$ARCHIVE" "https://github.com/actions/runner/releases/download/v$VERSION/$ARCHIVE"

HOST="$(hostname -s | tr -c 'A-Za-z0-9-' '-' | sed 's/-*$//')"
for i in $(seq 1 "$COUNT"); do
  dir="$BASE/runner-$i"
  if [ -e "$dir/.runner" ]; then echo "runner-$i já está registrado; mantido."; continue; fi
  mkdir -p "$dir" && tar -xzf "$BASE/$ARCHIVE" -C "$dir"
  # --no-default-labels: sem `self-hosted`, `macOS`, `Linux`... este runner só atende jobs que
  # pedem `factory`. Sem isso ele pegaria o deploy de produção, que pede [self-hosted, macOS, ARM64].
  (cd "$dir" && ./config.sh --unattended --url "https://github.com/$ORG" --token "$TOKEN" \
      --name "factory-$HOST-$i" --labels factory --no-default-labels --work _work --replace)
  if [ "$(uname -s)" = "Linux" ] && command -v systemctl >/dev/null && [ -d /run/systemd/system ]; then
    echo "Instalando o runner-$i como serviço (pede sudo)..."
    (cd "$dir" && sudo ./svc.sh install "$USER" && sudo ./svc.sh start)
  elif [ "$(uname -s)" = "Darwin" ]; then
    (cd "$dir" && ./svc.sh install && ./svc.sh start)
  else
    echo "Sem systemd: inicie o runner-$i à mão com:  (cd $dir && nohup ./run.sh &)"
  fi
done
rm -f "$BASE/$ARCHIVE"
echo "Pronto: $COUNT runner(s) factory-$HOST-* registrados em $ORG."
echo "Confira em https://github.com/organizations/$ORG/settings/actions/runners"
