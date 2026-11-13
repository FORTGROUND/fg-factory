#!/usr/bin/env bash
# Configura a factory nesta máquina: quantos runners, qual agente corrige e qual revisa, e as
# credenciais. Os valores secretos são digitados uma vez e valem para todos os runners desta
# máquina e para todos os repositórios da organização que têm o workflow da factory.
#
# O jeito mais fácil é o app "FG Factory Setup" (pasta app/ do repositório), que chama este
# script. Ele também roda sozinho, perguntando no terminal ou numa janela do sistema:
#
#   bash setup.sh                 pergunta tudo e configura
#   bash setup.sh --check         diz, em JSON, o que a máquina tem (usado pelo app)
#   bash setup.sh --apply         configura com as respostas lidas da entrada (usado pelo app)
#   bash setup.sh --install-tools instala o GitHub CLI (gh) e o jq em ~/.local/bin, sem sudo
#   bash setup.sh --gh-login      entra no GitHub pelo navegador, com código
#   bash setup.sh --login-claude  conecta a conta do Claude (claude setup-token, no contêiner)
#   bash setup.sh --login-codex   conecta a conta do ChatGPT ao Codex (no contêiner, vale para todos)
#
# Rodar de novo é seguro: o que fica em branco mantém o valor gravado.
#
# O que a configuração faz: grava o .env desta pasta (ignorado pelo git, modo 600), registra os
# runners na organização com o `gh`, sobe os runners, faz o login do Codex quando a máquina usa
# a assinatura do ChatGPT, confere que eles ficaram online e, se pedido, apaga segredos antigos
# dos repositórios, que passariam por cima das credenciais desta máquina.
set -euo pipefail
cd "$(dirname "$0")"
export PATH="$HOME/.local/bin:$PATH"

# Para testes: FACTORY_SETUP_DRY_RUN=1 não chama docker nem gh; FACTORY_SETUP_NO_DIALOG=1 pergunta no terminal.
DRY="${FACTORY_SETUP_DRY_RUN:-}"
ENV_FILE="${FACTORY_SETUP_ENV:-.env}"
MAX_RUNNERS=4
# Num build pesado (Rust, Next) cada runner chega a uns 3 núcleos e 6 GB; a máquina fica com folga.
PER_RUNNER_CPUS=3
PER_RUNNER_GB=6
RESERVE_CPUS=2
RESERVE_GB=4

say() { printf '%s\n' "$*"; }
fail() { printf '::fail::%s\n' "$*"; printf 'Erro: %s\n' "$*" >&2; exit 1; }

# ---- o .env --------------------------------------------------------------------------------
env_get() { [ -f "$ENV_FILE" ] && sed -n "s/^$1=//p" "$ENV_FILE" | tail -1 || true; }
env_set() {  # env_set CHAVE VALOR; VALOR vazio apaga a linha
  local tmp; tmp="$(mktemp "$ENV_FILE.XXXXXX")"; chmod 600 "$tmp"
  { grep -v "^$1=" "$ENV_FILE" 2>/dev/null || true; [ -z "$2" ] || printf '%s=%s\n' "$1" "$2"; } > "$tmp"
  mv "$tmp" "$ENV_FILE"
}
has() { [ -n "$(env_get "$1")" ]; }
ensure_env() { [ -f "$ENV_FILE" ] || cp .env.example "$ENV_FILE"; chmod 600 "$ENV_FILE"; }

# ---- a máquina -------------------------------------------------------------------------------
docker_resources() {  # "núcleos memória_em_GB", do que o Docker enxerga (no Docker Desktop, a VM dele)
  local line; line="$(docker info --format '{{.NCPU}} {{.MemTotal}}' 2>/dev/null || true)"
  [ -n "$line" ] || { echo "0 0"; return; }
  echo "${line% *} $(( ${line#* } / 1073741824 ))"
}
suggest_runners() {  # suggest_runners núcleos GB
  local by_cpu=$(( ($1 - RESERVE_CPUS) / PER_RUNNER_CPUS )) by_mem=$(( ($2 - RESERVE_GB) / PER_RUNNER_GB )) n
  n=$(( by_cpu < by_mem ? by_cpu : by_mem ))
  [ "$n" -le "$MAX_RUNNERS" ] || n=$MAX_RUNNERS
  [ "$n" -ge 1 ] || n=1
  echo "$n"
}
running_runners() {
  [ -z "$DRY" ] || { echo 0; return; }
  docker compose ps --services --status running 2>/dev/null | grep -c '^runner[0-9]' || true
}
default_machine() {
  local m; m="$(env_get FACTORY_MACHINE)"
  case "$m" in ''|meu-notebook) m="$(hostname -s 2>/dev/null || hostname)" ;; esac
  clean_machine "$m"
}
clean_machine() { printf '%s' "$1" | tr 'A-Z' 'a-z' | tr -c 'a-z0-9-' '-' | sed 's/-\{2,\}/-/g; s/^-//; s/-$//'; }
default_org() { local o; o="$(env_get FACTORY_ORG)"; case "$o" in ''|minha-org) o=FORTGROUND ;; esac; printf '%s' "$o"; }

# ---- --check: o que a máquina tem, em JSON --------------------------------------------------
check() {
  if ! command -v jq >/dev/null; then
    printf '{"missingTools":["jq"%s],"os":"%s"}\n' "$(command -v gh >/dev/null || printf ',"gh"')" "$(uname -s)"; return
  fi
  local docker_installed=false docker_running=false cpus=0 gb=0 gh_installed=false gh_logged=false
  local gh_user="" orgs="[]" git_ok=false codex_logged=false running=0 os
  os="$(uname -s)"; grep -qi microsoft /proc/version 2>/dev/null && os="WSL"
  command -v git >/dev/null && git_ok=true
  if command -v docker >/dev/null; then
    docker_installed=true
    if docker info >/dev/null 2>&1; then
      docker_running=true
      read -r cpus gb <<< "$(docker_resources)"
      running="$(running_runners)"
      if docker compose ps --services --status running 2>/dev/null | grep -qx runner1 \
         && docker compose exec -T -u factory -e HOME=/home/factory runner1 codex login status >/dev/null 2>&1; then
        codex_logged=true
      fi
    fi
  fi
  if command -v gh >/dev/null; then
    gh_installed=true
    if gh auth status >/dev/null 2>&1; then
      gh_logged=true
      gh_user="$(gh api user --jq .login 2>/dev/null || true)"
      # As organizações em que a pessoa é owner: só owner gera o token de registro de runner.
      orgs="$(gh api user/memberships/orgs --paginate --jq '[.[] | select(.state == "active" and .role == "admin") | .organization.login]' 2>/dev/null | jq -s 'add // []' 2>/dev/null || echo '[]')"
    fi
  fi
  local suggestion=1; [ "$cpus" -gt 0 ] && suggestion="$(suggest_runners "$cpus" "$gb")"
  jq -n --arg os "$os" --argjson git "$git_ok" \
    --argjson di "$docker_installed" --argjson dr "$docker_running" --argjson cpus "$cpus" --argjson gb "$gb" \
    --argjson gi "$gh_installed" --argjson gl "$gh_logged" --arg user "$gh_user" --argjson orgs "${orgs:-[]}" \
    --arg org "$(default_org)" --arg machine "$(default_machine)" \
    --arg agent "$(env_get FACTORY_AGENT)" --arg review "$(env_get FACTORY_REVIEW_AGENT)" \
    --arg compose "$(env_get COMPOSE_FILE)" --argjson running "${running:-0}" --argjson suggestion "$suggestion" \
    --argjson claude "$(has CLAUDE_CODE_OAUTH_TOKEN && echo true || echo false)" \
    --argjson anthropic "$(has ANTHROPIC_API_KEY && echo true || echo false)" \
    --argjson openai "$(has OPENAI_API_KEY && echo true || echo false)" \
    --argjson tracker "$(has FG_TRACKER_TOKEN && echo true || echo false)" \
    --argjson codex "$codex_logged" --arg dir "$PWD" \
    --argjson per_cpu "$PER_RUNNER_CPUS" --argjson per_gb "$PER_RUNNER_GB" --argjson max "$MAX_RUNNERS" '
    {os: $os, dir: $dir, git: $git,
     docker: {installed: $di, running: $dr, cpus: $cpus, memoryGb: $gb},
     gh: {installed: $gi, loggedIn: $gl, user: $user, adminOrgs: $orgs},
     current: {org: $org, machine: $machine, agent: (if $agent == "" then "claude" else $agent end),
               reviewAgent: (if $review == "" then (if $agent == "" then "claude" else $agent end) else $review end),
               dockerInRunner: ($compose == "" or ($compose | contains("compose.docker.yml"))),
               runnersRunning: $running},
     saved: {claude: ($claude or $anthropic), openai: $openai, tracker: $tracker, codexLogin: $codex},
     suggestion: {runners: $suggestion, max: $max, perRunnerCpus: $per_cpu, perRunnerGb: $per_gb}}'
}

# ---- --install-gh e --gh-login ----------------------------------------------------------------
install_tools() {
  local os arch version url tmp
  case "$(uname -s)" in Linux) os=linux ;; Darwin) os=macos ;; *) fail "sistema não suportado." ;; esac
  case "$(uname -m)" in x86_64|amd64) arch=amd64 ;; arm64|aarch64) arch=arm64 ;; *) fail "processador não suportado." ;; esac
  mkdir -p "$HOME/.local/bin"
  if command -v jq >/dev/null; then say "O jq já está instalado."; else
    curl -fsSL "https://github.com/jqlang/jq/releases/latest/download/jq-$os-$arch" -o "$HOME/.local/bin/jq" \
      && chmod 0755 "$HOME/.local/bin/jq" || fail "não consegui baixar o jq."
    say "::ok::jq instalado em ~/.local/bin/jq."
  fi
  if command -v gh >/dev/null; then say "O gh já está instalado: $(command -v gh)"; return; fi
  [ "$os" = macos ] && os=macOS
  version="$(curl -fsSL https://api.github.com/repos/cli/cli/releases/latest | jq -r .tag_name | sed 's/^v//')"
  [ -n "$version" ] || fail "não consegui descobrir a versão do gh."
  tmp="$(mktemp -d)"; mkdir -p "$HOME/.local/bin"
  if [ "$os" = linux ]; then
    url="https://github.com/cli/cli/releases/download/v$version/gh_${version}_linux_$arch.tar.gz"
    curl -fsSL "$url" | tar -xz -C "$tmp"
  else
    url="https://github.com/cli/cli/releases/download/v$version/gh_${version}_macOS_$arch.zip"
    curl -fsSL "$url" -o "$tmp/gh.zip" && (cd "$tmp" && unzip -q gh.zip)
  fi
  install -m 0755 "$(find "$tmp" -type f -path '*/bin/gh' | head -1)" "$HOME/.local/bin/gh"
  rm -rf "$tmp"
  say "::ok::gh $version instalado em ~/.local/bin/gh."
}

gh_login() {
  command -v gh >/dev/null || fail "instale o gh primeiro."
  if gh auth status >/dev/null 2>&1; then say "Já está logado no GitHub."; return; fi
  say "Abra https://github.com/login/device e digite o código abaixo."
  # O gh pede Enter antes de abrir o navegador; o código sai antes disso.
  printf '\n' | gh auth login --hostname github.com --git-protocol https --web --scopes admin:org 2>&1 \
    | sed -u 's/^! First copy your one-time code: /::code::/'
  gh auth status >/dev/null 2>&1 || fail "o login no GitHub não terminou."
  say "::ok::Logado no GitHub como $(gh api user --jq .login)."
}

# ---- logins pelo navegador, dentro da imagem dos runners --------------------------------------
# Os CLIs já estão na imagem: a pessoa não precisa instalar nada nem abrir um terminal. O comando
# roda como o usuário dos jobs, com o volume do login do Codex (o mesmo de todos os runners).
in_runner() {
  [ -z "$DRY" ] || { say "[teste] $*"; return 0; }
  command -v docker >/dev/null && docker info >/dev/null 2>&1 || fail "o Docker não está rodando."
  # A configuração do compose pede a organização e a máquina; numa máquina nova o .env ainda não as tem.
  export FACTORY_ORG FACTORY_MACHINE
  FACTORY_ORG="$(default_org)"; FACTORY_MACHINE="$(default_machine)"
  if ! docker image inspect fg-factory-runner >/dev/null 2>&1; then
    say "::step::Preparando a imagem dos runners (alguns minutos, só na primeira vez)"
    docker compose build -q runner1 2>&1 | tail -5
  fi
  if docker compose ps --services --status running 2>/dev/null | grep -qx runner1; then
    docker compose exec -T -u factory -e HOME=/home/factory -e BROWSER=true runner1 "$@"
  else
    docker compose run --rm --no-deps -T -u factory -e HOME=/home/factory -e BROWSER=true --entrypoint "" runner1 "$@"
  fi
}
# O `claude setup-token` precisa de um terminal: o `script` dá um, largo o bastante para o endereço
# e o token saírem numa linha só. A saída (com o token) vai só para quem chamou, o app.
login_claude() {
  say "::step::Conectando a conta do Claude"
  in_runner pkill -f "claude setup-token" >/dev/null 2>&1 || true   # uma tentativa anterior, esquecida aberta
  in_runner timeout 900 script -q -c "stty cols 4000 rows 50; claude setup-token" /dev/null
}
login_codex() {
  say "::step::Conectando a conta do ChatGPT ao Codex"
  in_runner pkill -f "codex login" >/dev/null 2>&1 || true
  in_runner timeout 900 codex login --device-auth || true
  in_runner codex login status >/dev/null 2>&1 || fail "a conta do ChatGPT não foi conectada (o código vale 15 minutos). Clique em Conectar de novo."
  say "::ok::Codex conectado; vale para todos os runners desta máquina."
}

# ---- perguntas do modo interativo -------------------------------------------------------------
read_line() {
  if [ -z "$DRY" ] && [ -r /dev/tty ]; then IFS= read -r "$@" < /dev/tty || true; else IFS= read -r "$@" || true; fi
}
ask() {  # ask "pergunta" "padrão" -> resposta (Enter = padrão)
  local answer=""
  printf '%s [%s]: ' "$1" "$2" >&2
  read_line answer
  printf '%s' "${answer:-$2}"
}
dialog_kind() {
  if [ -n "${FACTORY_SETUP_NO_DIALOG:-}" ] || [ -n "$DRY" ]; then echo terminal; return; fi
  if grep -qi microsoft /proc/version 2>/dev/null && command -v powershell.exe >/dev/null; then echo windows; return; fi
  if [ "$(uname -s)" = Darwin ] && command -v osascript >/dev/null; then echo mac; return; fi
  if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ] && command -v zenity >/dev/null; then echo zenity; return; fi
  echo terminal
}
# A janela do Windows (pelo WSL): o texto vai codificado no script; o valor volta pela saída padrão.
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
ask_secret() {  # ask_secret "título" "explicação" -> valor (vazio mantém o atual)
  local value="" status=0
  case "$(dialog_kind)" in
    windows) value="$(windows_dialog "$1" "$2")" || status=$? ;;
    mac) value="$(osascript -e 'on run argv' \
             -e 'display dialog (item 2 of argv) with title (item 1 of argv) default answer "" with hidden answer buttons {"Cancelar", "OK"} default button "OK" cancel button "Cancelar"' \
             -e 'return text returned of result' -e 'end run' "$1" "$2" 2>/dev/null)" || status=$? ;;
    zenity) value="$(zenity --entry --hide-text --title "$1" --text "$2" 2>/dev/null)" || status=$? ;;
    *) printf '\n%s\n%s\n(nada aparece na tela; Enter mantém o atual): ' "$1" "$2" >&2
       read_line -s value; echo >&2 ;;
  esac
  [ "$status" -eq 0 ] || return 1
  printf '%s' "${value//[[:space:]]/}"
}
agent_choice() {  # agent_choice "pergunta" atual -> claude|codex
  case "$(ask "$1: 1 = Claude Code, 2 = Codex" "$([ "$2" = codex ] && echo 2 || echo 1)")" in
    2|codex) echo codex ;; *) echo claude ;;
  esac
}

# Preenche as respostas (A_*) perguntando. Os segredos ficam vazios quando a pessoa mantém o atual.
interactive() {
  local cpus gb suggestion running current
  say "Configuração da factory nesta máquina. Enter aceita o valor entre colchetes."
  say ""
  A_ORG="$(ask "Organização do GitHub" "$(default_org)")"
  A_MACHINE="$(ask "Nome desta máquina no painel" "$(default_machine)")"
  read -r cpus gb <<< "$( [ -z "$DRY" ] && docker_resources || echo "8 16")"
  suggestion="$(suggest_runners "$cpus" "$gb")"; running="$(running_runners)"
  say "O Docker desta máquina tem $cpus núcleos e $gb GB. Sugestão: $suggestion runner(s) (cada um chega a $PER_RUNNER_CPUS núcleos e $PER_RUNNER_GB GB num build pesado)."
  A_COUNT="$(ask "Quantas tarefas ao mesmo tempo (1 a $MAX_RUNNERS; cada uma é um runner)" "$([ "$running" -gt 0 ] && echo "$running" || echo "$suggestion")")"
  current="$(env_get FACTORY_AGENT)"; current="${current:-claude}"
  A_AGENT="$(agent_choice "Quem corrige as issues" "$current")"
  current="$(env_get FACTORY_REVIEW_AGENT)"; current="${current:-$A_AGENT}"
  A_REVIEW_AGENT="$(agent_choice "Quem revisa a correção antes do PR" "$current")"
  local docker_on=n; case "$(env_get COMPOSE_FILE)" in ''|*compose.docker.yml*) docker_on=s ;; esac
  A_DOCKER="$(ask "Os testes podem subir contêineres (banco etc.)? Liga um Docker próprio por runner (s/n)" "$docker_on")"

  say ""; say "Agora as credenciais. Cada uma abre uma janela; o que você digita não aparece."
  A_CLAUDE=""; A_OPENAI=""; A_CODEX_LOGIN=""; A_TRACKER=""
  if [ "$A_AGENT" = claude ] || [ "$A_REVIEW_AGENT" = claude ]; then
    A_CLAUDE="$(ask_secret "Factory: Claude Code" "Cole o token do Claude Code. Para gerar, rode no terminal: claude setup-token (abre o navegador e imprime um token que começa com sk-ant-oat). Uma chave da API da Anthropic (sk-ant-api...) também serve. Deixe vazio para manter o que já está gravado.")" || true
  fi
  if [ "$A_AGENT" = codex ] || [ "$A_REVIEW_AGENT" = codex ]; then
    if [ "$(ask "Codex: 1 = assinatura do ChatGPT (login no navegador), 2 = chave da API da OpenAI" "$(has OPENAI_API_KEY && echo 2 || echo 1)")" = 2 ]; then
      A_CODEX_LOGIN=api
      A_OPENAI="$(ask_secret "Factory: Codex" "Cole a chave da API da OpenAI (sk-...). Deixe vazio para manter o que já está gravado.")" || true
    else
      A_CODEX_LOGIN=chatgpt
    fi
  fi
  A_TRACKER="$(ask_secret "Factory: FG Tracker" "Cole o token do FG Tracker. No painel: Conta, Tokens de acesso, Novo token, modelo FG Factory, restrito à organização. Deixe vazio para manter o que já está gravado.")" || true
  A_DELETE_OLD=ask
}

# Lê as respostas (A_*) da entrada, uma por linha: CHAVE=valor. É o que o app manda.
from_stdin() {
  A_ORG=""; A_MACHINE=""; A_COUNT=""; A_AGENT=""; A_REVIEW_AGENT=""; A_DOCKER=""
  A_CLAUDE=""; A_OPENAI=""; A_CODEX_LOGIN=""; A_TRACKER=""; A_DELETE_OLD=n
  local key value
  while IFS='=' read -r key value; do
    value="${value%$'\r'}"
    case "$key" in
      ORG) A_ORG="$value" ;; MACHINE) A_MACHINE="$value" ;; RUNNERS) A_COUNT="$value" ;;
      AGENT) A_AGENT="$value" ;; REVIEW_AGENT) A_REVIEW_AGENT="$value" ;; DOCKER) A_DOCKER="$value" ;;
      CLAUDE_TOKEN) A_CLAUDE="$value" ;; OPENAI_API_KEY) A_OPENAI="$value" ;; CODEX_LOGIN) A_CODEX_LOGIN="$value" ;;
      FG_TRACKER_TOKEN) A_TRACKER="$value" ;; DELETE_OLD_SECRETS) A_DELETE_OLD="$value" ;;
      ''|'#'*) ;;
      *) fail "resposta desconhecida: $key" ;;
    esac
  done
  A_ORG="${A_ORG:-$(default_org)}"; A_MACHINE="${A_MACHINE:-$(default_machine)}"; A_COUNT="${A_COUNT:-1}"
  A_AGENT="${A_AGENT:-claude}"; A_REVIEW_AGENT="${A_REVIEW_AGENT:-$A_AGENT}"; A_DOCKER="${A_DOCKER:-s}"
}

# ---- a configuração ----------------------------------------------------------------------------
apply() {
  local org="$A_ORG" machine count="$A_COUNT" agent="$A_AGENT" review="$A_REVIEW_AGENT" changed_tracker=false
  machine="$(clean_machine "$A_MACHINE")"
  [ -n "$machine" ] || fail "o nome da máquina não pode ficar vazio."
  case "$org" in *[!A-Za-z0-9._/-]*|'') fail "organização inválida: '$org'." ;; esac
  case "$count" in [1-9]) [ "$count" -le "$MAX_RUNNERS" ] || fail "no máximo $MAX_RUNNERS runners." ;; *) fail "o número de runners vai de 1 a $MAX_RUNNERS." ;; esac
  for a in "$agent" "$review"; do case "$a" in claude|codex) ;; *) fail "agente inválido: '$a' (claude ou codex)." ;; esac; done

  if [ -z "$DRY" ]; then
    command -v docker >/dev/null || fail "falta o Docker (Docker Desktop no Windows e no macOS)."
    docker info >/dev/null 2>&1 || fail "o Docker não está rodando. Abra o Docker Desktop e tente de novo."
    command -v gh >/dev/null || fail "falta o GitHub CLI (gh)."
    gh auth status >/dev/null 2>&1 || fail "o gh não está logado no GitHub."
  fi

  say "::step::Gravando as credenciais"
  local claude="${A_CLAUDE//[[:space:]]/}" openai="${A_OPENAI//[[:space:]]/}" tracker="${A_TRACKER//[[:space:]]/}"
  case "$claude" in
    '') ;;
    sk-ant-api*) env_set ANTHROPIC_API_KEY "$claude"; env_set CLAUDE_CODE_OAUTH_TOKEN "" ;;
    *) env_set CLAUDE_CODE_OAUTH_TOKEN "$claude"; env_set ANTHROPIC_API_KEY "" ;;
  esac
  if [ "$agent" = claude ] || [ "$review" = claude ]; then
    has CLAUDE_CODE_OAUTH_TOKEN || has ANTHROPIC_API_KEY || fail "falta a credencial do Claude Code (token de claude setup-token ou chave da API)."
  fi
  local codex_login=false
  if [ "$agent" = codex ] || [ "$review" = codex ]; then
    if [ "$A_CODEX_LOGIN" = api ]; then
      [ -z "$openai" ] || env_set OPENAI_API_KEY "$openai"
      has OPENAI_API_KEY || fail "falta a chave da API da OpenAI para o Codex."
    else
      env_set OPENAI_API_KEY ""
      codex_login=true
    fi
  fi
  if [ -n "$tracker" ]; then env_set FG_TRACKER_TOKEN "$tracker"; changed_tracker=true; fi
  has FG_TRACKER_TOKEN || fail "falta o token do FG Tracker (modelo FG Factory, restrito à organização)."
  unset claude openai tracker A_CLAUDE A_OPENAI A_TRACKER

  env_set FACTORY_ORG "$org"
  env_set FACTORY_MACHINE "$machine"
  env_set FACTORY_AGENT "$agent"
  env_set FACTORY_REVIEW_AGENT "$review"
  case "$A_DOCKER" in s|S|sim|y|Y|true|1) env_set COMPOSE_FILE "compose.yml:compose.docker.yml" ;; *) env_set COMPOSE_FILE "compose.yml" ;; esac
  # Um token só: o token separado do painel, de antes, passaria por cima do novo.
  if $changed_tracker; then env_set FG_TRACKER_FACTORY_TOKEN ""; fi
  # O token de registro vale uma hora e só serve na primeira subida: não fica gravado.
  env_set FACTORY_REGISTRATION_TOKEN ""
  say "::ok::Gravado em runner/$ENV_FILE (só o seu usuário lê)."

  local services=(); for i in $(seq 1 "$count"); do services+=("runner$i"); done
  say "::step::Registrando e subindo ${services[*]}"
  if [ -z "$DRY" ]; then
    # Recriar um runner no meio de uma tarefa a interromperia.
    local busy
    busy="$(gh api "orgs/$org/actions/runners" --paginate --jq ".runners[] | select(.busy and (.name | startswith(\"factory-$machine-\"))) | .name" 2>/dev/null || true)"
    [ -z "$busy" ] || fail "estes runners estão no meio de uma tarefa: $(echo $busy). Tente de novo quando terminarem."
    if git -C .. rev-parse --is-inside-work-tree >/dev/null 2>&1 && [ -z "$(git -C .. status --porcelain)" ]; then
      git -C .. pull -q --ff-only 2>/dev/null || true
    fi
  fi
  local token_path registration=""
  case "$org" in */*) token_path="repos/$org/actions/runners/registration-token" ;; *) token_path="orgs/$org/actions/runners/registration-token" ;; esac
  if [ -z "$DRY" ]; then
    registration="$(gh api -X POST "$token_path" --jq .token 2>/dev/null || true)"
    [ -n "$registration" ] || fail "o GitHub não deu o token de registro de runner: a conta do gh precisa ser owner de $org."
  fi
  if [ -n "$DRY" ]; then say "[teste] docker compose build"; else docker compose build -q 2>&1 | tail -5; fi
  if [ -n "$DRY" ]; then say "[teste] docker compose up -d --force-recreate ${services[*]}"; else
    FACTORY_REGISTRATION_TOKEN="$registration" docker compose up -d --force-recreate "${services[@]}" 2>&1 | tail -8
  fi
  unset registration
  for i in $(seq $((count + 1)) "$MAX_RUNNERS"); do
    if [ -z "$DRY" ] && docker compose ps --services --status running 2>/dev/null | grep -qx "runner$i"; then
      say "Parando runner$i (agora são $count)."; docker compose stop "runner$i" >/dev/null 2>&1 || true
    fi
  done
  say "::ok::Runners no ar."

  if $codex_login; then
    say "::step::Login do Codex"
    if [ -n "$DRY" ]; then say "[teste] codex login --device-auth"
    elif docker compose exec -T -u factory -e HOME=/home/factory runner1 codex login status >/dev/null 2>&1; then
      say "::ok::O Codex já está logado nesta máquina."
    else
      say "Abra o endereço abaixo, entre na sua conta do ChatGPT e digite o código. A conta precisa permitir o login por código (ChatGPT, Configurações, Segurança)."
      docker compose exec -T -u factory -e HOME=/home/factory runner1 codex login --device-auth 2>&1 \
        | sed -u 's/\x1b\[[0-9;]*m//g'
      docker compose exec -T -u factory -e HOME=/home/factory runner1 codex login status >/dev/null 2>&1 \
        || fail "o login do Codex não terminou. Rode a configuração de novo."
      say "::ok::Codex logado; vale para todos os runners desta máquina."
    fi
  fi

  if [ -z "$DRY" ]; then
    say "::step::Conferindo"
    local online=0
    for _ in $(seq 1 30); do
      online="$(gh api "orgs/$org/actions/runners" --paginate --jq "[.runners[] | select(.status == \"online\" and (.name | startswith(\"factory-$machine-\")))] | length" 2>/dev/null || echo 0)"
      [ "${online:-0}" -ge "$count" ] && break
      sleep 4
    done
    for service in "${services[@]}"; do
      if docker compose exec -T -u factory "$service" test -s /home/factory/.fg-factory/credentials 2>/dev/null; then
        say "  $service: credenciais guardadas"
      else
        say "  $service: SEM credenciais (veja: docker compose logs $service)"
      fi
    done
    [ "${online:-0}" -ge "$count" ] && say "::ok::$online de $count runner(s) online no GitHub." || say "::warn::$online de $count runner(s) online no GitHub até agora."
    delete_old_secrets "$org"
  fi

  say ""
  say "::done::Pronto. Esta máquina roda até $count tarefa(s) ao mesmo tempo: corrige com $(name_of "$agent") e revisa com $(name_of "$review"), para todos os repositórios de $org que têm o workflow da factory."
}
name_of() { [ "$1" = codex ] && echo Codex || echo "Claude Code"; }

# Segredos de repositório passam por cima das credenciais do runner; os antigos só confundem.
delete_old_secrets() {
  local org="$1" names="FACTORY_CLAUDE_TOKEN FACTORY_ANTHROPIC_API_KEY FACTORY_OPENAI_API_KEY FACTORY_FG_TRACKER_TOKEN FACTORY_FG_TRACKER_PANEL_TOKEN"
  local found="" repos repo have name
  case "$A_DELETE_OLD" in n|N|nao|não|false|0) return ;; esac
  case "$org" in */*) repos="$org" ;; *) repos="$(gh repo list "$org" --limit 200 --no-archived --json nameWithOwner --jq '.[].nameWithOwner' 2>/dev/null || true)" ;; esac
  for repo in $repos; do
    have="$(gh secret list --repo "$repo" --json name --jq '.[].name' 2>/dev/null || true)"
    for name in $names; do grep -qx "$name" <<< "$have" && found+="$repo $name"$'\n'; done
  done
  [ -n "$found" ] || return 0
  say "Segredos de repositório que passariam por cima das credenciais desta máquina:"
  printf '%s' "$found" | sed 's/^/  /'
  if [ "$A_DELETE_OLD" = ask ] && [ "$(ask "Apagar, para todos usarem as credenciais dos runners? (s/n)" s)" != s ]; then return; fi
  while read -r repo name; do
    [ -n "$repo" ] || continue
    gh secret delete "$name" --repo "$repo" >/dev/null 2>&1 && say "  apagado: $repo $name"
  done <<< "$found"
}

case "${1:-}" in
  --check) ensure_env; check ;;
  --install-tools) install_tools ;;
  --gh-login) gh_login ;;
  --login-claude) ensure_env; login_claude ;;
  --login-codex) ensure_env; login_codex ;;
  --apply) ensure_env; from_stdin; apply ;;
  '') ensure_env; interactive; apply ;;
  *) fail "opção desconhecida: $1 (veja o começo do setup.sh)" ;;
esac
