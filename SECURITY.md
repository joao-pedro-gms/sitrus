# Política de Segurança

## O que é segredo no Sitrus

O Sitrus funciona autenticando-se com as credenciais da **sua sessão** do Apple Music.
Os seguintes dados são segredos e devem ser tratados como senhas:

- **Cookies da sessão** de `music.apple.com` — dão acesso completo à sua conta no
  web player.
- **`media-user-token`** — identifica sua conta nas requisições à API do player.

Regras que o projeto segue e que contribuidores devem respeitar:

- Esses dados ficam apenas em `~/.local/share/sitrus/`, em arquivo com permissão
  `0600` (leitura/escrita só pelo dono). Nunca são enviados para nenhum servidor que
  não seja da Apple.
- **Nunca** inclua cookies, `media-user-token`, e-mail da conta ou qualquer dado da
  sua conta Apple em issues, PRs, logs, fixtures de teste ou capturas de tela. Ao
  reportar bugs, sanitize os logs antes de colar.
- Fixtures de teste de `sitrus-api` devem usar dados fictícios/sanitizados.

## Reportando uma vulnerabilidade

**Não abra uma issue pública** para falhas de segurança — em especial qualquer coisa
que possa expor cookies, tokens ou dados de conta de usuários.

Reporte de forma privada por um destes canais:

1. **GitHub Security Advisories** (preferido): na aba
   [Security](https://github.com/joao-pedro-gms/sitrus/security) do repositório,
   clique em "Report a vulnerability".
2. Se o canal acima não estiver disponível, contate o mantenedor
   [@joao-pedro-gms](https://github.com/joao-pedro-gms) por mensagem privada no GitHub.

Inclua no relato, se possível: descrição do problema, passos para reproduzir, impacto
potencial e versão/commit afetado.

## O que esperar

- Confirmação de recebimento em até **7 dias**.
- Avaliação e, se confirmada, correção em branch privada, com release de correção e
  aviso no advisory público após o patch.
- Crédito ao relator no advisory, se desejado.

## Escopo

Esta política cobre o código deste repositório e os binários publicados nos releases
(AppImage e RPM). Problemas nos serviços da Apple devem ser reportados à Apple.

## Versões suportadas

| Versão | Suportada |
| --- | --- |
| 0.1.x (mais recente) | Sim |
| anteriores | Não — atualize para o release mais recente |
