# Cerul — Press Kit

Thanks for writing about Cerul! Everything you need to cover us accurately is
in this folder. If anything is missing or you need a custom asset (transparent
PNG at a specific size, dark-mode screenshot, founder headshot), email
**support@cerul.ai** and we'll turn it around quickly.

所有素材可免费用于新闻报道、合作伙伴展示、第三方集成等用途，请遵循
`06-brand-guidelines/brand-guidelines.md` 中的使用规范。

## What is Cerul?

**Cerul: where video becomes citable.** It lets agents and
developers search video by meaning — across speech, visuals, and on-screen
text — through a single API.

- **Website:** https://cerul.ai
- **Docs:** https://cerul.ai/docs
- **Dashboard:** https://cerul.ai/dashboard
- **Status:** https://status.cerul.ai
- **X / Twitter:** https://x.com/cerul_hq
- **Discord:** https://discord.gg/qHDEMQB9vN
- **GitHub:** https://github.com/cerul-ai
- **Contact:** support@cerul.ai

完整的一句话介绍、短/长描述、boilerplate 都在
`06-brand-guidelines/company-info.md`。

## 目录结构

```
press-kit/
├── README.md                       ← 本文件
├── BRAND.md                        品牌规范（= 06-brand-guidelines/brand-guidelines.md 的根目录副本）
│
├── 01-logos/                       全尺寸源文件（设计师导出，1024/1376 px）
│   ├── primary-horizontal/         横版主 logo：图标 + 文字（官网 header、邮件、PPT 首页）
│   │   ├── cerul-primary-black.png
│   │   ├── cerul-primary-white.png
│   │   └── cerul-primary-color.png
│   ├── stacked/                    竖版：图标在上、文字在下（海报、名片、方形位）
│   │   ├── cerul-stacked-black.png
│   │   └── cerul-stacked-white.png
│   ├── icon/                       纯图标（favicon / App icon / 头像源）
│   │   ├── cerul-icon-black.png
│   │   ├── cerul-icon-white.png
│   │   └── cerul-icon-color.png
│   ├── wordmark/                   纯文字 logo
│   │   ├── cerul-wordmark-black.png
│   │   ├── cerul-wordmark-white.png
│   │   ├── cerul-wordmark-color-light.png   浅底方案
│   │   └── cerul-wordmark-color-dark.png    深底方案（立体/压印效果）
│   └── vector-svg/                 矢量 SVG 版本（可任意缩放）
│       ├── cerul-primary-black.svg
│       ├── cerul-primary-white.svg
│       ├── cerul-primary-color.svg
│       └── cerul-wordmark.svg
│
├── 02-transparent/                 透明背景版本（直接叠加在任意背景上）
│   ├── cerul-primary-black-transparent.png
│   ├── cerul-primary-white-transparent.png
│   ├── cerul-stacked-black-transparent.png
│   ├── cerul-stacked-white-transparent.png
│   ├── cerul-icon-black-transparent.png
│   ├── cerul-icon-cream-transparent.png
│   ├── cerul-icon-white-transparent.png
│   ├── cerul-wordmark-black-transparent.png
│   └── cerul-wordmark-white-transparent.png
│
├── 03-app-icons/                   Web & App 图标
│   ├── favicon.ico                 16/32/48/64 多尺寸
│   ├── apple-touch-icon.png        180×180
│   ├── icon-192.png                PWA
│   ├── icon-512.png                PWA
│   ├── app-store-icon-1024.png     App Store（深底+白 mark，无透明）
│   └── app-store-icon-light-1024.png   浅色版（奶油底+黑 mark）
│
├── 04-social/                      社交媒体
│   ├── avatar-light-400.png
│   ├── avatar-dark-400.png
│   ├── cerul-og-1200x630.png       OpenGraph 分享卡（Twitter/LinkedIn/Slack 预览）
│   ├── cerul-og-2400x1260.png      retina 版
│   ├── cerul-og-800x418.png        小尺寸
│   ├── cerul-og-source-2752x1536.png   源文件
│   ├── twitter-header-dark.png     Twitter/X 封面 1500×500
│   └── twitter-header-light.png
│
├── 05-product-screenshots/         产品截图（可直接用作媒体/文章配图）
│   ├── screenshot-home.png         官网首页 hero（最佳封面图）
│   ├── agent-skill-search.png      Claude Code 使用 Cerul skill
│   ├── agent-skill-result.png      Agent 综合视频证据
│   ├── cli-search.png              cerul CLI 输出
│   └── usage/                      真实使用场景
│       ├── cli-sam-altman-search.png      CLI 搜索 + 内联视频帧
│       ├── cli-dario-amodei-result.png    CLI 结果 + Dwarkesh Patel 片段
│       ├── claude-code-demis-research.png Claude Code 运行 skill
│       ├── claude-code-research-notes.png Agent 合成的研究笔记
│       └── telegram-bot-dario-query.jpg   Telegram bot 集成（中文）
│
├── 06-brand-guidelines/
│   ├── brand-guidelines.md         配色、字体、安全边距、禁用示例
│   └── company-info.md             公司简介 / boilerplate / 联系方式
│
└── video/
    └── demo.mp4                    产品 demo（约 7 MB，可循环播放）
```

> **Legacy 提示：** 根目录下仍残留旧版结构的 `icons/`、`logo/`、`social/`、
> `screenshots/` 文件夹，它们的内容已经被上面的 01–05 编号结构完全取代，可
> 手动删除。

## 快速选择

| 场景 | 用哪个文件 |
|---|---|
| 官网 header（浅色主题） | `01-logos/primary-horizontal/cerul-primary-black.png` |
| 官网 header（深色主题） | `01-logos/primary-horizontal/cerul-primary-white.png` |
| 媒体报道配图 | `01-logos/primary-horizontal/cerul-primary-color.png` |
| 需要矢量缩放 | `01-logos/vector-svg/` |
| PPT 首页 | `01-logos/stacked/cerul-stacked-black.png` |
| App 图标 | `03-app-icons/app-store-icon-1024.png` |
| Twitter/X 分享 | `04-social/cerul-og-1200x630.png` |
| 社交头像 | `04-social/avatar-dark-400.png` |
| 浏览器 favicon | `03-app-icons/favicon.ico` |
| 嵌入任意背景 | `02-transparent/` 下对应版本 |
| 媒体文章配图 | `05-product-screenshots/` |
| 产品 demo 视频 | `video/demo.mp4` |

## Usage

- You may use these assets to link to, write about, or review Cerul.
- Please don't modify the logo (don't recolor, stretch, rotate, or add effects).
- Please don't use our logo or name in a way that implies partnership,
  endorsement, or sponsorship without written permission.
- Full rules: see `06-brand-guidelines/brand-guidelines.md`.

## 联系

- 媒体 / 合作：**support@cerul.ai**
- 品牌资源申请：**jessytsui@outlook.com**
