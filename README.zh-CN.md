<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/cerul-logo-dark.png">
    <img src="docs/assets/cerul-logo-light.png" alt="Cerul" width="280">
  </picture>
</p>

<p align="center"><strong>让 AI 理解、记住并调用视频。</strong></p>
<p align="center">开源视频处理核心与 CLI。使用你自己的模型服务，无需 Cerul 账号。</p>

<p align="center">
  <a href="https://cerul.ai">官网</a> ·
  <a href="docs/installation.md">安装指南</a> ·
  <a href="examples/video-search.md">视频教程</a> ·
  <a href="https://x.com/cerul_hq">X / Twitter</a> ·
  <a href="https://discord.gg/qHDEMQB9vN">Discord</a>
</p>

<p align="center">
  <a href="https://github.com/cerul-ai/cerul/actions/workflows/ci.yml"><img src="https://github.com/cerul-ai/cerul/actions/workflows/ci.yml/badge.svg" alt="构建与测试"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="Apache-2.0 许可证"></a>
</p>

<p align="center"><a href="README.md">English</a> · 简体中文 · <a href="README.zh-TW.md">繁體中文</a></p>

## 用一句话搜索你的视频

Cerul 将本地视频变成可搜索的资料库。描述一个画面，查找视频中说过或出现过的文字，再把匹配片段导出。支持普通视频文件夹和 LeRobot 数据集，使用你自己的模型 API key。

- **按含义或图片搜索**：输入一句描述，或提供参考图片。
- **查找语音和屏幕文字**：语音转录配合内置的本地 OCR。
- **保存有用的结果**：导出片段，生成结构化语义标注。
- **中断后继续**：重复运行会复用已完成的工作。

## 快速开始

支持 **macOS Apple Silicon** 和 **Linux x86_64（Ubuntu 24.04 或更新）**。完整发行包包含 Cerul、FFmpeg、ffprobe 和 OCR 模型，无需另外安装 Python、Ollama 或 FFmpeg。

> 新 CLI 的首个完整发行包尚待发布，旧 release 是不同的客户端。现在试用请使用下方源码步骤，或让 agent 按安装指南操作。下面的一行安装命令将在 `v0.0.3` 发布后可用。

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/cerul-ai/cerul/releases/download/v0.0.3/cerul-installer.sh | sh
```

<details>
<summary>发布前，从当前源码构建</summary>

安装 [Rust](https://rustup.rs)、C 编译器、Protobuf 和 pkg-config。macOS 使用 `brew install protobuf pkgconf`；Ubuntu 依赖见[安装指南](docs/installation.md)。

```sh
git clone https://github.com/cerul-ai/cerul.git
cd cerul
bash scripts/build-distribution.sh
export PATH="$PWD/target/bundle:$PATH"
cerul --version
```

试用尚未合并的 PR 时，切换到作者提供的分支。此步骤也会从固定源码编译媒体工具，第一次构建比下载安装包更慢。

</details>

### 1. 处理一个视频

```sh
cerul index ./demo.mp4
```

首次在交互终端运行时，Cerul 会引导输入 [Gemini API key](https://aistudio.google.com/apikey)，验证后私密保存在本机。输入不会显示。也可以自行设置 `GEMINI_API_KEY`，环境变量优先。

OCR 在本地运行；视频嵌入、转录和语义标注会向配置的模型服务发送输入，并可能产生服务商费用。无需 Cerul 账号，也没有遥测数据收集。

### 2. 搜索一个画面

```sh
cerul search "有人把杯子放到桌上"
cerul search --text "connection refused"
cerul search --image ./reference.png
```

### 3. 导出匹配片段

```sh
cerul search "有人把杯子放到桌上" --save ./clips
```

建议先用短视频尝试。[完整教程](examples/video-search.md)包含更多例子、进度检查和恢复方法。

## 让 agent 帮你安装

把下面这段话发给能操作终端的 agent：

```text
帮我安装 https://github.com/cerul-ai/cerul 的 Cerul。先阅读所选版本的
docs/agent-setup.md 和 docs/installation.md。优先使用经过验证的完整发行包；
如果新 CLI 尚无发行包，就从当前源码构建，包括随包媒体工具。验证安装，
引导我配置 API key，不显示或记录密钥。询问我要尝试哪个本地视频，说明模型
数据传输后，处理一个短视频并搜索，然后用我的语言教我如何重复操作。
报告真实结果，包括未完成的处理步骤。
```

[Agent 安装指南 →](docs/agent-setup.md)

## 更多用法

| 目标 | 命令 |
| --- | --- |
| 处理整个文件夹 | `cerul index ./videos` |
| 预览操作 | `cerul --dry-run index ./videos` |
| 生成语义标注 | `cerul annotate ./videos --semantic` |
| 查看进度和结果 | `cerul status` |
| 限定一个视频搜索 | `cerul search "打开门" --in ./demo.mp4` |

转录、标注和向量保存在视频旁的 sidecar 文件中，可不调用模型重建搜索索引。LeRobot 子任务回写需要显式选择开启。

## 了解更多

- [安装与排错](docs/installation.md)
- [视频搜索教程](examples/video-search.md)
- [LeRobot 教程](examples/lerobot-subtasks.md)
- [模型服务与配置](docs/configuration.md)
- [贡献与开发集成](CONTRIBUTING.md)

## 许可证

Cerul 的 Rust 代码采用 Apache-2.0。发行包中的媒体工具和 OCR 模型各自遵循其许可证，详见[第三方声明](THIRD_PARTY_NOTICES.md)。[Cerul 名称与标识](docs/assets/README.md)用于识别项目，不代表对第三方产品的背书。
