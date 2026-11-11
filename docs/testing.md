# Testes

## Testes automáticos

Windows:

```powershell
$env:FFMPEG_DIR = "$PWD\vendor\ffmpeg"
$env:LIBCLANG_PATH = "<pasta do libclang>"
$env:PATH = "$PWD\vendor\ffmpeg\bin;" + $env:PATH
cargo test
```

Linux (com as dependências do README instaladas):

```bash
cargo test
```

O que eles cobrem:

| Módulo | O que é verificado |
|---|---|
| `canvas` | Alças de borda/canto, mover, redimensionar, tamanho mínimo, trava 16:9, linhas-guia, manter dentro da janela, z-order |
| `config` | Salvar/carregar, arquivo ausente, arquivo corrompido vira backup, remover câmera limpa todos os layouts, operações de layout |
| `stream::url` | URL com caracteres especiais na senha (`@ : / # %`), senha mascarada nos logs |
| `stream::session` | Publica quadros, senha errada não fica tentando, reconexão com espera 1 s → 2 s, troca de qualidade reabre o stream, fechar para a thread |
| `stream::quality` / `backoff` | Histerese 540/400 px com 1 s de estabilidade; esperas 1, 2, 4, 8, 16, 30 s |
| `stream::ffmpeg` | Decodifica um arquivo H.264 + G.711 A-law gerado na hora (mesmos codecs das Tapo) pelo caminho real do FFmpeg |
| `audio` | Mudo, volume, soma, limite, fila máxima de 250 ms, reamostragem para placas que não usam 48 kHz |
| `app` | Encaixe da imagem (caber/preencher), mensagens de estado, validação do formulário, barra que some em 2 s |
| `autostart` | Windows: liga/desliga num registro de teste. Linux: cria/remove o `vyu.desktop` numa pasta temporária. Não mexe no seu início automático real |

Alguns testes tocam o sistema de verdade e ficam marcados como `#[ignore]`. Para rodá-los:

```powershell
cargo test -- --ignored   # usa o cofre de senhas real (Credential Manager / GNOME Keyring)
```

## Roteiro com as câmeras reais

O teste automático não cobre a conexão RTSP com uma câmera de verdade. Faça este roteiro uma vez com as suas Tapo:

- [ ] **Conectar:** adicione a câmera, clique em **Testar conexão** e veja a prévia. Salve: o vídeo aparece em poucos segundos.
- [ ] **Diagnóstico pelo terminal:** `cargo run --example probe -- <ip> <usuario> <senha> low` mostra resolução, fps e se há áudio.
- [ ] **Senha errada:** edite a câmera com uma senha errada. Aparece "Usuário ou senha incorretos" e o vyu não fica tentando.
- [ ] **Queda:** desligue a câmera da tomada. Aparece "Reconectando…" com o último quadro escurecido. Religue: o vídeo volta sozinho.
- [ ] **Áudio:** ligue o som de duas câmeras, ajuste volumes diferentes, teste o `M` com o mouse em cima.
- [ ] **Qualidade automática:** aumente um bloco até mais da metade da tela. Depois de ~1 s a imagem fica mais nítida (`stream1`). Diminua: volta para `stream2`.
- [ ] **Layouts:** crie "Noite", deixe só uma câmera nele, troque com `Ctrl+1` / `Ctrl+2`.
- [ ] **Reabrir:** feche e abra o vyu. Mesmo layout, mesmas posições, mesmos volumes.
- [ ] **Iniciar com o sistema:** ative em ⚙ e confira. Windows: `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v vyu`. Linux: `cat ~/.config/autostart/vyu.desktop`.
- [ ] **Logs:** veja `%APPDATA%\vyu\logs\` (Linux: `~/.config/vyu/logs/`). As URLs aparecem com a senha como `***`.

## Testando a interface sem câmeras

`$env:VYU_DEMO = "1"; cargo run` (Linux: `VYU_DEMO=1 cargo run`) abre três câmeras simuladas, com imagem animada e tom de áudio. O modo demo usa `config-demo.json`, separado da configuração real.
