#!/usr/bin/env bash
# Sinal do runner para o painel da factory no FG Tracker: a cada 30 s diz que o runner está vivo,
# se está com um job e quanto de CPU, memória e disco ele enxerga.
#
# Roda como o usuário `factory-report`, não como `factory` (o usuário dos jobs e do agente). O
# token fica num arquivo que só `factory-report` lê e entra no curl por arquivo (-H @arquivo):
# não aparece no ambiente nem nos argumentos de nenhum processo. O entrypoint é quem inicia este
# script; ele nunca falha o runner.
#
# Uso: heartbeat.sh <runner> <máquina> <url-do-fg-tracker> <versão-do-runner> <diretório-de-trabalho>
set -u
RUNNER="${1:?nome do runner}"
MACHINE="${2:?nome da máquina}"
URL="${3:-https://tracker-api.fortground.com.br}"
VERSION="${4:-}"
WORKDIR="${5:-/home/factory/runner}"
# Só para teste local: outro arquivo de cabeçalho, outro intervalo, um envio só.
HEADER_FILE="${FACTORY_HEARTBEAT_HEADER_FILE:-/run/fg-factory/authorization}"
INTERVAL="${FACTORY_HEARTBEAT_INTERVAL:-30}"
PROC="${FACTORY_HEARTBEAT_PROC:-/proc}"

# O runner cria um processo Runner.Worker para cada job e o encerra no fim: existir é estar
# ocupado. A linha de comando de todo processo é legível em /proc, também para outro usuário.
busy() {
  grep -lqa 'bin/Runner[.]Worker' "$PROC"/[0-9]*/cmdline 2>/dev/null
}

payload() {
  local load1=0 load5=0 load15=0 cpus total_kb=0 available_kb=0 disk_total=0 disk_free=0 is_busy=false
  if busy; then is_busy=true; fi
  read -r load1 load5 load15 _ < "$PROC/loadavg" 2>/dev/null || true
  cpus="$(nproc 2>/dev/null || echo 1)"
  total_kb="$(awk '/^MemTotal:/ { print $2 }' "$PROC/meminfo" 2>/dev/null)"
  available_kb="$(awk '/^MemAvailable:/ { print $2 }' "$PROC/meminfo" 2>/dev/null)"
  read -r disk_total disk_free < <(df -Pk "$WORKDIR" 2>/dev/null | awk 'NR == 2 { print $2 * 1024, $4 * 1024 }') || true
  jq -nc --arg runner "$RUNNER" --arg machine "$MACHINE" --arg version "$VERSION" \
        --arg os "$(uname -s | tr '[:upper:]' '[:lower:]')" --arg arch "$(uname -m)" --argjson busy "$is_busy" \
        --arg cpus "$cpus" --arg load1 "$load1" --arg load5 "$load5" --arg load15 "$load15" \
        --arg total_kb "${total_kb:-}" --arg available_kb "${available_kb:-}" --arg disk_total "${disk_total:-}" --arg disk_free "${disk_free:-}" '
    def number: if . == "" then null else (tonumber? // null) end;
    ($total_kb | number) as $total | ($available_kb | number) as $available
    | {runner: $runner, machine: $machine, os: $os, arch: $arch, labels: ["factory"], busy: $busy,
       runnerVersion: (if $version == "" then null else $version end),
       resources: ({cpuCount: ($cpus | number), load1: ($load1 | number), load5: ($load5 | number), load15: ($load15 | number),
                    memoryTotalBytes: (if $total then $total * 1024 else null end),
                    memoryUsedBytes: (if $total and $available then (($total - $available) * 1024 | if . < 0 then 0 else . end) else null end),
                    diskTotalBytes: ($disk_total | number), diskFreeBytes: ($disk_free | number)}
                   | with_entries(select(.value != null)))}
    | with_entries(select(.value != null))'
}

beat() {
  local code
  code="$(payload | curl -sS -o /dev/null -w '%{http_code}' --connect-timeout 5 --max-time 10 -X POST "${URL%/}/v1/factory/heartbeat" \
    -H 'content-type: application/json' -H @"$HEADER_FILE" --data-binary @- 2>/dev/null)"
  # Uma linha por mudança de situação, não uma a cada 30 s.
  if [ "$code" != "${last_code:-}" ]; then
    if [ "$code" = "200" ]; then echo "heartbeat: o painel da factory está recebendo o sinal de $RUNNER."
    else echo "heartbeat: sinal não aceito (HTTP ${code:-sem resposta}). O runner segue normalmente." >&2; fi
    last_code="$code"
  fi
}

if [ ! -r "$HEADER_FILE" ]; then echo "heartbeat: sem o arquivo do token; nada a enviar." >&2; exit 0; fi
if [ "${FACTORY_HEARTBEAT_ONCE:-}" = "1" ]; then beat; exit 0; fi
while :; do
  beat
  sleep "$INTERVAL"
done
