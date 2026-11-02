# vyu — Design

**Data:** 2026-11-02
**Status:** aprovado

## 1. Problema e objetivo

O usuário tem câmeras Tapo Wi-Fi em casa. Hoje, para assistir no PC, abre um VLC por câmera e digita endereço, usuário e senha toda vez.

O **vyu** é um app desktop para Windows que fica aberto como monitor fixo. Ele mostra todas as câmeras ao vivo, com áudio, numa tela livre onde o usuário arruma e redimensiona os vídeos como quiser.

**Critério de sucesso:** abrir o app e ver todas as câmeras rodando na hora, no mesmo layout da última vez, sem digitar nada.

## 2. Escopo

**Dentro (v1):**
- Vídeo ao vivo via RTSP de 2 a 6 câmeras Tapo (ou qualquer câmera RTSP H.264/H.265)
- Áudio por câmera: mudo/som e volume individual; várias podem tocar ao mesmo tempo
- Tela livre: mover, redimensionar, sobrepor e alinhar com linhas-guia
- Layouts salvos com nome
- Cadastro manual de câmeras, com senha no Gerenciador de Credenciais do Windows
- Qualidade automática (`stream1`/`stream2`) conforme o tamanho do bloco
- Restaurar o estado ao abrir, iniciar com o Windows (opcional)

**Fora (v1), mas no radar:**
- Busca automática de câmeras na rede (ONVIF WS-Discovery)
- Controle PTZ, fotos, gravação, janela "sempre no topo"

## 3. Decisões tomadas

| Tema | Decisão |
|---|---|
| Uso | Monitor fixo, sempre aberto |
| Layout | Tela livre (blocos arrastáveis, redimensionáveis, sobrepostos) |
| Áudio | Botão de som + volume por câmera; várias simultâneas; todas começam mudas |
| Adicionar câmera | Formulário manual (nome, IP, usuário, senha) com "Testar conexão" |
| Estilo visual | Imersivo: fundo preto, só vídeo, barra flutuante que some sozinha |
| Stack | Rust + egui/eframe (wgpu) + FFmpeg (`ffmpeg-next`) + cpal |
| Repositório | Público em `engviniciusbs/vyu`, licença MIT |

## 4. Arquitetura

Um único binário Rust, dividido em módulos com uma responsabilidade cada:

```
src/
├─ main.rs          Entrada: logging, carrega config, abre a janela
├─ app/             VyuApp (eframe::App): estado geral, loop da UI, barra flutuante, diálogos
├─ canvas/          Geometria pura da tela livre: blocos, arrastar, redimensionar,
│                   z-order, linhas-guia (sem dependência de egui → testável)
├─ stream/          Sessão por câmera numa thread: FFmpeg RTSP → decodifica vídeo/áudio
├─ audio/           Mixer: soma as câmeras com som, aplica volume, saída via cpal
├─ config/          Modelos + leitura/gravação JSON em %APPDATA%\vyu\config.json
├─ secrets/         Senhas no Windows Credential Manager (crate keyring)
└─ autostart/       Liga/desliga "iniciar com o Windows" (registro HKCU\...\Run)
```

### Comunicação entre partes

- **UI → sessão:** canal de comandos (`SetQuality`, `Stop`).
- **Sessão → UI:** um "último quadro" compartilhado (`Arc<Mutex<Option<Frame>>>`) e um status (`Connecting`, `Live`, `Reconnecting{tentativa, em}`, `AuthFailed`, `Error(msg)`). A UI pega o quadro mais recente a cada desenho. Quadros antigos são sobrescritos e nunca acumulam, então o atraso não cresce.
- **Sessão → áudio:** cada sessão escreve amostras f32 (48 kHz, mono) num ring buffer próprio (crate `ringbuf`). O mixer lê todos, multiplica pelo volume (0 se mudo), soma e limita em [-1, 1].
- A UI pede repaint quando chega quadro novo (`egui::Context::request_repaint`).

### Pipeline de vídeo (por câmera)

1. `avformat_open_input` com `rtsp_transport=tcp`, `timeout=5s`
2. Decodificador H.264/H.265 do stream de vídeo; tenta aceleração D3D11VA e cai para software se falhar
3. `swscale` converte para RGBA. O tamanho máximo é limitado ao tamanho do stream
4. Publica o `Frame { width, height, rgba: Vec<u8>, pts }`
5. A UI envia a imagem para uma textura egui (`TextureHandle::set`) e desenha no retângulo do bloco, em modo "caber" (faixas pretas) ou "preencher" (cortar)

### Pipeline de áudio

O stream de áudio da Tapo é G.711 A-law, 8 kHz. O decodificador do FFmpeg converte com `swresample` para f32 mono 48 kHz e envia ao ring buffer. Se a câmera não tiver áudio, a sessão segue só com vídeo e o botão de som fica desabilitado.

### Qualidade automática

- URL: `rtsp://{usuario}:{senha}@{host}:{porta}/stream1` (alta) ou `/stream2` (baixa). Usuário e senha passam por percent-encoding.
- No modo `Auto`, usa `stream1` quando a altura do bloco na tela passa de 540 px físicos e volta para `stream2` abaixo de 400 px (histerese). A troca espera 1 s de estabilidade. Ao trocar, a sessão reconecta mantendo o último quadro na tela até chegar o novo.
- O usuário pode fixar `Alta` ou `Baixa` por câmera.

## 5. Dados persistidos

`%APPDATA%\vyu\config.json` (com `version` para migrações futuras):

```json
{
  "version": 1,
  "cameras": [
    { "id": "uuid", "name": "Sala", "host": "192.168.0.20", "port": 554,
      "username": "camuser", "quality": "Auto", "fit": "Contain" }
  ],
  "layouts": [
    { "name": "Padrão",
      "tiles": [ { "camera_id": "uuid", "x": 0.02, "y": 0.03, "w": 0.55, "h": 0.6,
                   "z": 0, "muted": true, "volume": 0.8 } ] }
  ],
  "active_layout": "Padrão",
  "settings": { "start_with_windows": false, "start_fullscreen": false }
}
```

- As posições dos blocos são **normalizadas (0–1)** em relação à janela. Ao redimensionar a janela, o layout acompanha.
- A senha **nunca** vai para o JSON: fica no Credential Manager com o alvo `vyu:<camera_id>`.
- Posição, tamanho e monitor da janela são restaurados pela persistência do eframe.
- A gravação é atômica (escreve `config.json.tmp` e renomeia), com debounce de 500 ms após mudanças.

## 6. Interface

**Tela livre**
- Arrastar o bloco move. Arrastar bordas ou cantos redimensiona; com `Shift`, trava em 16:9. Tamanho mínimo de 120×68 px.
- Clicar traz para a frente. Linhas-guia "grudam" o bloco a 8 px das bordas de outros blocos e da janela; com `Alt`, move sem grudar.
- Duplo clique ou ⛶ põe a câmera na janela inteira; duplo clique ou `Esc` volta.
- O nome fica discreto no canto superior esquerdo.

**Controles do bloco** (com o mouse em cima): 🔊 mudo/som, volume, ⛶, ⋯ (editar, remover, qualidade Auto/Alta/Baixa, Caber/Preencher).

**Barra flutuante** (centralizada embaixo, aparece ao mexer o mouse e some após 2 s parado): ＋ Câmera · ▦ Layouts · ⚙ Configurações.

**Diálogos**
- *Câmera:* nome, IP/host, porta (554), usuário, senha, "Testar conexão" (abre o stream por até 5 s e mostra prévia ou o erro) e Salvar. Inclui uma dica: "Crie a conta da câmera no app Tapo → Configurações da câmera → Avançado → Conta da câmera".
- *Layouts:* lista com trocar, renomear e apagar, além de "Salvar layout atual como…".
- *Configurações:* iniciar com o Windows, abrir em tela cheia.

**Estado vazio:** botão grande "Adicionar primeira câmera" com a dica da conta Tapo.

**Atalhos:** `F11` tela cheia · `Ctrl+N` nova câmera · `Ctrl+1…9` layout · `M` muta/desmuta a câmera sob o mouse · `Esc` sai do foco ou da tela cheia.

**Câmera nova** entra com o bloco centralizado (40% da largura, 16:9), em cima dos outros.

## 7. Erros e reconexão

| Situação | Comportamento |
|---|---|
| Conectando | Último quadro (ou fundo escuro) com spinner e "Conectando…" |
| Queda / sem quadros por 5 s | Status `Reconnecting`, último quadro escurecido, nova tentativa com espera de 1, 2, 4, 8, 16 e 30 s (máximo), indefinidamente |
| Senha errada (RTSP 401) | Status `AuthFailed`, **sem** retentar em loop, mensagem "Usuário ou senha incorretos" + botão "Editar câmera" |
| Host inalcançável | Tratado como queda (retenta com espera); a mensagem mostra "Câmera não encontrada em {host}" |
| Sem dispositivo de áudio | App funciona sem som; ícones de som desabilitados com aviso |
| Config corrompida | Renomeia para `config.json.bak-<timestamp>`, inicia vazio e avisa |
| Credencial ausente | Bloco mostra "Senha não encontrada" + "Editar câmera" |

Os logs (crate `tracing`) vão para `%APPDATA%\vyu\logs\vyu.log`, com rotação diária e as senhas sempre mascaradas nas URLs.

## 8. Testes

- **Unitários:**
  - `canvas`: hit-test, mover, redimensionar por cada borda, tamanho mínimo, trava 16:9, snap, z-order, normalização
  - `config`: serialização, valores padrão, config corrompida
  - URL RTSP com caracteres especiais e mascaramento de senha
  - Mixer: volume, mudo, soma e clipping
  - Escolha de qualidade com histerese
  - Política de espera da reconexão
- **Sessão com fonte falsa:** a lógica de estados e reconexão fica atrás de um trait `Source`, testável sem câmera.
- **Manual:** checklist em `docs/testing.md` contra as câmeras reais (conectar, derrubar o Wi-Fi, senha errada, áudio, troca de qualidade).

## 9. Build e distribuição

- Toolchain: Rust stable MSVC. Dependências de build: LLVM/libclang (bindgen do `ffmpeg-sys-next`) e FFmpeg *shared* LGPL (build BtbN), baixado por `scripts/setup-ffmpeg.ps1` para `vendor/ffmpeg`.
- `cargo build --release` gera `vyu.exe`. O script `scripts/package.ps1` junta o exe e as DLLs do FFmpeg em `dist/vyu/` e gera o `.zip`.
- O FFmpeg LGPL com link dinâmico é compatível com a licença MIT do projeto; o aviso fica no `README` e em `THIRD_PARTY.md`.
- Subsystem `windows` (sem console) em release.
