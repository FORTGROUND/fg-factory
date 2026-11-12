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

### 3. Dê as credenciais à factory

São duas: a do agente de código (Claude Code, o padrão, ou Codex:
[Claude Code ou Codex](#claude-code-ou-codex)) e **um** token do FG Tracker.

O token do FG Tracker é criado no painel, em Tokens de acesso → novo token, modelo **FG Factory**:
lê telemetria, issues e releases (para a investigação) e relata ao painel da factory. Restrinja-o
a **uma organização**; sem isso o FG Tracker recusa os relatos.

Elas podem ficar em dois lugares. Escolha um:

**No runner (uma vez para todos os repositórios).** Para runners próprios em contêiner. Em
`runner/`, depois de criar o `.env` (passo 4):

```sh
bash set-credentials.sh
```

O script pergunta os dois valores sem mostrá-los e os grava no `.env` da máquina. Todo
repositório atendido por esses runners passa a funcionar só com o `factory.yml`, sem segredos.
Veja [Credenciais no runner](#credenciais-no-runner) antes de escolher.

**Em cada repositório (segredos do GitHub).** O caminho para o runner hospedado pelo GitHub, e
para quem prefere credenciais diferentes por repositório:

| Segredo | Valor |
| --- | --- |
| `FACTORY_CLAUDE_TOKEN` | O token que `claude setup-token` imprime, numa máquina logada |
| `FACTORY_ANTHROPIC_API_KEY` | Alternativa ao de cima: a chave da API da Anthropic |
| `FACTORY_FG_TRACKER_TOKEN` | O token do FG Tracker do modelo **FG Factory** |

```sh
bash scripts/set-secrets.sh minha-org/meu-repo
```

Ou à mão, em Settings → Secrets and variables → Actions. Um segredo de organização serve a
todos os repositórios de uma vez, mas em repositório privado isso exige plano pago do GitHub.

Quando os dois existem, o segredo do repositório vence o que está no runner.

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
3. **Triagem e correção.** O agente (Claude Code ou Codex) investiga com o MCP do FG Tracker (trace, logs,
   releases) e decide uma de três saídas: corrigir, ignorar ou deixar para uma pessoa.
4. **Validação.** O `validate` do repositório precisa passar.
5. **Revisão independente.** Outra sessão, sem nada do que a primeira pensou,
   lê a issue e o diff e julga pelo código. Aprovada, a revisão vai junto no PR. Reprovada, não
   há PR: a issue fica com `fg-factory:needs-human`, o comentário diz o que foi apontado e o
   diff fica nos artefatos da execução.
6. **Publicação.** Branch `factory-issue-<n>-<tentativa>`, commit e PR com `Fixes #<n>`. Por
   padrão a factory para aqui; com `merge: true` ela mesma faz o merge
   ([Merge automático](#merge-automático)).
7. **Merge.** Quando o PR entra na branch principal, o GitHub fecha a issue e o FG Tracker a
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
`fg-factory:ignored` ou `fg-factory:needs-human`, a factory espera uma pessoa responder.

### Responder à factory

Quando a factory para numa issue (pede uma decisão, a revisão reprovou, a execução falhou ou a
triagem ignorou), a resposta de uma pessoa devolve a issue a ela. Há dois jeitos, com o mesmo
efeito:

- **Comentar na issue do GitHub.** Vale o comentário de quem tem escrita no repositório
  (conferido na API do GitHub). Comentário de quem não tem não faz nada.
- **Responder no FG Tracker.** O cartão GitHub da issue, e o evento de alerta, mostram o que a
  factory concluiu e o campo "Responder à factory". A resposta vira um comentário do app do FG
  Tracker na issue.

O que acontece depois da resposta:

1. O label `fg-factory:needs-human` (ou `fg-factory:ignored`) sai e a issue passa a
   `fg-factory:working`.
2. A sessão recebe a conversa inteira: as conclusões anteriores da factory, como contexto, e as
   respostas da pessoa, como **instruções**. Ela segue a mais recente. O corpo da issue continua
   sendo dado, não instrução.
3. O limite de tentativas (`max_attempts`) recomeça: cada resposta abre uma rodada nova.
4. Se a resposta ainda não basta, a factory devolve para a pessoa dizendo o que falta decidir.

Isso depende do gatilho `issue_comment` do `templates/factory.yml`. Com `FACTORY_AUTO` diferente
de `true`, a resposta só tira o label e devolve a issue à fila (`fg-tracker:ready`), sem abrir
sessão; a correção roda no próximo disparo manual. Um comentário em issue que não está esperando
uma pessoa não dispara nada.

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

A imagem traz git, gh, jq, Node, pnpm, bun, Rust, ffmpeg, o Claude Code e o Codex. Se o seu `validate` precisa de
outra coisa (Python, Go, Java), acrescente ao `runner/Dockerfile` e reconstrua.

| Quero | Comando (em `runner/`) |
| --- | --- |
| Ver se estão rodando | `docker compose ps` |
| Ver o que um runner faz | `docker compose logs -f runner1` |
| Pausar a máquina | `docker compose stop` |
| Atualizar as ferramentas | `docker compose build --pull && docker compose up -d runner1` |
| Tirar a máquina da factory | `docker compose down -v` e remova os runners no GitHub |

Notebook que dorme sai da fila enquanto dorme; o job espera outro runner por até 24 h.

#### Docker dentro do runner (opcional)

Sem isto o runner não tem Docker: um `validate` que sobe Postgres, Redis ou outro serviço por
`docker compose` não roda. Com o arquivo `compose.docker.yml` cada runner ganha um Docker só
dele, dentro do próprio contêiner:

```sh
echo 'COMPOSE_FILE=compose.yml:compose.docker.yml' >> .env
docker compose up -d --build runner1 runner2
```

- **Portas não colidem.** Cada runner tem a própria rede. Dois runners na mesma máquina sobem o
  mesmo `docker compose` nas mesmas portas ao mesmo tempo, e cada job enxerga os seus serviços em
  `localhost`. Nada aparece no Docker da máquina nem ocupa as portas dela.
- **Mais de um repositório.** Quando a correção passa por dois serviços, o agente pode subir os
  dois (cada um do seu clone) e testá-los juntos.
- **Limpeza.** No fim de cada execução, os contêineres, redes e volumes que ela subiu são
  removidos; as imagens ficam como cache, no volume `docker<N>`.
- **Montagens.** `-v` funciona com caminhos de dentro do runner (a pasta de trabalho do job).
  Um contêiner que escreve como root na pasta montada deixa arquivos que o job não consegue
  apagar: rode-o com `--user "$(id -u):$(id -g)"`.

O preço é o isolamento. Para ter um Docker próprio, o contêiner do runner roda como
**privilegiado**, e quem controla um Docker privilegiado consegue alcançar a máquina. Sem esta
opção, "o contêiner não vê a máquina" é uma garantia; com ela, é só o que acontece enquanto
ninguém tenta sair. Ligue nas máquinas em que os testes precisam de Docker, e mantenha a regra
de sempre: nada de credencial de produção na máquina.

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

### O token

O mesmo token do FG Tracker do passo 3 (modelo **FG Factory**) já relata ao painel: não há
nada a mais para gravar. Ele precisa da permissão **FG Factory: relatar**, estar restrito a
**uma organização** (é ela que recebe os relatos) e ter sido criado por um owner ou admin dela.

Um token só de leitura continua servindo para a factory trabalhar; o painel só fica vazio. O
primeiro relato recusado aparece como aviso na execução, e os seguintes nem são tentados.

Quer manter a permissão de relatar longe da sessão que lê a issue? Crie um segundo token pelo
modelo **FG Factory (relatos)**, que não lê nada, e grave-o como `FACTORY_FG_TRACKER_PANEL_TOKEN`
no repositório (ou `FG_TRACKER_FACTORY_TOKEN` no `.env` do runner). Quando existe, é ele que
relata, e o token de leitura pode voltar a ser só de leitura.

### Execuções

Cada execução relata o começo, o início e o fim de cada passo e o resultado. Para ver também a
saída da sessão do Claude Code enquanto ela roda, ligue no `with:` do `factory.yml`:

```yaml
      report_logs: true
```

### Runners

Só para runners em contêiner: com o token no `.env` da máquina (`bash set-credentials.sh`),
cada runner envia um sinal a cada 30 segundos. Sem sinal por 2 minutos, o painel mostra o
runner como offline.

### O que é enviado

| De onde | O quê |
| --- | --- |
| Workflow | Repositório, número, título e link da issue, tentativa, link da execução, nome do runner, início e fim de cada passo, resultado (`fix`, `ignore`, `needs_human` ou `failed`), passo em que parou, links dos PRs, a frase de conclusão e qual CLI e modelo rodou cada sessão |
| Workflow, com `report_logs` | O log da sessão (`session.log`), em lotes, enquanto ela roda: o que o agente escreveu e cada ferramenta que chamou, com o comando ou o arquivo |
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
- **O token do FG Tracker não altera nada.** O modelo FG Factory lê e relata ao painel da
  factory; o pior que uma sessão enganada faz com ele é enviar um relato falso ao painel. A
  credencial do GitHub ao alcance do agente, quando existe, é de leitura de código.
- **Quer separar?** Um token só de leitura para a sessão e outro, que não lê nada, para o
  painel ([Painel no FG Tracker](#painel-no-fg-tracker)).
- **Limites por execução.** Mudanças em `.github/workflows` ou acima de 30 arquivos não são
  publicadas sozinhas. Duas tentativas por issue, depois fica para uma pessoa.
- **Use em repositórios privados** quando tiver runners próprios: o GitHub não recomenda
  runners próprios em repositórios públicos.

Limite conhecido: num runner próprio, o registro do runner fica na máquina e o agente roda
como o mesmo usuário. Trate a máquina como algo que pode ser comprometido: nada de produção nela.

### Credenciais no runner

Guardar a credencial do Claude e o token do FG Tracker no runner tira os segredos de cada
repositório. O que muda:

- Ficam num arquivo que só o usuário dos jobs lê, dentro do contêiner; não vão para o ambiente
  do runner nem para o log (o workflow mascara os valores).
- Todo job que roda naquele runner alcança o arquivo, inclusive o `setup` e o `validate` de
  qualquer repositório atendido. Com segredos por repositório isso também vale durante a
  execução da factory, mas só para o repositório que os gravou.
- Por isso: registre o runner só com o rótulo `factory` (é o padrão), use-o só em repositórios
  privados da sua organização e não deixe workflows de terceiros rodarem nele.
- Trocar ou tirar: `bash set-credentials.sh` de novo, ou apague as linhas do `.env` e suba os
  runners outra vez.

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
| `model` | padrão da conta | Modelo do Claude Code: um apelido (`opus`, `sonnet`, `haiku`, `fable`) ou o nome completo |
| `effort` | padrão do modelo | Nível de raciocínio: `low`, `medium`, `high`, `xhigh` ou `max` |
| `fast` | `false` | Modo rápido: as mesmas respostas, mais depressa, com preço maior por token |
| `tracker_url` | API do FG Tracker | Para instalações próprias |
| `review` | `true` | Revisão independente antes de publicar; `false` publica sem ela |
| `review_model`, `review_effort`, `review_fast` | os de cima | Modelo, raciocínio e modo rápido do revisor (`review_fast` é `'true'` ou `'false'`) |
| `merge` | `false` | Faz o merge do PR sozinha ([Merge automático](#merge-automático)) |
| `merge_method` | `squash` | `squash`, `merge` ou `rebase` |
| `merge_wait_minutes` | `30` | Quanto esperar pelos checks do PR (até 45) |
| `agent`, `review_agent` | vazio | `claude` ou `codex`; vazio deixa a máquina decidir ([Claude Code ou Codex](#claude-code-ou-codex)) |
| `reply_only` | `false` | Só registra a resposta de uma pessoa, sem abrir sessão (o template liga com `FACTORY_AUTO` desligado) |
| `report_logs` | `false` | Envia a saída da sessão ao painel do FG Tracker (com o segredo do painel) |

### Claude Code ou Codex

As sessões rodam no Claude Code (padrão) ou no Codex CLI. Quem decide, nesta ordem:

1. **O workflow:** `agent: codex` (ou `claude`) no `with:` do `factory.yml`, ou o campo do disparo
   manual. Vale para todas as máquinas, que então precisam ter a credencial desse CLI.
2. **A máquina:** `FACTORY_AGENT=codex` no `.env` do runner. É o jeito de ter uma máquina com a
   assinatura do ChatGPT e outra com a do Claude atendendo os mesmos repositórios.
3. Sem nenhum dos dois, `claude`.

O revisor segue a mesma ordem com `review_agent` e `FACTORY_REVIEW_AGENT`; sem eles, é o mesmo
CLI de quem corrigiu. Dá para corrigir com um e revisar com o outro.

Cada CLI tem o seu modelo, raciocínio e modo rápido, também nessa ordem (workflow, máquina,
padrão da conta):

| | No workflow | Na máquina (`.env` do runner) |
| --- | --- | --- |
| Claude Code | `model`, `effort`, `fast` | `FACTORY_CLAUDE_MODEL`, `FACTORY_CLAUDE_EFFORT`, `FACTORY_CLAUDE_FAST` |
| Codex | `codex_model`, `codex_effort`, `codex_fast` | `FACTORY_CODEX_MODEL`, `FACTORY_CODEX_EFFORT`, `FACTORY_CODEX_FAST` |
| Revisor no Claude Code | `review_model`, `review_effort`, `review_fast` | as de cima |
| Revisor no Codex | `review_codex_model`, `review_codex_effort`, `review_codex_fast` | as de cima |

- `codex_model` é o nome que `codex -m` aceita; `codex_effort`, o que o modelo aceitar (`low`,
  `medium`, `high`, `xhigh`...); `codex_fast` liga o modo rápido do Codex (`service_tier fast`),
  que consome mais.
- O modo rápido ligado na máquina vale quando o workflow não o pede; o do revisor aceita `'false'`
  para desligar.
- O Codex roda sem confirmações e sem a sandbox dele (o contêiner do runner é a sandbox), com a
  busca na web desligada e o mesmo MCP do FG Tracker.

**Credencial do Codex**, uma das duas:

- **Assinatura do ChatGPT:** `bash codex-login.sh` em `runner/` (um login por runner: `bash
  codex-login.sh runner2`). O Codex mostra um endereço e um código para você confirmar no
  navegador. O login fica num volume do runner e se renova sozinho.
- **Chave da API da OpenAI:** `bash set-credentials.sh` (guarda no runner) ou o segredo
  `FACTORY_OPENAI_API_KEY` do repositório.

O comentário da issue, o resumo da execução e o painel do FG Tracker dizem qual CLI e qual modelo
rodou cada sessão. O modelo é o que o CLI informou ao começar, não o que foi pedido.

### Modelo, raciocínio e modo rápido

As três entradas escolhem como a sessão do Claude Code roda. O modelo de `templates/factory.yml`
já traz as três no disparo manual (Actions → Factory → Run workflow), para testar uma combinação
numa issue sem editar o arquivo; nos disparos automáticos vale o que está no `with:`.

- **Raciocínio** mais alto investiga mais e gasta mais tokens e tempo. `low` serve para triagem
  barata; `high` ou acima, para causas difíceis.
- **Modo rápido** só existe em alguns modelos (hoje Opus e Fable; a lista está em
  [code.claude.com/docs/en/fast-mode](https://code.claude.com/docs/en/fast-mode)). Num modelo sem
  suporte a sessão roda na velocidade normal. Com o token da assinatura ele **não entra na cota do
  plano**: é cobrado dos créditos de uso da conta, que precisam estar ligados. Com a chave da API,
  entra na fatura pelo preço do modo rápido.
- A linha "Sessão: modelo …" no resumo da execução mostra o que foi usado.

### O revisor independente

Quem corrige tende a aprovar o próprio trabalho. Por isso a factory abre uma segunda sessão
depois da validação, que não recebe a conclusão nem o raciocínio da primeira: só a issue, o
diff e o código. Ela não tem token do FG Tracker nem do GitHub e não pode mudar arquivos (o
workflow confere o diff antes e depois). Só reprova por problema concreto, com arquivo, linha e
a situação em que dá errado; preferência de estilo vira observação no PR.

- Custa uma sessão a mais, só quando a decisão é corrigir.
- `review_model`, `review_effort` e `review_fast` escolhem a sessão do revisor separadamente.
  Vazios, ele usa os mesmos de quem corrige. Um arranjo comum: quem corrige num modelo mais
  barato e o revisor num mais forte, ou o contrário.
- Uma correção reprovada conta como tentativa. `review: false` desliga.

### Merge automático

O padrão é parar no pull request aberto, para uma pessoa olhar. Com `merge: true` no `with:` do
`factory.yml`, a factory faz o merge na branch de destino quando **tudo** isto vale:

- a validação do repositório passou;
- o revisor independente aprovou (com `review: false` não há merge automático);
- os checks do próprio PR terminaram sem falha, dentro de `merge_wait_minutes`;
- o PR não recebeu commits depois da revisão e não tem conflito.

Se qualquer item falha, nada é forçado: o PR fica aberto, a issue ganha
`fg-factory:needs-human` e um comentário diz o motivo.

- **Vários repositórios.** Todos os PRs precisam estar verdes antes do primeiro merge. Os dos
  outros repositórios entram primeiro; o deste, que fecha a issue, por último.
- **Workflows de push.** Com o GitHub App da factory, o merge dispara os workflows do
  repositório (um deploy, por exemplo). Com o token do Actions o GitHub não os dispara.
- **Onde ligar.** O merge leva uma mudança à branch principal sem ninguém olhar. Se a branch
  principal faz deploy sozinha, isso é uma mudança em produção. Comece por repositórios de baixo
  risco e com checks no PR; regras de proteção da branch continuam valendo e podem recusar o merge.

## Com uma IA

O MCP do FG Tracker ganha o prompt `integrate-fg-factory` na próxima versão: peça à sua IA para integrar o
repositório com o FG Factory e ela segue este passo a passo, preenchendo o `factory.yml` com os
comandos reais do repositório. Os segredos continuam sendo digitados por você.

## Licença

MIT. Veja [LICENSE](LICENSE).
