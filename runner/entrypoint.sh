#!/usr/bin/env bash
# Registra o runner na primeira subida e o mantém rodando. O registro fica no volume do
# contêiner (/home/factory/runner), então reiniciar não pede token novo.
#
# Começa como root só para separar dois usuários e larga o privilégio antes de o runner subir:
#   factory         roda o runner, os jobs e o agente. Não tem sudo.
#   factory-report  envia o sinal ao painel da factory no FG Tracker (heartbeat.sh) e é o único
#                   que lê o token do painel. O runner sobe sem esse token no ambiente.
set -euo pipefail
RUNNER_DIR=/home/factory/runner
ORG="${FACTORY_ORG:?defina FACTORY_ORG}"
NAME="${FACTORY_RUNNER_NAME:-factory-$(hostname)}"
# A máquina no painel: FACTORY_MACHINE, ou o nome do runner sem "factory-" e sem o número.
MACHINE="${FACTORY_MACHINE:-$(sed -E 's/^factory-//; s/-[0-9]+$//' <<< "$NAME")}"
PANEL_TOKEN="${FG_TRACKER_FACTORY_TOKEN:-}"
PANEL_URL="${FG_TRACKER_URL:-https://tracker-api.fortground.com.br}"
REGISTRATION_TOKEN="${FACTORY_REGISTRATION_TOKEN:-}"
unset FG_TRACKER_FACTORY_TOKEN FACTORY_REGISTRATION_TOKEN
cd "$RUNNER_DIR"

# Um comando como o usuário dos jobs, sem privilégio e sem os tokens deste script.
as_factory() {
  if [ "$(id -u)" = "0" ]; then
    setpriv --reuid factory --regid factory --init-groups env HOME=/home/factory USER=factory LOGNAME=factory "$@"
  else
    "$@"
  fi
}

# Docker próprio deste runner (opcional, compose.docker.yml): um daemon dentro do contêiner, com
# imagens, contêineres, redes e portas só dele. Dois runners na mesma máquina sobem os mesmos
# serviços nas mesmas portas sem se enxergar, e nada disso aparece no Docker da máquina.
if [ "${FACTORY_DOCKER:-}" = "true" ]; then
  if [ "$(id -u)" != "0" ]; then
    echo "Docker do runner: o contêiner não começou como root; o daemon não sobe." >&2
  else
    # cgroup v2: os processos saem da raiz para o daemon poder criar os grupos dos contêineres.
    if [ -f /sys/fs/cgroup/cgroup.controllers ]; then
      mkdir -p /sys/fs/cgroup/init
      xargs -rn1 < /sys/fs/cgroup/cgroup.procs > /sys/fs/cgroup/init/cgroup.procs 2>/dev/null || true
      sed -e 's/ / +/g' -e 's/^/+/' < /sys/fs/cgroup/cgroup.controllers > /sys/fs/cgroup/cgroup.subtree_control 2>/dev/null || true
    fi
    rm -f /var/run/docker.pid /var/run/docker.sock
    # Sem os tokens deste script no ambiente; volta sozinho se cair.
    ( while true; do
        env -u FG_TRACKER_FACTORY_TOKEN -u FACTORY_REGISTRATION_TOKEN dockerd --host=unix:///var/run/docker.sock >> /var/log/dockerd.log 2>&1 || true
        rm -f /var/run/docker.pid
        sleep 3
      done ) &
    for _ in $(seq 1 60); do
      if docker info >/dev/null 2>&1; then break; fi
      sleep 1
    done
    docker info >/dev/null 2>&1 || { echo "Docker do runner: o daemon não subiu. Veja /var/log/dockerd.log; o contêiner precisa de privileged (compose.docker.yml)." >&2; tail -20 /var/log/dockerd.log >&2 || true; }
  fi
fi

if [ ! -f .runner ]; then
  : "${REGISTRATION_TOKEN:?primeira subida: defina FACTORY_REGISTRATION_TOKEN (README)}"
  # --no-default-labels: só o rótulo factory. Com os padrões (self-hosted, Linux, X64...) este
  # runner poderia pegar jobs de outros workflows da organização, como o deploy de produção.
  as_factory ./config.sh --unattended --url "https://github.com/$ORG" --token "$REGISTRATION_TOKEN" \
    --name "$NAME" --labels factory --no-default-labels --work _work --replace
fi
unset REGISTRATION_TOKEN

# Painel da factory no FG Tracker (opcional): o sinal sai de outro usuário, que guarda o token.
if [ -n "$PANEL_TOKEN" ]; then
  if [ "$(id -u)" = "0" ]; then
    install -d -m 0700 -o factory-report -g factory-report /run/fg-factory
    (umask 077; printf 'Authorization: Bearer %s\n' "$PANEL_TOKEN" > /run/fg-factory/authorization)
    chown factory-report:factory-report /run/fg-factory/authorization
    version="$(as_factory ./bin/Runner.Listener --version 2>/dev/null | head -1 | tr -cd '0-9.' || true)"
    # O grupo `factory` só deixa este usuário medir o disco do volume de trabalho (a pasta do
    # usuário dos jobs não é aberta a outros). O arquivo do token continua só dele.
    setpriv --reuid factory-report --regid factory-report --groups factory \
      env -i PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
      /usr/local/lib/fg-factory/heartbeat.sh "$NAME" "$MACHINE" "$PANEL_URL" "$version" "$RUNNER_DIR" &
  else
    echo "Painel da factory: o contêiner não começou como root, então não há como separar o token do usuário dos jobs. O sinal do runner não será enviado." >&2
  fi
fi
unset PANEL_TOKEN

if [ "$(id -u)" = "0" ]; then
  exec setpriv --reuid factory --regid factory --init-groups \
    env -u FG_TRACKER_FACTORY_TOKEN -u FACTORY_REGISTRATION_TOKEN HOME=/home/factory USER=factory LOGNAME=factory ./run.sh
fi
exec ./run.sh
