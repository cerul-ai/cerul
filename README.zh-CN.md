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
  <a href="docs/video-search.md">快速上手</a> ·
  <a href="docs/agent-setup.md">让 agent 安装</a> ·
  <a href="docs/configuration.md">配置</a> ·
  <a href="https://x.com/cerul_hq">X / Twitter</a> ·
  <a href="https://discord.gg/qHDEMQB9vN">Discord</a>
</p>

<p align="center">
  <a href="https://github.com/cerul-ai/cerul/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/cerul-ai/cerul/ci.yml?branch=main&style=flat-square&logo=github&logoColor=white&label=build" alt="构建与测试"></a>
  <a href="https://github.com/cerul-ai/cerul/releases"><img src="https://img.shields.io/github/v/release/cerul-ai/cerul?style=flat-square&color=2160FA&label=release" alt="最新版本"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-1D2229?style=flat-square" alt="支持平台：macOS 与 Linux">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-1D2229?style=flat-square" alt="Apache-2.0 许可证"></a>
  <a href="https://discord.gg/qHDEMQB9vN"><img src="https://img.shields.io/badge/Discord-join-5865F2?style=flat-square&logo=discord&logoColor=white" alt="加入 Discord"></a>
</p>

<p align="center"><a href="README.md">English</a> · 简体中文 · <a href="README.zh-TW.md">繁體中文</a></p>

## 用一句话搜索你的视频

Cerul 将本地视频变成可搜索的资料库。描述一个画面，查找视频中说过或出现过的文字，再把匹配片段导出。支持普通视频文件夹和 LeRobot 数据集，使用你自己的模型 API key。

- **按含义或图片搜索**：输入一句描述，或提供参考图片。
- **查找语音和屏幕文字**：语音转录配合内置的本地 OCR。
- **保存有用的结果**：导出片段，生成结构化语义标注。
- **中断后继续**：重复运行会复用已完成的工作。

## 快速开始

Cerul 是一个搜索视频、导出片段的命令行工具。安装包已包含所需的媒体工具和 OCR 模型。

支持 **macOS Apple Silicon** 和 **Linux x86_64（Ubuntu 24.04 或更新）**。

### 1. 安装

```sh
curl -fsSL https://cerul.ai/install.sh | sh
```

之后用 `cerul upgrade` 升级到最新版本，工作区、密钥和标注文件都不受影响。

### 2. 添加视频

```sh
cerul index ./demo.mp4
```

第一次使用时，按提示输入 [Gemini API key](https://aistudio.google.com/apikey)，也可以提前用 `cerul auth set` 保存，之后无需重复输入。模型处理会将数据发送给 Gemini，并可能产生 API 费用。

随时直接运行 `cerul`，可以看到已索引的视频和下一步命令。

### 3. 搜索并保存片段

```sh
cerul search "有人把杯子放到桌上"
cerul search "有人把杯子放到桌上" --save ./clips
```

每条结果是一张卡片，显示匹配度、时间区间和链接。在 iTerm2、Ghostty、Kitty 或 WezTerm 中还会显示该瞬间的画面截图，用 `--no-preview` 可以关闭。装了 [IINA](https://iina.io) 时，链接会直接跳到匹配的时间点，而不是从头播放。

结果都有编号，`cerul open 2` 会用播放器直接从第二个片段开始播，不用离开终端。

<details>
<summary><strong>清理数据与 shell 补全</strong></summary>

```sh
cerul remove ./demo.mp4      # 删除索引与旁车数据，保留原视频
cerul remove --cache         # 释放可再生的磁盘占用
cerul completions zsh        # 生成 shell 补全脚本
```

zsh 用户把它放到 `fpath` 里，例如 `cerul completions zsh > ~/.zfunc/_cerul`，并确保 `~/.zfunc` 在 `compinit` 之前加入 `fpath`。bash 用户可以用
`cerul completions bash > /usr/local/etc/bash_completion.d/cerul`。

</details>

[更多使用示例 →](docs/video-search.md)

## 标注动作与示范视频

为普通视频、第一人称录像或机器人示范生成动作步骤、事件、交互和状态变化标注，无需先运行索引。

```sh
cerul annotate ./video.mp4 --semantic subtask,event,interaction,state
```

加上 `--dry-run` 可预览处理计划。结果保存在 JSONL sidecar 中，运行 `cerul status ./video.mp4` 查看位置。对于 LeRobot 数据集，可先用 `cerul annotate ./dataset --semantic --only 0` 标注第一个 episode。这些是语义标注，当前不提供姿态、深度或 3D 轨迹。

[标注类型、输出位置与 LeRobot 示例 →](docs/annotation.md)

## 让 agent 帮你安装

把下面这段话发给能操作终端的 agent：

```text
按 https://github.com/cerul-ai/cerul/blob/main/docs/agent-setup.md 帮我安装 Cerul，安全配置 Gemini API key，搜索一个本地视频并保存匹配片段，然后教我怎么使用。
```

支持 skill 的 agent 可以直接从 Cerul 学会整套命令行：

```sh
cerul skill --install claude    # 也支持 codex、pi，或 --dir ./skills
```

[Agent 安装指南 →](docs/agent-setup.md) · [Agent 接口约定 →](docs/agent.md)

## 常用命令

| 目标 | 命令 | 什么时候用 |
| --- | --- | --- |
| 处理整个文件夹 | `cerul index ./videos` | 会递归子目录，已完成的部分自动跳过 |
| 预览要做的事 | `cerul --dry-run index ./videos` | 花钱调模型之前，先看清处理计划 |
| 生成语义标注 | `cerul annotate ./videos --semantic` | 只要动作标注，不需要建搜索索引 |
| 查看进度和结果 | `cerul status` | 中断之后继续，或查找 sidecar 位置 |
| 限定一个视频搜索 | `cerul search "打开门" --in ./demo.mp4` | 跳过资料库里的其他视频 |
| 回看某条结果 | `cerul open 2` | 用播放器打开编号对应的片段 |

转录、标注和向量保存在视频旁的 sidecar 文件中，可不调用模型重建搜索索引。LeRobot 子任务回写需要显式选择开启。

## 配置模型服务

Cerul 调用的是你自己的模型服务，供应商由你选择，费用也记在你自己账上。运行 `cerul config` 可以修改下面任意一项。

| 环节 | 默认 | 可选 |
| --- | --- | --- |
| 多模态搜索 | Gemini Embedding 2，3072 维 | 任意已配置的服务 |
| 语音转录 | 有可用 key 时默认用 Gemini | Groq、OpenAI、OpenAI 兼容服务，或关闭 |
| 屏幕文字（OCR） | 本地运行，不调用 API | — |

索引只构建视频和文字的搜索数据，不会生成场景描述、章节或摘要。需要场景和总览请用 `cerul analyze ./video.mp4`，需要具身语义标注请用 `cerul annotate ./video.mp4`。`--no-audio` 可以单独跳过语音环节。

[模型服务与凭据 →](docs/configuration.md) ·
[查看已保存的分析结果 →](docs/video-search.md#inspect-video-understanding)

## 工作原理

索引会把画面、屏幕文字和可选的语音编码到同一个多模态空间。另有一条独立的视觉生成流程，为动作、交互和状态变化生成带时间戳的标注，同样适用于具身与第一人称录像。

![Cerul 架构：多模态索引与搜索，以及独立的第一人称标注流程](docs/assets/cerul-architecture.png)

*架构图中的画面由 AI 生成，仅作示意。默认搜索会把视频、语音、屏幕文字和描述四路独立候选，与受控的全文匹配结果，用 max fusion 和受限的一致性加权合并。原始证据和时间戳始终可以回溯查看。数据集回写支持可选开启的 LeRobot 子任务。已实现的行为详见 [DESIGN.md](DESIGN.md)。*

## 了解更多

- [文档目录](docs/README.md)
- [安装与排错](docs/installation.md)
- [视频搜索教程](docs/video-search.md)
- [显式视频分析](docs/analyze.md)
- [LeRobot 教程](docs/lerobot-subtasks.md)
- [模型服务与配置](docs/configuration.md)
- [贡献与开发集成](CONTRIBUTING.md)

## 社区

欢迎提问、报 bug，也欢迎分享你搜出来的得意片段。

[Discord](https://discord.gg/qHDEMQB9vN) ·
[Issues](https://github.com/cerul-ai/cerul/issues) ·
[X / Twitter](https://x.com/cerul_hq)

<a href="https://star-history.com/#cerul-ai/cerul&Date">
  <img src="https://api.star-history.com/svg?repos=cerul-ai/cerul&type=Date" alt="cerul-ai/cerul 的 Star 增长曲线" width="600">
</a>

## 许可证

Cerul 的 Rust 代码采用 Apache-2.0。发行包中的媒体工具和 OCR 模型各自遵循其许可证，详见[第三方声明](THIRD_PARTY_NOTICES.md)。[Cerul 名称与标识](docs/assets/README.md)用于识别项目，不代表对第三方产品的背书。
