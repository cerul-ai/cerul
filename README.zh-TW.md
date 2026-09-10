<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/cerul-logo-dark.png">
    <img src="docs/assets/cerul-logo-light.png" alt="Cerul" width="280">
  </picture>
</p>

<p align="center"><strong>讓 AI 理解、記住並調用影片。</strong></p>
<p align="center">開源影片處理核心與 CLI。使用你自己的模型服務，無需 Cerul 帳號。</p>

<p align="center">
  <a href="https://cerul.ai">官網</a> ·
  <a href="docs/installation.md">安裝指南</a> ·
  <a href="docs/video-search.md">影片教學</a> ·
  <a href="https://x.com/cerul_hq">X / Twitter</a> ·
  <a href="https://discord.gg/qHDEMQB9vN">Discord</a>
</p>

<p align="center">
  <a href="https://github.com/cerul-ai/cerul/actions/workflows/ci.yml"><img src="https://github.com/cerul-ai/cerul/actions/workflows/ci.yml/badge.svg" alt="建置與測試"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="Apache-2.0 授權條款"></a>
</p>

<p align="center"><a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · 繁體中文</p>

## 用一句話搜尋你的影片

Cerul 將本機影片變成可搜尋的資料庫。描述一個畫面，查找影片中說過或出現過的文字，再把匹配片段匯出。支援普通影片資料夾和 LeRobot 資料集，使用你自己的模型 API key。

- **按含義或圖片搜尋**：輸入一句描述，或提供參考圖片。
- **查找語音和螢幕文字**：語音轉錄配合內建的本機 OCR。
- **保存有用的結果**：匯出片段，生成結構化語意標註。
- **中斷後繼續**：重複執行會重用已完成的工作。

## 快速開始

Cerul 是一個搜尋影片、匯出片段的命令列工具。安裝包已包含所需的媒體工具和 OCR 模型。

支援 **macOS Apple Silicon** 和 **Linux x86_64（Ubuntu 24.04 或更新）**。

### 1. 安裝

```sh
curl -fsSL https://cerul.ai/install.sh | sh
```

之後用 `cerul upgrade` 升級到最新版本，工作區、金鑰和標註檔案都不受影響。

### 2. 加入影片

```sh
cerul index ./demo.mp4
```

第一次使用時，依提示輸入 [Gemini API key](https://aistudio.google.com/apikey)，也可以先用 `cerul auth set` 儲存，之後無需重複輸入。模型處理會將資料傳送給 Gemini，並可能產生 API 費用。

隨時直接執行 `cerul`，即可看到已索引的影片和下一步指令。

### 3. 搜尋並儲存片段

```sh
cerul search "有人把杯子放到桌上"
cerul search "有人把杯子放到桌上" --save ./clips
```

每筆結果是一張卡片，顯示匹配度、時間區間和連結。在 iTerm2、Ghostty、Kitty 或 WezTerm 中還會顯示該瞬間的畫面截圖，用 `--no-preview` 可以關閉。裝了 [IINA](https://iina.io) 時，連結會直接跳到匹配的時間點，而不是從頭播放。

結果都有編號，`cerul open 2` 會用播放器直接從第二個片段開始播，不用離開終端。

### 日常維護

```sh
cerul remove ./demo.mp4      # 刪除索引與旁車資料，保留原影片
cerul remove --cache         # 釋放可再生的磁碟佔用
cerul completions zsh        # 產生 shell 補全指令碼
```

zsh 使用者把它放到 `fpath` 裡，例如 `cerul completions zsh > ~/.zfunc/_cerul`，並確保 `~/.zfunc` 在 `compinit` 之前加入 `fpath`。

[更多使用範例 →](docs/video-search.md)

## 標註動作與示範影片

為一般影片、第一人稱錄影或機器人示範產生動作步驟、事件、互動及狀態變化標註，無需先建立索引。

```sh
cerul annotate ./video.mp4 --semantic subtask,event,interaction,state
```

加上 `--dry-run` 可預覽處理計畫。結果儲存在 JSONL sidecar 中，執行 `cerul status ./video.mp4` 查看位置。對於 LeRobot 資料集，可先用 `cerul annotate ./dataset --semantic --only 0` 標註第一個 episode。這些是語義標註，目前不提供姿態、深度或 3D 軌跡。

[標註類型、輸出位置與 LeRobot 範例 →](docs/annotation.md)

## 讓 agent 幫你安裝

把下面這段話發給能操作終端的 agent：

```text
依照 https://github.com/cerul-ai/cerul/blob/main/docs/agent-setup.md 幫我安裝 Cerul，安全設定 Gemini API key，搜尋一部本機影片並儲存匹配片段，然後教我如何使用。
```

[Agent 安裝指南 →](docs/agent-setup.md)

## 更多用法

| 目標 | 命令 |
| --- | --- |
| 處理整個資料夾 | `cerul index ./videos` |
| 預覽操作 | `cerul --dry-run index ./videos` |
| 生成語意標註 | `cerul annotate ./videos --semantic` |
| 查看進度和結果 | `cerul status` |
| 限定一部影片搜尋 | `cerul search "打開門" --in ./demo.mp4` |

轉錄、標註和向量保存在影片旁的 sidecar 檔案中，可不呼叫模型重建搜尋索引。LeRobot 子任務回寫需要明確選擇開啟。

## 了解更多

- [文件目錄](docs/README.md)

- [安裝與排錯](docs/installation.md)
- [影片搜尋教學](docs/video-search.md)
- [LeRobot 教學](docs/lerobot-subtasks.md)
- [模型服務與設定](docs/configuration.md)
- [貢獻與開發整合](CONTRIBUTING.md)

## 授權條款

Cerul 的 Rust 程式碼採用 Apache-2.0。發行套件中的媒體工具和 OCR 模型各自遵循其授權條款，詳見[第三方聲明](THIRD_PARTY_NOTICES.md)。[Cerul 名稱與標識](docs/assets/README.md)用於識別專案，不代表對第三方產品的背書。
