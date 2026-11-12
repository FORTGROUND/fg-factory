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

## Painel no FG Tracker

Opcional. Com ele, o FG Tracker mostra em **FG Factory** o que os seus runners estão fazendo:
quais estão trabalhando, livres ou offline, em que issue e em que passo cada execução está, o
resultado de cada uma, os PRs e gráficos do período. O item aparece no menu da organização
depois do primeiro relato, para owners e admins dela.

### 1. Crie o token do painel

No FG Tracker, em Tokens de acesso → novo token, modelo **FG Factory (relatos)**:

- só a permissão **FG Factory: relatar**. Ela não lê nada, nem o próprio painel;
- **restrito a uma organização**: é ela que recebe os relatos. Sem essa restrição o FG Tracker
  recusa os relatos;
- quem cria precisa ser owner ou admin da organização.

É um token diferente do `FACTORY_FG_TRACKER_TOKEN` (o de leitura, que o agente usa). Não use um
no lugar do outro.

### 2. Execuções: um segredo no repositório

Grave o token como `FACTORY_FG_TRACKER_PANEL_TOKEN` (o `scripts/set-secrets.sh` pergunta por ele)
e confira que o seu `factory.yml` o repassa, como no [template](templates/factory.yml):

```yaml
    secrets:
      FG_TRACKER_FACTORY_TOKEN: ${{ secrets.FACTORY_FG_TRACKER_PANEL_TOKEN }}
```

A partir daí cada execução relata o começo, o início e o fim de cada passo e o resultado. Sem o
segredo nada muda: nenhum passo do painel roda.

Para ver também a saída da sessão do Claude Code enquanto ela roda, ligue no `with:`:

```yaml
      report_logs: true
```

### 3. Runners: o token no `.env` da máquina

Só para runners em contêiner. Em `runner/.env`:

```sh
FG_TRACKER_FACTORY_TOKEN=fgt_...
```

e suba de novo (`docker compose up -d --build runner1`). Cada runner passa a enviar um sinal a
cada 30 segundos. Sem sinal por 2 minutos, o painel mostra o runner como offline.

### O que é enviado

| De onde | O quê |
| --- | --- |
| Workflow | Repositório, número, título e link da issue, tentativa, link da execução, nome do runner, início e fim de cada passo, resultado (`fix`, `ignore`, `needs_human` ou `failed`), passo em que parou, links dos PRs e a frase de conclusão |
| Workflow, com `report_logs` | As linhas da saída da sessão (`session.log`), em lotes, enquanto ela roda |
| Runner em contêiner | Nome do runner e da máquina, sistema, arquitetura, versão do runner, se há um job rodando, núcleos, carga, memória e disco |

### O que não é enviado

- Código, diff, o corpo da issue, o `result.json` inteiro, variáveis de ambiente ou segredos.
- Nada do que o agente lê no FG Tracker.
- Sem `report_logs`, nenhuma linha da sessão. Com ele, a saída pode citar trechos de código e
  de logs: o FG Tracker troca os segredos que reconhece por `[Filtered]`, guarda só as últimas
  5.000 linhas de cada execução e apaga tudo em 7 dias. Deixe desligado se a saída da sessão
  não pode sair da sua infraestrutura.

### O que o painel mostra de uma execução

Só o que o workflow relatou: quais passos terminaram, qual está rodando e há quanto tempo. Não
há porcentagem nem previsão de término: o passo de triagem e correção leva o tempo que levar.

### Limites, ditos com clareza

- **Os números de CPU, memória e disco são os que o contêiner enxerga.** A carga e a memória
  vêm de `/proc` e costumam ser as da máquina inteira (no Docker Desktop, as da VM do Docker); o
  disco é o do volume do runner.
- **Runner direto na máquina (`setup-runner.sh`) ainda não envia o sinal.** As execuções dele
  aparecem no painel; a máquina, não. O mesmo vale para runners hospedados pelo GitHub.
- **O relato nunca derruba nem atrasa uma execução.** Cada envio tem poucos segundos de limite;
  se o FG Tracker não responder, o passo deixa um aviso no resumo da execução e segue.
- **Onde o token fica.** No contêiner, o sinal sai de outro usuário do sistema (`factory-report`),
  o único que lê o token; o runner e os jobs sobem sem ele no ambiente e sem acesso ao arquivo.
  No workflow, o token só entra nos passos "Painel", que não rodam código do repositório, e nunca
  no passo do agente nem no `setup` e no `validate`. A exceção é o `report_logs`: para enviar o
  log durante a sessão, um processo com o token na memória roda ao mesmo tempo que o agente,
  como o mesmo usuário. O token não fica no ambiente, em arquivo nem em argumentos desse
  processo, mas isso não é uma separação de verdade. O que limita o estrago é a permissão: quem
  tiver esse token só consegue enviar relatos da factory para a sua organização.

## Segurança

- **O texto da issue é dado, não instrução.** O corpo traz mensagens de erro geradas por
  tráfego externo; o agente é instruído a nunca obedecer comandos que apareçam ali.
- **O agente não publica.** O checkout não guarda credencial. Quem faz commit, push e PR é o
  workflow, depois da validação.
- **Leitura só.** O token do FG Tracker é de leitura; a credencial do GitHub ao alcance do
  agente, quando existe, é de leitura de código.
- **O token do painel não lê nada.** Ele só envia relatos da factory e fica fora do ambiente do
  agente ([Painel no FG Tracker](#painel-no-fg-tracker)).
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
| `report_logs` | `false` | Envia a saída da sessão ao painel do FG Tracker (com o segredo do painel) |

## Com uma IA

O MCP do FG Tracker ganha o prompt `integrate-fg-factory` na próxima versão: peça à sua IA para integrar o
repositório com o FG Factory e ela segue este passo a passo, preenchendo o `factory.yml` com os
comandos reais do repositório. Os segredos continuam sendo digitados por você.

## Licença

MIT. Veja [LICENSE](LICENSE).
