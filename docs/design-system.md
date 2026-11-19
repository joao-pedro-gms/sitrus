# Sitrus — sistema de design (UI nativa egui, inspirada na Apple)

Status: direção **B (imersivo/Apple TV)** escolhida em 2026-11-19; tokens e catálogo propostos.
Nada implementado; nenhum pixel renderizado ainda.
Alvo: app desktop Linux nativo (egui/eframe) — **inspiração** na linguagem visual da Apple,
não conformidade HIG nem certificação Apple; o HIG descreve iOS/iPadOS/macOS, não Linux.

## Decisões que não podem ficar para depois

O que dá para adiar é o polimento visual (ilustrações, microinterações, arte final). O que
**não** dá é o sistema: tokens, tipografia, biblioteca de componentes e estrutura de janela.
Retrofitar tokens e componentes num app egui já escrito custa rewrite de todas as telas.

## Duas restrições de licença (verificadas, não suposição)

- **SF Pro não pode ser redistribuído** — a licença da Apple limita a fonte a UI de plataformas
  Apple. O app embute **Inter** (SIL OFL 1.1), de métricas próximas, como fonte de UI. Nesta
  máquina não há Inter nem SF Pro instalados (só Noto), então a fonte entra no binário.
- **SF Symbols não pode ser usado** — ícones vêm de **Lucide** (ISC) via `egui-phosphor`/`egui-lucide`
  ou SVG próprio; desenho sempre original.

## Realidade da plataforma (Fedora 45, Wayland + KDE)

- **Vibrancy/blur real de fundo não é exposto por egui/eframe** (precisa de `org_kde_kwin_blur`
  ou equivalente do compositor). Nada de "vidro" translúcido de verdade no MVP.
- Substituto nativo e barato para o efeito Apple: desfocar o **artwork** em software (downscale
  para 32×32 → blur gaussiano → upscale) e usá-lo como fundo do hero e dos painéis. Sem compositor.
- Cantos: egui arredonda retângulos comuns; o "squircle" contínuo da Apple é aproximado
  (raio 10–14 px + antialiasing), não copiado.
- Wayland: sem posicionamento global de janela nem blur; atalhos globais exigiriam portal
  (fora do MVP; atalhos só com a janela focada).

## Tokens

Escala de espaçamento (base 4): `2, 4, 8, 12, 16, 20, 24, 32, 40, 56`.
Raios: `sm 6`, `md 10`, `lg 14`, `pill 999`.
Alturas: linha de lista `44`, item de rail `40`, card de prateleira `224` (artwork 180 + texto),
hero `320`, barra de player `88`, toolbar `52`.
Alvo de clique mínimo `24×24` (referência macOS, não os 44 do iOS).

Tipografia (Inter, escada tipo SF):

| Token | px / altura | peso | uso |
| --- | --- | --- | --- |
| Display | 44 / 52 | 700 | título do hero |
| LargeTitle | 34 / 41 | 600 | título de álbum/playlist |
| Title1 | 28 / 34 | 600 | cabeçalho de seção raiz |
| Title2 | 22 / 28 | 600 | cabeçalho interno / prateleira |
| Headline | 17 / 22 | 600 | título de faixa, botões |
| Body | 17 / 22 | 400 | texto padrão |
| Subheadline | 15 / 20 | 400 | subtítulo/artista |
| Footnote | 13 / 18 | 400 | metadados |
| Caption | 12 / 16 | 500 | badges, tempo de faixa |

Cores (papéis semânticos, nunca valores soltos na tela):

- `bg.window`, `bg.content`, `bg.rail`, `bg.elevated`, `bg.hover`, `bg.selected`
- `label.primary`, `label.secondary` (60%), `label.tertiary` (35%)
- `separator` (branco 10% / preto 8%)
- `accent` (padrão: vermelho-música `#FA243C`, configurável), `accent.pressed`, `onAccent`
- `hero.gradient.from` / `hero.gradient.to` — derivados do artwork, sempre escurecidos para
  garantir contraste do texto branco sobre o hero
- `danger`, `success`, `focusRing`

Tema escuro como padrão, claro como variante — as duas paletas saem do mesmo conjunto de papéis.
Contraste mínimo verificado: 4.5:1 para texto normal, 3:1 para texto grande (o hero é o caso crítico
e por isso tem véu escuro por cima do gradiente).

Movimento: 120 ms (hover), 180 ms (estado), 240 ms (transição de tela/prateleira);
curva `ease-out cúbica` em `sitrus-ui::anim` (egui não traz curvas prontas).

## Estrutura de janela (direção B)

Quatro zonas, sem sidebar larga:

1. **Rail de navegação** (72 px, só ícone + tooltip): Início, Biblioteca, Playlists, Busca.
2. **Conteúdo com hero**: topo com artwork 320 px, gradiente extraído do artwork e metadados;
   abaixo, prateleiras horizontais (`ShelfRow`) com snap e setas de rolagem.
3. **Player bar** (rodapé, 88 px): artwork, título/artista, transporte, seek, volume, fila.
4. **Modo lista** (alternável no topo e automático acima de ~200 itens numa seção): a mesma
   superfície vira lista densa virtualizada. O hero é o ponto fraco de B para varrer biblioteca
   grande, então o modo lista é requisito, não extra.

## Catálogo de componentes (`crates/sitrus-ui`)

Cada componente nasce com todos os estados: normal, hover, pressed, focus, disabled, selecionado.

- `NavigationRail`, `RailItem` (ícone + tooltip)
- `HeroHeader` (artwork 320 px, gradiente do artwork, título, subtítulo, botão play, ações)
- `ShelfRow` (prateleira horizontal com snap, setas, título + "Ver tudo")
- `Toolbar` (breadcrumb, alternador prateleiras/lista), `SearchField`
- `ListRow` (artwork opcional, título, subtítulo, duração, ação de hover)
- `AlbumCard` / `ArtworkTile` (artwork com cantos, placeholder de gradiente)
- `SectionHeader` (título + "Ver tudo")
- `PlayerBar`, `TransportControls`, `SeekSlider`, `VolumeSlider`, `QueuePanel`
- `Button` (primário, secundário, plain, ícone), `SegmentedControl`, `Toggle`
- `ScrollView` com fade nas bordas, `Divider`
- `EmptyState`, `SkeletonRow` (shimmer de carregamento), `ErrorBanner`, `Toast`
- `ContextMenu` (clique direito em faixa/álbum), `Modal`/`Sheet` (login, confirmações)
- `FocusRing` e ordem de tabulação; `accesskit` ligado para leitores de tela

Tela obrigatória de desenvolvimento: **galeria** (`sitrus --gallery`) renderizando todos os
componentes em todos os estados, claro e escuro. É o que permite revisar UI sem depender de login.

## Três direções de casca (decidida: B)

**A. Music.app nativo** — sidebar + lista densa + player bar fixa. Escala para biblioteca grande,
densidade informativa alta. Perde em impacto visual.

**B. Imersivo / Apple TV** — capa hero grande com gradiente do artwork, prateleiras horizontais,
player fixo. **Escolhida**: é a que carrega a linguagem visual Apple de forma mais reconhecível
sem web engine. Sua fraqueza (varrer coleções grandes) é resolvida pelo modo lista obrigatório.

**C. Compacto / mini-player** — janela pequena, fila sempre visível, cromagem mínima. Fica como
possível modo secundário depois do MVP, não agora.

## O que entra em qual milestone

- M0: tokens, tema, fonte embutida, `Button`, `RailItem`, `ListRow`, `FocusRing` + galeria.
- M1: `HeroHeader`, `ShelfRow`, `AlbumCard`, modo lista, `SearchField`, estados de erro/vazio/skeleton.
- M2: `PlayerBar`, `SeekSlider`, `VolumeSlider`, `QueuePanel`, `ContextMenu`.
- M3: ícone do app, `.desktop`, splash, capturas de tela para o README.

## Pendência de empacotamento (não é design, mas decide aqui)

Login usa WebKitGTK: `webkit2gtk4.1` já está na máquina (2.54.2), o `-devel` não. O RPM declara
`Requires: webkit2gtk4.1`; o AppImage ou embute WebKitGTK (centenas de MB) ou passa a depender do
sistema. Decidir em M3 com número medido, não por estimativa.
