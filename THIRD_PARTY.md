# Componentes de terceiros

## FFmpeg

O vyu usa as bibliotecas do [FFmpeg](https://ffmpeg.org) para receber e decodificar o vídeo e o áudio das câmeras.

- Build usada: [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds), variante **LGPL shared** (`ffmpeg-n9.0-latest-win64-lgpl-shared-9.0`).
- Licença: **GNU Lesser General Public License v2.1 ou posterior** (LGPL-2.1+).
- O vyu faz **link dinâmico**: as DLLs (`avcodec-*.dll`, `avformat-*.dll`, `avutil-*.dll`, `swscale-*.dll`, `swresample-*.dll` etc.) ficam ao lado do `vyu.exe` e podem ser trocadas por outra build compatível.
- Código-fonte do FFmpeg: <https://ffmpeg.org/download.html> e <https://github.com/BtbN/FFmpeg-Builds>.

FFmpeg é uma marca registrada de Fabrice Bellard, criador do projeto FFmpeg.

## Crates Rust

As demais dependências (egui/eframe, cpal, keyring, winreg, serde, tracing, uuid etc.) são distribuídas sob licenças MIT e/ou Apache-2.0. A lista completa está em `Cargo.lock`.

## Fontes

A interface usa as fontes Segoe UI do próprio Windows, carregadas de `C:\Windows\Fonts` em tempo de execução (não são redistribuídas).
