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

Cerul 是一个搜索视频、导出片段的命令行工具。安装包已包含所需的媒体工具和 OCR 模型。

支持 **macOS Apple Silicon** 和 **Linux x86_64（Ubuntu 24.04 或更新）**。

### 1. 安装

```sh
curl -fsSL https://cerul.ai/install.sh | sh
```

### 2. 添加视频

```sh
cerul index ./demo.mp4
```

第一次使用时，按提示输入 [Gemini API key](https://aistudio.google.com/apikey)，之后无需重复输入。模型处理会将数据发送给 Gemini，并可能产生 API 费用。

### 3. 搜索并保存片段

```sh
cerul search "有人把杯子放到桌上"
cerul search "有人把杯子放到桌上" --save ./clips
```

[更多使用示例 →](examples/video-search.md)

## 让 agent 帮你安装

把下面这段话发给能操作终端的 agent：

```text
按 https://github.com/cerul-ai/cerul/blob/main/docs/agent-setup.md 帮我安装 Cerul，安全配置 Gemini API key，搜索一个本地视频并保存匹配片段，然后教我怎么使用。
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
