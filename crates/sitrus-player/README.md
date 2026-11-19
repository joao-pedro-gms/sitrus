# sitrus-player

Backend de reprodução de áudio nativa do Sitrus para Linux.

## Decisão técnica

A saída de áudio usa **rodio 0.23 + cpal/ALSA**. A decodificação usa **Symphonia 0.6.1** diretamente para WAV, MP3, FLAC, AAC/ADTS e fMP4/MP4. O caminho evita GStreamer e seus headers, indisponíveis no ambiente de desenvolvimento do Sitrus; ALSA está disponível via `pkg-config`.

No Symphonia 0.6.1, não existe uma feature separada chamada `adts`: habilitar `aac` também registra o leitor ADTS. As features `aac` e `isomp4` são ativadas junto com `mp3`, `flac`, `wav` e `pcm`. Rodio fica responsável pela saída; a conversão Symphonia → `SamplesBuffer` mantém a decodificação alinhada com a versão direta do workspace.

A API `Player` envia comandos por `std::sync::mpsc` a uma thread dedicada. A leitura de arquivo, as requisições HTTP, a decodificação e o controle de saída acontecem nessa thread, não na chamadora da UI. O consumidor pode consultar `Player::events()` para receber `StateChanged`, `Position`, `Duration`, `Ended` e `Error`.

## HLS implementado

A feature `hls` está ligada por padrão e habilita `reqwest` com TLS Rustls e cliente blocking. O worker baixa o manifesto, seleciona a variante com maior `BANDWIDTH`, baixa os segmentos e os concatena antes da decodificação:

- segmentos AAC em ADTS (`.aac` ou `.adts`); ou
- segmentos fMP4 com `#EXT-X-MAP` contendo o segmento de inicialização.

O suporte é limitado a playlists finitas/snapshots: não há atualização de playlist ao vivo, descriptografia HLS, `BYTERANGE`, masters aninhados ou troca de segmento de inicialização durante a lista. A fonte concatenada e o PCM decodificado têm limite de 512 MiB. A implementação não inclui autenticação de conta Apple Music; URLs HTTP públicas ou assinadas precisam ser fornecidas pela camada chamadora.

## Uso

```rust,no_run
use std::time::Duration;
use sitrus_player::{Player, PlayerEvent};

let player = Player::new();
player.load("/caminho/para/faixa.wav")?;
player.set_volume(0.7)?;
player.play()?;
player.seek(Duration::from_secs(12))?;

while let Ok(event) = player.events().recv() {
    match event {
        PlayerEvent::StateChanged(state) => println!("Estado: {state:?}"),
        PlayerEvent::Position(position) => println!("Posição: {position:?}"),
        PlayerEvent::Duration(duration) => println!("Duração: {duration:?}"),
        PlayerEvent::Ended => break,
        PlayerEvent::Error(error) => eprintln!("Erro: {error}"),
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`load` é assíncrono em relação à UI: seu retorno confirma apenas que o comando entrou no canal. O sucesso do carregamento chega por `Duration`; falhas chegam por `Error`. Se não houver dispositivo de saída, a crate continua compilando e o worker publica o erro do dispositivo em vez de falhar no `Player::new()`.

## Testes

Os testes de playlist usam fixtures locais; a geração e decodificação de WAV sintético não dependem de hardware. O teste que abre a saída ALSA é `#[ignore]`, pois CI e hosts de build podem não ter um dispositivo acessível. Para executá-lo explicitamente:

```sh
cargo test -p sitrus-player player::tests::plays_a_runtime_generated_wav_on_an_audio_device -- --ignored
```
