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
  <a href="examples/video-search.md">影片教學</a> ·
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

支援 **macOS Apple Silicon** 和 **Linux x86_64（Ubuntu 24.04 或更新）**。完整發行套件包含 Cerul、FFmpeg、ffprobe 和 OCR 模型，無需另外安裝 Python、Ollama 或 FFmpeg。

> 新 CLI 的首個完整發行套件尚待發布，舊 release 是不同的用戶端。現在試用請使用下方原始碼步驟，或讓 agent 按安裝指南操作。下面的一行安裝命令將在 `v0.0.3` 發布後可用。

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/cerul-ai/cerul/releases/download/v0.0.3/cerul-installer.sh | sh
```

<details>
<summary>發布前，從目前原始碼建置</summary>

安裝 [Rust](https://rustup.rs)、C 編譯器、Protobuf 和 pkg-config。macOS 使用 `brew install protobuf pkgconf`；Ubuntu 相依套件見[安裝指南](docs/installation.md)。

```sh
git clone https://github.com/cerul-ai/cerul.git
cd cerul
bash scripts/build-distribution.sh
export PATH="$PWD/target/bundle:$PATH"
cerul --version
```

試用尚未合併的 PR 時，切換到作者提供的分支。此步驟也會從固定原始碼編譯媒體工具，第一次建置比下載安裝包更慢。

</details>

### 1. 處理一部影片

```sh
cerul index ./demo.mp4
```

首次在交互終端執行時，Cerul 會引導輸入 [Gemini API key](https://aistudio.google.com/apikey)，驗證後私密保存在本機。輸入不會顯示。也可以自行設定 `GEMINI_API_KEY`，環境變數優先。

OCR 在本機執行；影片嵌入、轉錄和語意標註會向設定的模型服務傳送輸入，並可能產生服務商費用。無需 Cerul 帳號，也沒有遙測資料收集。

### 2. 搜尋一個畫面

```sh
cerul search "有人把杯子放到桌上"
cerul search --text "connection refused"
cerul search --image ./reference.png
```

### 3. 匯出匹配片段

```sh
cerul search "有人把杯子放到桌上" --save ./clips
```

建議先用短片嘗試。[完整教學](examples/video-search.md)包含更多例子、進度檢查和恢復方法。

## 讓 agent 幫你安裝

把下面這段話發給能操作終端的 agent：

```text
幫我安裝 https://github.com/cerul-ai/cerul 的 Cerul。先閱讀所選版本的
docs/agent-setup.md 和 docs/installation.md。優先使用經過驗證的完整發行套件；
如果新 CLI 尚無發行套件，就從目前原始碼建置，包括隨附媒體工具。驗證安裝，
引導我設定 API key，不顯示或記錄金鑰。詢問我要嘗試哪個本機影片，說明模型
資料傳輸後，處理一個短片並搜尋，然後用我的語言教我如何重複操作。
回報真實結果，包括未完成的處理步驟。
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

- [安裝與排錯](docs/installation.md)
- [影片搜尋教學](examples/video-search.md)
- [LeRobot 教學](examples/lerobot-subtasks.md)
- [模型服務與設定](docs/configuration.md)
- [貢獻與開發整合](CONTRIBUTING.md)

## 授權條款

Cerul 的 Rust 程式碼採用 Apache-2.0。發行套件中的媒體工具和 OCR 模型各自遵循其授權條款，詳見[第三方聲明](THIRD_PARTY_NOTICES.md)。[Cerul 名稱與標識](docs/assets/README.md)用於識別專案，不代表對第三方產品的背書。
