# Contribuindo com o Sitrus

Obrigado pelo interesse! Antes de abrir um PR, leia este guia e o
[Código de Conduta](CODE_OF_CONDUCT.md).

## Pré-requisitos

- Toolchain Rust estável (a versão exata está em `rust-toolchain.toml`).
- `alsa-lib-devel` (Fedora) para o backend de áudio.
- `webkit2gtk4.1-devel` **apenas** se for compilar com `--features webview`.

## Antes de enviar: rode os gates do CI localmente

O CI roda estes mesmos comandos em todo push/PR e **falha o merge** se qualquer um
quebrar. Rode todos antes de abrir o PR:

```bash
# Formatação — deve sair sem nenhuma alteração
cargo fmt --check

# Lints — warnings são erro
cargo clippy --all-targets -- -D warnings

# Testes
cargo test

# Build de release
cargo build --release
```

Para corrigir formatação automaticamente: `cargo fmt`.

## Convenção de commits

Usamos [Conventional Commits](https://www.conventionalcommits.org/pt-br/v1.0.0/):

```
<tipo>(<escopo opcional>): descrição curta no imperativo
```

Tipos aceitos: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`, `revert`.

Exemplos:

```
feat(player): adiciona reordenação da fila por arrastar
fix(api): renova media-user-token expirado antes de cada requisição
docs(readme): corrige comando de verificação do SHA256SUMS
```

Um commit faz uma coisa só. Mensagens em português ou inglês são aceitas; seja
consistente dentro do mesmo PR.

## Pull requests

- Abra PRs pequenos e focados; descreva o **porquê**, não só o quê.
- Preencha o template de PR — ele aparece automaticamente.
- Referencie a issue relacionada (`Closes #123`) quando houver.
- Não altere versões em `Cargo.toml` nem toque em `packaging/` no mesmo PR de
  uma feature, a menos que seja o assunto do PR.
- Nunca envie cookies, `media-user-token` ou dados da sua conta Apple em código,
  testes, fixtures ou logs — veja [SECURITY.md](SECURITY.md). Use fixtures JSON
  sanitizadas para testes de `sitrus-api`.

## Reportando bugs e pedindo features

Use os templates de issue em
[Issues](https://github.com/joao-pedro-gms/sitrus/issues/new/choose): um para
relato de bug, outro para pedido de feature. Quanto mais contexto (versão, distro,
passos para reproduzir), mais rápido o diagnóstico.
