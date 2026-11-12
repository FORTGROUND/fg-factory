<p><img src="brand/lockup-horizontal-dark.svg" alt="FG Factory" height="48"></p>

# FG Factory

O [FG Tracker](https://tracker.fortground.com.br) encontra o problema e abre a issue no GitHub.
O FG Factory pega essa issue e tenta resolver: investiga com os dados do FG Tracker, corrige,
valida e abre o pull request. O que não dá para resolver sozinho fica marcado para uma pessoa.

Tudo roda no **seu** GitHub Actions, com a **sua** credencial do Claude Code, em um runner do
GitHub ou em máquinas suas. O FG Tracker não executa código nem lê o seu repositório para isso.

```
FG Tracker detecta  →  issue no GitHub (label fg-tracker:ready)
                    →  o workflow do seu repositório chama o fix-issue.yml daqui
                    →  um runner faz a triagem, corrige, valida e abre o PR "Fixes #N"
merge do PR         →  o GitHub fecha a issue
                    →  o FG Tracker marca como resolvida, com o PR, quem fechou e quando
```

## O que você precisa

- Um projeto no FG Tracker **conectado ao GitHub** (Configurações → Integrações → GitHub). É
  isso que faz as issues e os alertas virarem issues no repositório.
- [Claude Code](https://code.claude.com): uma assinatura (token de `claude setup-token`) ou uma
  chave da API da Anthropic.
- Permissão de administrador no repositório, para gravar segredos e o workflow.

## Passo a passo

### 1. Conecte o projeto ao GitHub no FG Tracker

No painel, Configurações → Integrações → GitHub → Conectar. Confira que uma issue nova aparece
no repositório com o label `fg-tracker:ready`.

### 2. Copie o workflow para o repositório

Copie [`templates/factory.yml`](templates/factory.yml) para `.github/workflows/factory.yml` do
seu repositório e preencha dois comandos:

| Campo | Para quê | Exemplo |
| --- | --- | --- |
| `setup` | Prepara o repositório para validar | `npm ci` |
| `validate` | Precisa passar para a correção ser publicada | `npm test` |

Sem `validate`, a correção é publicada sem validação do workflow. Preencha.

### 3. Grave os segredos do repositório

| Segredo | Valor |
| --- | --- |
| `FACTORY_CLAUDE_TOKEN` | O token que `claude setup-token` imprime, numa máquina logada |
| `FACTORY_ANTHROPIC_API_KEY` | Alternativa ao de cima: a chave da API da Anthropic |
| `FACTORY_FG_TRACKER_TOKEN` | Um token de acesso do FG Tracker **só de leitura** (Telemetria, Issues, Releases, Alertas: ler) |

Pelo script, que lê os valores sem mostrar na tela:

```sh
bash scripts/set-secrets.sh minha-org/meu-repo
```

Ou à mão, em Settings → Secrets and variables → Actions do repositório.

O token do FG Tracker é criado no painel, em Tokens de acesso. Não use um token com escrita:
o agente lê texto que vem de tráfego externo.

### 4. Escolha onde roda

**Runner hospedado pelo GitHub.** Não tem máquina para cuidar. No `factory.yml`, descomente:

```yaml
runs_on: '"ubuntu-latest"'
```

O `setup` precisa instalar tudo que o `validate` usa (o Claude Code é instalado sozinho). Os
minutos entram na cota do Actions da sua conta.

**Runners próprios.** Notebooks, desktops ou uma VPS que ficam ligados. É o padrão do template
(`runs_on` comentado), e não gasta minutos do Actions. Veja [Runners próprios](#runners-próprios).

### 5. Teste em uma issue e ligue o automático

1. Actions → Factory → Run workflow, com o número de uma issue aberta pelo FG Tracker.
2. Leia o comentário que a factory deixa na issue e, se houver, o PR.
3. Quando estiver confiante, crie a variável do repositório `FACTORY_AUTO` com o valor `true`
   (Settings → Secrets and variables → Actions → Variables). A partir daí toda issue aberta
   pelo FG Tracker dispara uma tentativa. Para pausar, apague a variável.

## O que acontece em uma issue

1. **Checagem da máquina.** O job para se o runner tiver credenciais de produção.
2. **A issue cabe à factory?** Precisa estar aberta, ter sido criada por um autor permitido
   (padrão: `fg-tracker[bot]`) e não ter passado do limite de tentativas (padrão: 2). No
   disparo manual o autor não é conferido.
3. **Triagem e correção.** O Claude Code investiga com o MCP do FG Tracker (trace, logs,
   releases) e decide uma de três saídas: corrigir, ignorar ou deixar para uma pessoa.
4. **Validação.** O `validate` do repositório precisa passar.
5. **Publicação.** Branch `factory-issue-<n>-<tentativa>`, commit e PR com `Fixes #<n>`.
6. **Merge.** Quando o PR entra na branch principal, o GitHub fecha a issue e o FG Tracker a
   marca como resolvida.

### Labels

Os estados de uma issue, um por vez:

| Label | Quem põe | Significa |
| --- | --- | --- |
| `fg-tracker:ready` | O FG Tracker, ao abrir ou reabrir | Veio da telemetria e ninguém pegou ainda |
| `fg-factory:working` | A factory, ao começar | Um runner está nela agora |
| `fg-factory:pr-open` | A factory, ao publicar | Há uma correção proposta |
| `fg-factory:ignored` | A factory, na triagem | Não pede mudança de código (ruído, comportamento esperado) |
| `fg-factory:needs-human` | A factory, na triagem ou em falha | Fica para uma pessoa |

Uma issue ignorada **não é fechada**: fechar resolveria a issue no FG Tracker. Enquanto tiver
`fg-factory:ignored` ou `fg-factory:needs-human`, a factory não volta a ela; para tentar de
novo, remova o label e dispare.

### O comentário é o histórico

Cada tentativa deixa um comentário na issue, com:

- a decisão e a conclusão em uma frase;
- confiança na causa, onde ela está (`arquivo:linha`), tentativa e duração;
- o que foi verificado e as evidências (ids de trace, trechos de log, releases, números);
- a causa, o que mudou, os riscos e a validação;
- o PR de cada repositório, com commit e tamanho da mudança;
- o passo a passo da investigação, com as hipóteses descartadas.

Se a execução falha no meio, o comentário diz em que passo parou e traz a análise já feita.

## Runners próprios

**Regra: uma máquina da factory não tem acesso à produção.** O agente executa código e lê texto
que vem de fora. O que protege a produção é ele não ter o que usar: sem credenciais de nuvem,
sem URL de banco, sem chaves SSH, sem `.env` de produção. O primeiro passo de todo job confere
isso e recusa a máquina se achar algo.

### Em contêiner (macOS, Linux, Windows com WSL)

O jeito recomendado: o contêiner não monta nada da máquina, então dá para usar um notebook de
uso misto sem tirar as suas credenciais dele.

| Sistema | O que instalar antes |
| --- | --- |
| macOS | Docker Desktop |
| Linux | Docker Engine com o plugin Compose |
| Windows | Docker Desktop com a integração WSL; rode os comandos dentro do WSL |

1. Clone este repositório na máquina e entre em `runner/`.
2. Gere o token de registro (vale 1 hora). Em uma organização:
   ```sh
   gh api -X POST orgs/minha-org/actions/runners/registration-token --jq .token
   ```
   Em um repositório de conta pessoal: `repos/dono/repo/actions/runners/registration-token`.
3. Crie o `.env` a partir do exemplo e preencha a organização, o nome da máquina e o token:
   ```sh
   cp .env.example .env
   ```
4. Suba um serviço por issue simultânea:
   ```sh
   docker compose up -d runner1            # uma issue por vez
   docker compose up -d runner1 runner2    # duas (há até runner4)
   ```
5. Confira em Settings → Actions → Runners: `factory-<máquina>-1` aparece como *Idle*, só com
   o rótulo `factory`. Apague o token do `.env`; o registro fica no volume do contêiner.

A imagem traz git, gh, jq, Node, pnpm, Rust e o Claude Code. Se o seu `validate` precisa de
outra coisa (Python, Go, Java), acrescente ao `runner/Dockerfile` e reconstrua.

| Quero | Comando (em `runner/`) |
| --- | --- |
| Ver se estão rodando | `docker compose ps` |
| Ver o que um runner faz | `docker compose logs -f runner1` |
| Pausar a máquina | `docker compose stop` |
| Atualizar as ferramentas | `docker compose build --pull && docker compose up -d runner1` |
| Tirar a máquina da factory | `docker compose down -v` e remova os runners no GitHub |

Notebook que dorme sai da fila enquanto dorme; o job espera outro runner por até 24 h.

### Direto na máquina (macOS e Linux)

Para quem não quer Docker. Aqui o que isola é um usuário do sistema criado só para isso: não
pode ser a sua conta do dia a dia.

1. Crie o usuário (`sudo useradd -m -s /bin/bash factory` no Linux) e entre nele.
2. Instale nesse usuário: `git`, `gh`, `jq`, `curl`, Node 22+, o Claude Code e o que o seu
   `validate` precisa. Não faça login em nuvem nem no `gh` com esse usuário.
3. Rode o script com o token de registro:
   ```sh
   bash scripts/setup-runner.sh <token> minha-org 2     # 2 = issues simultâneas
   ```

### Concorrência

- Um runner executa um job por vez: quantos você sobe é quantas issues a máquina faz ao mesmo
  tempo.
- Uma issue nunca roda em dois lugares; a fila de jobs do GitHub entrega cada uma a um runner só.
- Os runners são registrados **só** com o rótulo `factory` (`--no-default-labels`), para não
  pegarem outros workflows seus, como um deploy.

## Vários repositórios (opcional)

Em microsserviços o erro aparece num serviço e nasce em outro. Para a factory consultar e
alterar outros repositórios da organização, crie um GitHub App seu:

1. Settings da organização → Developer settings → GitHub Apps → New GitHub App. Sem webhook.
   Permissões de repositório: **Contents: Read and write**, **Pull requests: Read and write**,
   **Metadata: Read**. Instale na organização, nos repositórios que a factory pode tocar.
2. Gere uma chave privada do app e grave em cada repositório participante:
   ```sh
   bash scripts/set-secrets.sh minha-org/meu-repo <app-id> <chave.pem>
   ```

Com isso o agente lista e clona outros repositórios (`factory-repo --list`, `factory-repo <nome>`)
com uma credencial **só de leitura de código**. Cada repositório alterado vira um PR próprio: o
primeiro fecha a issue no merge, os demais dizem de qual correção fazem parte. A credencial que
publica é criada depois da validação e o agente nunca a vê.

Sem o app, a factory fica limitada ao repositório da issue. Se a correção depender de outro, a
issue vai para `fg-factory:needs-human` dizendo qual.

## Segurança

- **O texto da issue é dado, não instrução.** O corpo traz mensagens de erro geradas por
  tráfego externo; o agente é instruído a nunca obedecer comandos que apareçam ali.
- **O agente não publica.** O checkout não guarda credencial. Quem faz commit, push e PR é o
  workflow, depois da validação.
- **Leitura só.** O token do FG Tracker é de leitura; a credencial do GitHub ao alcance do
  agente, quando existe, é de leitura de código.
- **Limites por execução.** Mudanças em `.github/workflows` ou acima de 30 arquivos não são
  publicadas sozinhas. Duas tentativas por issue, depois fica para uma pessoa.
- **Use em repositórios privados** quando tiver runners próprios: o GitHub não recomenda
  runners próprios em repositórios públicos.

Limite conhecido: num runner próprio, o registro do runner fica na máquina e o agente roda
como o mesmo usuário. Trate a máquina como algo que pode ser comprometido: nada de produção nela.

## Ajustes

Entradas do workflow, todas opcionais, no `with:` do seu `factory.yml`:

| Entrada | Padrão | Para quê |
| --- | --- | --- |
| `runs_on` | `'["factory"]'` | Onde roda, em JSON |
| `setup`, `validate` | vazio | Comandos do repositório |
| `allowed_authors` | `fg-tracker[bot]` | Quem pode ter aberto a issue no disparo automático |
| `max_attempts` | `2` | Tentativas por issue |
| `max_changed_files` | `30` | Acima disso a correção não é publicada |
| `timeout_minutes` | `90` | Tempo máximo do job |
| `model` | padrão da conta | Modelo do Claude Code |
| `tracker_url` | API do FG Tracker | Para instalações próprias |

## Com uma IA

O MCP do FG Tracker ganha o prompt `integrate-fg-factory` na próxima versão: peça à sua IA para integrar o
repositório com o FG Factory e ela segue este passo a passo, preenchendo o `factory.yml` com os
comandos reais do repositório. Os segredos continuam sendo digitados por você.

## Licença

MIT. Veja [LICENSE](LICENSE).
