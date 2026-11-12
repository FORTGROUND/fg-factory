# Runner da factory num contêiner: a mesma imagem em macOS, Linux e Windows (Docker Desktop ou
# Docker Engine). O contêiner não monta nada do host, então não enxerga ~/.aws, chaves SSH nem
# .env da máquina: é ele que cumpre a regra "sem credenciais de produção".
FROM ubuntu:24.04

ARG NODE_MAJOR=24
ARG RUST_VERSION=1.99.0
ARG PNPM_VERSION=10.34.5
ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git jq tar unzip xz-utils sudo \
      build-essential pkg-config libssl-dev cmake clang \
      libicu74 libkrb5-3 zlib1g \
 && install -d -m 0755 /etc/apt/keyrings \
 && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg -o /etc/apt/keyrings/githubcli.gpg \
 && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli.gpg] https://cli.github.com/packages stable main" > /etc/apt/sources.list.d/github-cli.list \
 && curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc \
 && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu noble stable" > /etc/apt/sources.list.d/docker.list \
 && curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash - \
 && apt-get install -y --no-install-recommends gh nodejs \
      docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin iptables \
 && npm install -g "pnpm@${PNPM_VERSION}" @anthropic-ai/claude-code \
 && rm -rf /var/lib/apt/lists/* /root/.npm

# Usuário sem privilégios e sem sudo: o agente roda como ele.
# O grupo `docker` só serve com o Docker do runner ligado (compose.docker.yml); sem ele não há
# daemon nem socket no contêiner.
RUN useradd -m -s /bin/bash factory && usermod -aG docker factory
USER factory
WORKDIR /home/factory

# Rust na versão fixada pelos repositórios (rust-toolchain.toml baixa outra se precisar).
ENV PATH=/home/factory/.cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "${RUST_VERSION}" --component rustfmt,clippy \
 && mkdir -p /home/factory/.cargo/registry

# O runner do GitHub Actions, da versão mais recente no momento do build.
RUN set -eu; \
    case "$(dpkg --print-architecture)" in amd64) pkg=linux-x64 ;; arm64) pkg=linux-arm64 ;; *) echo "arquitetura não suportada" >&2; exit 1 ;; esac; \
    version="$(curl -fsSL https://api.github.com/repos/actions/runner/releases/latest | jq -r .tag_name | sed 's/^v//')"; \
    mkdir -p runner && cd runner \
 && curl -fsSL "https://github.com/actions/runner/releases/download/v${version}/actions-runner-${pkg}-${version}.tar.gz" | tar -xz

# O entrypoint começa como root só para separar o usuário que envia o sinal ao painel da factory
# (factory-report, o único que lê o token do painel) do usuário dos jobs, e troca para `factory`
# antes de o runner subir. Os dois scripts são do root: o usuário dos jobs não os altera.
USER root
RUN useradd --system --no-create-home --shell /usr/sbin/nologin factory-report
COPY --chown=root:root --chmod=0755 heartbeat.sh /usr/local/lib/fg-factory/heartbeat.sh
COPY --chown=root:root --chmod=0755 entrypoint.sh /usr/local/lib/fg-factory/entrypoint.sh
WORKDIR /home/factory
ENTRYPOINT ["/bin/bash", "/usr/local/lib/fg-factory/entrypoint.sh"]
