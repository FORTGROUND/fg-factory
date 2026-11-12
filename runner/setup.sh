#!/usr/bin/env bash
# Configura a factory nesta máquina, do zero ou de novo: quantos runners, qual agente e as
# credenciais. Os valores secretos são pedidos numa janela do sistema (Windows, macOS ou Linux
# com área de trabalho), digitados uma vez e usados por todos os runners desta máquina.
#
# Uso (em runner/):  bash setup.sh
#
# Rodar de novo é seguro: cada pergunta mostra o valor atual, e Enter (ou OK com o campo vazio)
# mantém o que já está gravado.
#
# O que ele faz:
#   1. pergunta o nome da máquina, quantos runners (tarefas ao mesmo tempo) e qual agente usar;
#   2. pede as credenciais numa janela: a do Claude Code (ou a do Codex) e o token do FG Tracker;
#   3. grava tudo no .env desta pasta (ignorado pelo git, modo 600);
#   4. registra e sobe os runners (o token de registro sai do `gh`, sem copiar nada);
#   5. com o Codex pela assinatura do ChatGPT, faz o login uma vez, para todos os runners;
#   6. confere que os runners estão online e oferece apagar segredos antigos dos repositórios,
#      que passariam por cima das credenciais desta máquina.
set -euo pipefail
cd "$(dirname "$0")"

# Para testes: FACTORY_SETUP_DRY_RUN=1 não chama docker nem gh; FACTORY_SETUP_NO_DIALOG=1 pergunta no terminal.
DRY="${FACTORY_SETUP_DRY_RUN:-}"
ENV_FILE="${FACTORY_SETUP_ENV:-.env}"
MAX_RUNNERS=4

say() { printf '%s\n' "$*"; }
fail() { printf 'Erro: %s\n' "$*" >&2; exit 1; }
# Lê uma linha do terminal (no teste, da entrada padrão). `-s` não mostra o que é digitado.
read_line() {
  if [ -z "$DRY" ] && [ -r /dev/tty ]; then IFS= read -r "$@" < /dev/tty || true; else IFS= read -r "$@" || true; fi
}

# ---- o .env --------------------------------------------------------------------------------
env_get() { [ -f "$ENV_FILE" ] && sed -n "s/^$1=//p" "$ENV_FILE" | tail -1 || true; }
env_set() {  # env_set CHAVE VALOR; VALOR vazio apaga a linha
  local tmp; tmp="$(mktemp "$ENV_FILE.XXXXXX")"; chmod 600 "$tmp"
  { grep -v "^$1=" "$ENV_FILE" 2>/dev/null || true; [ -z "$2" ] || printf '%s=%s\n' "$1" "$2"; } > "$tmp"
  mv "$tmp" "$ENV_FILE"
}

# ---- perguntas ------------------------------------------------------------------------------
ask() {  # ask "pergunta" "padrão" -> resposta (Enter = padrão)
  local answer
  printf '%s [%s]: ' "$1" "$2" >&2
  answer=""; read_line answer
  printf '%s' "${answer:-$2}"
}

dialog_kind() {
  if [ -n "${FACTORY_SETUP_NO_DIALOG:-}" ] || [ -n "$DRY" ]; then echo terminal; return; fi
  if grep -qi microsoft /proc/version 2>/dev/null && command -v powershell.exe >/dev/null; then echo windows; return; fi
  if [ "$(uname -s)" = Darwin ] && command -v osascript >/dev/null; then echo mac; return; fi
  if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ] && command -v zenity >/dev/null; then echo zenity; return; fi
  echo terminal
}

# A janela do Windows (pelo WSL): um campo que não mostra o que é digitado. O texto vai dentro
# do script codificado; o valor volta pela saída padrão, nunca por argumento ou arquivo.
windows_dialog() {
  local title="${1//\'/\'\'}" message="${2//\'/\'\'}" script
  script="Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()
\$f = New-Object System.Windows.Forms.Form
\$f.Text = '$title'; \$f.StartPosition = 'CenterScreen'; \$f.TopMost = \$true
\$f.FormBorderStyle = 'FixedDialog'; \$f.MaximizeBox = \$false; \$f.MinimizeBox = \$false
\$f.ClientSize = New-Object System.Drawing.Size(500, 210)
\$l = New-Object System.Windows.Forms.Label; \$l.Text = '$message'; \$l.SetBounds(16, 14, 468, 112)
\$t = New-Object System.Windows.Forms.TextBox; \$t.UseSystemPasswordChar = \$true; \$t.SetBounds(16, 132, 468, 24)
\$ok = New-Object System.Windows.Forms.Button; \$ok.Text = 'OK'; \$ok.SetBounds(308, 170, 84, 28); \$ok.DialogResult = 'OK'
\$no = New-Object System.Windows.Forms.Button; \$no.Text = 'Cancelar'; \$no.SetBounds(400, 170, 84, 28); \$no.DialogResult = 'Cancel'
\$f.Controls.AddRange(@(\$l, \$t, \$ok, \$no)); \$f.AcceptButton = \$ok; \$f.CancelButton = \$no
\$f.Add_Shown({ \$f.Activate(); \$t.Focus() })
if (\$f.ShowDialog() -eq 'OK') { [Console]::Out.Write(\$t.Text); exit 0 } else { exit 1 }"
  powershell.exe -NoProfile -STA -EncodedCommand "$(printf '%s' "$script" | iconv -t UTF-16LE | base64 -w0)" | tr -d '\r'
}

# ask_secret "título" "explicação" -> o valor digitado (sem espaços). Cancelar devolve erro.
ask_secret() {
  local value status=0
  case "$(dialog_kind)" in
    windows) value="$(windows_dialog "$1" "$2")" || status=$? ;;
    mac) value="$(osascript -e 'on run argv' \
             -e 'display dialog (item 2 of argv) with title (item 1 of argv) default answer "" with hidden answer buttons {"Cancelar", "OK"} default button "OK" cancel button "Cancelar"' \
             -e 'return text returned of result' -e 'end run' "$1" "$2" 2>/dev/null)" || status=$? ;;
    zenity) value="$(zenity --entry --hide-text --title "$1" --text "$2" 2>/dev/null)" || status=$? ;;
    *) printf '\n%s\n%s\n(nada aparece na tela; Enter mantém o atual): ' "$1" "$2" >&2
       value=""; read_line -s value; echo >&2 ;;
  esac
  [ "$status" -eq 0 ] || return 1
  printf '%s' "${value//[[:space:]]/}"
}

# ---- comandos externos (nada no modo de teste) ---------------------------------------------
run() { if [ -n "$DRY" ]; then say "[teste] $*"; else "$@"; fi; }
compose() { run docker compose "$@"; }

# ---- 0. o que precisa existir ---------------------------------------------------------------
if [ -z "$DRY" ]; then
  command -v docker >/dev/null || fail "falta o Docker (Docker Desktop no Windows e no macOS)."
  docker info >/dev/null 2>&1 || fail "o Docker não está rodando. Abra o Docker Desktop e tente de novo."
  command -v gh >/dev/null || fail "falta o gh (https://cli.github.com). Depois: gh auth login."
  gh auth status >/dev/null 2>&1 || fail "o gh não está logado. Rode: gh auth login."
fi
[ -f "$ENV_FILE" ] || { cp .env.example "$ENV_FILE"; chmod 600 "$ENV_FILE"; }
chmod 600 "$ENV_FILE"

say "Configuração da factory nesta máquina. Enter aceita o valor entre colchetes."
say ""

# ---- 1. máquina, runners e agente ------------------------------------------------------------
org="$(env_get FACTORY_ORG)"; case "$org" in ''|minha-org) org=FORTGROUND ;; esac
org="$(ask "Organização do GitHub" "$org")"
machine="$(env_get FACTORY_MACHINE)"
case "$machine" in ''|meu-notebook) machine="$(hostname -s 2>/dev/null || hostname)" ;; esac
machine="$(ask "Nome desta máquina no painel" "$machine" | tr 'A-Z' 'a-z' | tr -c 'a-z0-9-' '-' | sed 's/-\{2,\}/-/g; s/^-//; s/-$//')"
[ -n "$machine" ] || fail "o nome da máquina não pode ficar vazio."

running=0
if [ -z "$DRY" ]; then running="$(docker compose ps --services --status running 2>/dev/null | grep -c '^runner[0-9]' || true)"; fi
count="$(ask "Quantas tarefas ao mesmo tempo nesta máquina (1 a $MAX_RUNNERS; cada uma é um runner)" "$([ "$running" -gt 0 ] && echo "$running" || echo 1)")"
case "$count" in [1-9]) [ "$count" -le "$MAX_RUNNERS" ] || fail "no máximo $MAX_RUNNERS." ;; *) fail "responda um número de 1 a $MAX_RUNNERS." ;; esac

agent="$(env_get FACTORY_AGENT)"; agent="${agent:-claude}"
case "$(ask "Qual agente corrige as issues: 1 = Claude Code, 2 = Codex" "$([ "$agent" = codex ] && echo 2 || echo 1)")" in
  2|codex) agent=codex ;; *) agent=claude ;;
esac

docker_on=n; case "$(env_get COMPOSE_FILE)" in *compose.docker.yml*) docker_on=s ;; esac
[ -n "$(env_get COMPOSE_FILE)" ] || docker_on=s
case "$(ask "Os testes dos projetos podem subir contêineres (banco, etc.)? Liga um Docker próprio por runner (s/n)" "$docker_on")" in
  s|S|sim|y|Y) docker_on=s ;; *) docker_on=n ;;
esac

# ---- 2. credenciais -------------------------------------------------------------------------
say ""
say "Agora as credenciais. Cada uma abre uma janela; o que você digita não aparece."
changed_tracker=false

has() { [ -n "$(env_get "$1")" ]; }
keep_note() { if has "$1"; then printf ' Deixe vazio para manter o que já está gravado.'; fi; }

if [ "$agent" = claude ]; then
  note=""; if has CLAUDE_CODE_OAUTH_TOKEN || has ANTHROPIC_API_KEY; then note=" Deixe vazio para manter o que já está gravado."; fi
  if value="$(ask_secret "Factory: Claude Code" "Cole o token do Claude Code. Para gerar, rode no terminal: claude setup-token (abre o navegador e imprime um token que começa com sk-ant-oat). Uma chave da API da Anthropic (sk-ant-api...) também serve.$note")"; then
    case "$value" in
      '') ;;
      sk-ant-api*) env_set ANTHROPIC_API_KEY "$value"; env_set CLAUDE_CODE_OAUTH_TOKEN "" ;;
      *) env_set CLAUDE_CODE_OAUTH_TOKEN "$value"; env_set ANTHROPIC_API_KEY "" ;;
    esac
  fi
  has CLAUDE_CODE_OAUTH_TOKEN || has ANTHROPIC_API_KEY || fail "sem a credencial do Claude Code não há como corrigir. Rode de novo quando tiver o token."
fi

codex_login=false
if [ "$agent" = codex ]; then
  how="$(ask "Codex: 1 = assinatura do ChatGPT (login no navegador), 2 = chave da API da OpenAI" "$(has OPENAI_API_KEY && echo 2 || echo 1)")"
  if [ "$how" = 2 ]; then
    if value="$(ask_secret "Factory: Codex" "Cole a chave da API da OpenAI (sk-...).$(keep_note OPENAI_API_KEY)")" && [ -n "$value" ]; then env_set OPENAI_API_KEY "$value"; fi
    has OPENAI_API_KEY || fail "sem a chave da API o Codex não roda. Rode de novo e escolha 1 para usar a assinatura."
  else
    env_set OPENAI_API_KEY ""
    codex_login=true
  fi
fi

if value="$(ask_secret "Factory: FG Tracker" "Cole o token do FG Tracker. No painel: Tokens de acesso, Novo token, modelo FG Factory, restrito à organização. Ele lê a telemetria para investigar e mostra as execuções no painel.$(keep_note FG_TRACKER_TOKEN)")" && [ -n "$value" ]; then
  env_set FG_TRACKER_TOKEN "$value"; changed_tracker=true
fi
has FG_TRACKER_TOKEN || fail "o token do FG Tracker é obrigatório. Rode de novo quando tiver criado o token."
unset value

# ---- 3. o .env ------------------------------------------------------------------------------
env_set FACTORY_ORG "$org"
env_set FACTORY_MACHINE "$machine"
env_set FACTORY_AGENT "$agent"
if [ "$docker_on" = s ]; then env_set COMPOSE_FILE "compose.yml:compose.docker.yml"; else env_set COMPOSE_FILE "compose.yml"; fi
# Um token só: o token separado do painel, de antes, passaria por cima do novo.
if $changed_tracker; then env_set FG_TRACKER_FACTORY_TOKEN ""; fi
# O token de registro vale uma hora e só serve na primeira subida: não fica gravado.
env_set FACTORY_REGISTRATION_TOKEN ""
say ""
say "Gravado em runner/$ENV_FILE (só o seu usuário lê)."

# ---- 4. registrar e subir os runners ---------------------------------------------------------
services=(); for i in $(seq 1 "$count"); do services+=("runner$i"); done
if [ -z "$DRY" ]; then
  # Recriar um runner no meio de uma tarefa a interromperia.
  busy="$(gh api "orgs/$org/actions/runners" --paginate --jq ".runners[] | select(.busy and (.name | startswith(\"factory-$machine-\"))) | .name" 2>/dev/null || true)"
  [ -z "$busy" ] || fail "estes runners estão no meio de uma tarefa: $(echo $busy). Rode de novo quando terminarem."
  # Atualiza o fg-factory, quando esta pasta é um clone dele sem mudanças locais.
  if git -C .. rev-parse --is-inside-work-tree >/dev/null 2>&1 && [ -z "$(git -C .. status --porcelain)" ]; then
    git -C .. pull -q --ff-only 2>/dev/null || true
  fi
fi
case "$org" in */*) token_path="repos/$org/actions/runners/registration-token" ;; *) token_path="orgs/$org/actions/runners/registration-token" ;; esac
registration=""
if [ -z "$DRY" ]; then registration="$(gh api -X POST "$token_path" --jq .token 2>/dev/null || true)"; fi
if [ -z "$registration" ] && [ -z "$DRY" ]; then
  say "O gh não conseguiu gerar o token de registro (é preciso ser owner da organização)."
  registration="$(ask_secret "Factory: registro do runner" "Cole um token de registro: github.com/organizations/$org/settings/actions/runners/new mostra um depois de --token. Ele vale uma hora.")" || fail "sem o token de registro os runners novos não entram."
fi
say "Montando a imagem e subindo ${services[*]} ..."
compose build -q
FACTORY_REGISTRATION_TOKEN="$registration" compose up -d --force-recreate "${services[@]}"
unset registration
for i in $(seq $((count + 1)) "$MAX_RUNNERS"); do
  if [ -z "$DRY" ] && docker compose ps --services --status running 2>/dev/null | grep -qx "runner$i"; then
    say "Parando runner$i (agora são $count)."; compose stop "runner$i" >/dev/null
  fi
done

# ---- 5. login do Codex, uma vez para todos ----------------------------------------------------
if $codex_login; then
  if [ -z "$DRY" ] && docker compose exec -T -u factory -e HOME=/home/factory runner1 codex login status >/dev/null 2>&1; then
    say "O Codex já está logado nesta máquina."
  else
    say ""
    say "Login do Codex: abra o endereço que aparece abaixo, entre na sua conta e digite o código."
    say "(A conta precisa permitir o login por código: ChatGPT, Configurações, Segurança.)"
    compose exec -u factory -e HOME=/home/factory runner1 codex login --device-auth
  fi
fi

# ---- 6. conferência ----------------------------------------------------------------------------
if [ -z "$DRY" ]; then
  say ""
  say "Esperando os runners aparecerem no GitHub ..."
  for _ in $(seq 1 30); do
    online="$(gh api "orgs/$org/actions/runners" --paginate --jq "[.runners[] | select(.status == \"online\" and (.name | startswith(\"factory-$machine-\")))] | length" 2>/dev/null || echo 0)"
    [ "${online:-0}" -ge "$count" ] && break
    sleep 4
  done
  for service in "${services[@]}"; do
    if docker compose exec -T -u factory -e HOME=/home/factory "$service" test -s /home/factory/.fg-factory/credentials 2>/dev/null; then
      say "  $service: credenciais guardadas"
    else
      say "  $service: SEM credenciais (veja: docker compose logs $service)"
    fi
  done
  say "  online no GitHub: ${online:-0} de $count"

  # Segredos de repositório passam por cima das credenciais do runner. Os antigos, de antes de
  # elas ficarem aqui, só confundem: oferece apagar.
  names="FACTORY_CLAUDE_TOKEN FACTORY_ANTHROPIC_API_KEY FACTORY_OPENAI_API_KEY FACTORY_FG_TRACKER_TOKEN FACTORY_FG_TRACKER_PANEL_TOKEN"
  found=""
  case "$org" in */*) repos="$org" ;; *) repos="$(gh repo list "$org" --limit 200 --no-archived --json nameWithOwner --jq '.[].nameWithOwner' 2>/dev/null || true)" ;; esac
  for repo in $repos; do
    have="$(gh secret list --repo "$repo" --json name --jq '.[].name' 2>/dev/null || true)"
    for name in $names; do
      if grep -qx "$name" <<< "$have"; then found+="$repo $name"$'\n'; fi
    done
  done
  if [ -n "$found" ]; then
    say ""
    say "Estes segredos de repositório passam por cima das credenciais desta máquina:"
    printf '%s' "$found" | sed 's/^/  /'
    if [ "$(ask "Apagar, para todos usarem as credenciais guardadas nos runners? (s/n)" s)" = s ]; then
      while read -r repo name; do
        [ -n "$repo" ] || continue
        gh secret delete "$name" --repo "$repo" >/dev/null && say "  apagado: $repo $name"
      done <<< "$found"
    fi
  fi
fi

say ""
say "Pronto. Esta máquina roda até $count tarefa(s) ao mesmo tempo com o $([ "$agent" = codex ] && echo Codex || echo "Claude Code"),"
say "para todos os repositórios de $org que têm o workflow da factory."
say "Para mudar algo (mais runners, outro agente, um token novo), rode bash setup.sh de novo."
