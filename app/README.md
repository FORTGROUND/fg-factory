# FG Factory Setup

O app que configura os runners do FG Factory numa máquina (macOS, Windows com WSL, Linux). O passo
a passo para quem vai usar está no [README do repositório](../README.md#configurar-uma-máquina-passo-a-passo).

- É uma interface para o [`runner/setup.sh`](../runner/setup.sh): o app baixa o repositório,
  chama `setup.sh --check` para saber o que a máquina tem e `setup.sh --apply` para configurar.
  As respostas, inclusive os tokens, vão pela entrada padrão do script, nunca por argumento,
  arquivo temporário ou log.
- Os logins do Claude Code e do Codex rodam os CLIs oficiais dentro da imagem dos runners
  (`setup.sh --login-claude` e `--login-codex`): o app abre a página no navegador, mostra o código
  (Codex) ou recebe o código colado (Claude) e lê o token da saída, que não vai para o log.
- No Windows os comandos rodam dentro do WSL (`wsl.exe -e bash -lc`), onde estão o Docker, o `gh` e
  os runners.
- Feito com [egui](https://github.com/emilk/egui); o executável tem uns 6 MB.
- Os ícones em `assets/` saem de `brand/mark.svg`: `resvg -w 1024 -h 1024 ../brand/mark.svg
  assets/icon-1024.png` e o mesmo com 256; o `.ico` e o `.icns` são gerados a partir do PNG de
  1024.

```sh
cargo run --release   # abre o app
cargo test            # testes da interpretação do --check, das respostas e do log
```

O workflow `.github/workflows/setup-app.yml` monta os três executáveis; uma tag `app-v<versão>`
os publica num release.
