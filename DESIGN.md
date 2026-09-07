# Cerul CLI

定稿 2026-09-08。本仓库全部重写，旧内容不保留（只留 LICENSE、Git 历史、Releases 里的 `ffmpeg-vendor-*` 资产，Desktop 构建依赖它们）。本文只写接下来要做的东西。

**一句话**：`cerul` 把视频变成可搜索、可标注、可用于训练的数据。装上即用，自己填模型 Key，不需要 Cerul 账号。

---

## 1. 决策

| 项 | 选择 |
|---|---|
| 语言 | Rust，单 crate，单二进制。里面只有：ffmpeg 子进程、四种模型端点客户端、LanceDB、内置 OCR。没有 PyTorch、GPU、Python、插件 |
| 命令 | M1 五个：`index` `annotate` `search` `status` `clean`。`serve`（HTTP 与 MCP）整体在 M2 |
| 结构 | 逻辑全在 `lib.rs`，`main.rs` 只解析参数、调库、打印。Desktop 可直接链接库，或起子进程读 `--json` 进度事件，不依赖 `serve` |
| 真相 | 每个 episode 一个旁车目录（普通视频：`demo1.mp4.cerul/`），jsonl 与 parquet，**含 embedding 向量**。索引是可从旁车零调用重建的缓存 |
| 索引 | LanceDB，每个 `space_id` 一个目录。`space_id` = hash(kind, base_url, model, dims, 查询指令模板)，同名模型不同端点即不同空间，查询严格匹配 |
| 模型端点 | `embedding` `vision` `transcription`（`kind = gemini \| openai`，用户自己的 Key）+ `perception`（Cerul 契约，默认 Cerul Cloud，M1 阶段不实现）。能力检查按当前命令所需执行，缺 perception 不影响 index/search |
| 默认模型 | 全 Gemini 一把 Key：`gemini-embedding-2`（1536 维）、`gemini-3.8-flash` |
| OCR | 内置 PP-OCRv6 small（31MB，`tract-onnx`，CPU），默认开。二进制里唯一的模型，唯一例外 |
| 检索 | 一个多模态向量空间：每 30 秒单元最多三行（视频、转录文本、屏幕文字）同一模型嵌入。**embedding 端点必须支持视频与图像输入，否则报不支持，没有纯文本回退**。无 BM25、无融合。精确字符串走 `--text` 子串模式。annotation 过滤在向量检索**之前**生效 |
| 标注 | 三大类固定：`semantic` `grounding` `world`；子项可增。派生数据统称 annotation |
| 时间 | 整数微秒，半开区间，episode 时间轴，来自 PTS |
| 多相机 | 默认只处理主相机，`--streams all` 扩大 |
| 语言 | annotation 文本字段固定英文 |
| LeRobot | v3.0 与 v3.1 都能读取并生成旁车；`--write-lerobot` 只对已经是 v3.1 的数据集写回（language 列是 v3.1 引入的），v3.0 数据集报不支持，不做格式升级。M1 只写回 `subtask`，event/flag 留在旁车 |
| 平台 | M1：macOS arm64、Linux x86_64。Windows M2 |
| 版本 | 只用 `v0.0.x` 递增，没有 alpha 或 beta 后缀。npm 已发到 0.0.2，首个发布是 **v0.0.3**，crates.io 同号。里程碑用 M1/M2/M3 指代，不绑具体版本号 |
| 发布 | cargo-dist：curl 脚本、Homebrew、npm `cerul` 壳 |
| 遥测 | 零 |
| 云端 | 消费同一个二进制的 `serve`；额度计量与 perception 实现都在云端，CLI 只透传 `CERUL_API_KEY` |
| 范围 | M1 是完整的视频检索与语义标注 CLI，只需第三方 Key；姿态、深度、分割依赖后续 perception 服务，不宣传为现在可用 |

---

## 2. 用法

```bash
curl -fsSL https://cerul.ai/install.sh | sh
export GEMINI_API_KEY=...

cerul index ./videos                          # 登记、probe、转录、OCR、embedding
cerul search "拿起杯子后又放回桌上"
cerul search --image ref.png --save ./clips
cerul search --text "ECONNREFUSED"
cerul annotate ./videos --semantic
cerul search --filter semantic.event.verb=regrasp
cerul status
```

依赖：`ffmpeg`/`ffprobe` ≥ 6.0 在 PATH。工作区默认 `~/.cerul/`，`--workspace` 或 `CERUL_WORKSPACE` 覆盖。

---

## 3. 命令

全局：`--json`（stdout 只出一个最终 JSON；stderr 每行一个 JSON 事件：`{"event":"progress","episode":…,"station":"embed","done":37,"total":120}` 与 `{"event":"log","level":…,"msg":…}`，Desktop 起子进程即可读进度）、`--workspace`、`--recompute`、`--dry-run`、`--yes`、`-q`/`-v`。退出码：0 成功，2 参数或配置，3 缺依赖或能力，4 执行失败，5 取消，6 部分成功。

### `index <path>...`

`<path>`：文件、目录、LeRobot v3 数据集，自动识别。

| 参数 | 默认 |
|---|---|
| `--chunk 30s` `--overlap 5s` | 单元 ≤ 32s（Gemini 视频 embedding 按 1 fps 采样上限） |
| `--no-audio` `--no-ocr` `--skip-still` | 转录按 probe 自动，OCR 默认开；静止单元不嵌入 |
| `--streams primary\|all\|a,b` | primary |
| `--only SEL` | 全部 |
| `--jobs 4` `--rpm N` | |
| `--sidecar-dir DIR` | 媒体目录不可写时用 `~/.cerul/sidecars/<sha256>/` |

流程：发现 → sha256 → ffprobe → 站（transcript / screen_text / embed）→ 写 Lance。已存在且哈希、模型、参数未变的产物跳过。

### `annotate <path>... --semantic [items] --grounding [items] --world [items]`

| 参数 | 默认 |
|---|---|
| `--semantic` | LeRobot 数据集：`task,subtask,event,interaction,state,flag,progress`；普通视频：`task,subtask,flag`，其余三个显式指定 |
| `--grounding` | `box,affordance`（`mask` `track` `keypoint` 需 perception，M2） |
| `--world` | `camera,hand,object,depth,points`，全部需 perception，M2 起 |
| `--streams` `--only` `--ontology FILE` `--window 30s` `--fps 2` | 词表：LeRobot 默认 `cerul.verbs.v1` 并校验 verb；普通视频不给 `--ontology` 时 verb 为自由文本，不校验 |
| `--write-lerobot` `--out DIR` | 默认不动用户数据集；只接受 v3.1 数据集 |

每个子项一个模块：抽帧 → 带时间戳接触表 → 模型调用（JSON Schema 约束）→ `staging/` → 校验 → 写 annotation 文件 → 更新 `records` 表。校验失败整模块不落盘，其他模块不受影响。

### `search [query]`

| 参数 | 默认 |
|---|---|
| `--image PATH` | query、`--image`、`--filter` 至少一个；只给 `--filter` 时不算向量，按时间顺序返回匹配记录 |
| `--limit 10` `--threshold X` | M1 无默认阈值 |
| `--filter k=v`（可重复） | 键：`<annotation>.<field>`、`episode`、`stream`、`kind`；运算 `= != > < ~`。先解析成时间范围，再在范围内做向量检索（预过滤，不是取前 k 再过滤） |
| `--in PATH` | |
| `--count` | 只与 `--filter` 连用，返回精确标签的记录数与 episode 数。不提供自然语言探针计数 |
| `--save DIR` `--pad 2s` | 命中切 mp4 |
| `--rerank` | vision 模型重排前 20（M2） |
| `--text` | 子串匹配转录与屏幕文字，不走向量 |

返回 `hits[]: {episode, stream, start_us, end_us, frame_range?, score, matched: video|speech|screen, excerpt, annotations[]}`。

### `status [path]`

工作区总览或某 episode 的 annotation 清单。`--providers` 探测四种端点能力并缓存 7 天。`--json` 根对象含 `capabilities`。`cerul` 不带子命令等于 `status`。

### `clean`

`--index SPACE_ID`（`status` 列出 space_id 与对应模型）`--all-indexes` `--cache` `--compact`；`--sidecars PATH --yes` 是唯一删旁车的方式。全部支持 `--dry-run`。

### `serve`（M2）

`--mcp`：stdio MCP，暴露 index/annotate/search/status，schema 与 `--json` 一致。HTTP：`--host 127.0.0.1` `--port 0` `--token`（自动，`<workspace>/runtime/token`），供 Cloud 使用。M1 不实现；Desktop 在 M1 用子进程加 `--json` 事件接入。

---

## 4. 配置与模型

优先级：命令行 > 环境变量 > `./cerul.toml` > `~/.cerul/config.toml`。只记 Key 的环境变量名。

```toml
[embedding]
kind = "gemini"
model = "gemini-embedding-2"
dims = 1536

[vision]
kind = "gemini"
model = "gemini-3.8-flash"

[transcription]
kind = "gemini"
model = "gemini-3.8-flash"

# 换本地或第三方端点的写法：
# [vision]
# kind = "openai"
# base_url = "http://localhost:11434/v1"
# model = "qwen3-vl"
#
# [transcription]
# kind = "openai"
# base_url = "https://api.groq.com/openai/v1"
# model = "whisper-large-v3-turbo"
# api_key_env = "GROQ_API_KEY"
#
# [perception]                     # 默认值，M1 不用写
# base_url = "https://api.cerul.ai"
# api_key_env = "CERUL_API_KEY"
```

| 能力 | gemini | openai |
|---|---|---|
| embedding | `embedContent`，`outputDimensionality=1536`，文本查询前加 "Retrieve video segments matching:" | `/v1/embeddings`。**必须接受图像与视频输入**，探测时发一张图与一段 2 秒视频；只支持文本的端点直接报不支持 |
| vision | `generateContent` + `responseSchema` | `/v1/chat/completions` + `image_url` + `json_schema` |
| transcription | `generateContent` 输入音频，schema 分段 | `/v1/audio/transcriptions`，`verbose_json`，segment 粒度 |

探测只针对本次真正要发出的远程调用，不按命令名：`index` 在有待嵌入的单元时探测 embedding、有待转录的音轨时探测 transcription；`annotate` 按所选子项探测 vision 或 perception；`search` 只在要算查询向量时探测 embedding。纯 `--filter` 查询、`--text`、从旁车重建索引、`status`、`clean` 不发任何探测。`status --providers` 全查。embedding 发一句话、一张图、一段 2 秒视频核维度与模态；vision 发 64×64 图要 `{"ok":true}`；transcription 发 1 秒静音；perception 读 `/capabilities`。所需能力探测失败退出码 3，不开始处理媒体。首次向某端点发送媒体打印一次提示，`--yes` 跳过。

请求一律内联，编码后按真实字节数检查端点上限（Gemini 20MB），超限先降码率再切分。

### perception 契约（M2 起，CLI 只认契约）

| 路由 | 入 | 出 | 子项 |
|---|---|---|---|
| `GET /capabilities` | | `{version, tasks[], models{}}` | |
| `POST /segment` | 帧 zip + 文本或框提示 | 每帧 RLE | `grounding.mask` |
| `POST /track` | 代理片段 + 起始点/框 | `[[t_us,x,y,visible]]` | `grounding.track` |
| `POST /depth` | 帧 zip | 16 位 png + scale | `world.depth` |
| `POST /camera` | 代理片段 + 可选内参 | 位姿序列 + 点云 | `world.camera` `world.points` |
| `POST /hand` | 代理片段 | 双手关节序列 + valid | `world.hand` |

输入是 CLI 上传的字节，服务端不读调用方路径。

---

## 5. 存储

```text
videos/
  demo1.mp4
  demo1.mp4.cerul/
    episode.json                # 流列表（sha256、probe、primary）、时间轴、任务
    transcript.jsonl            # 以下皆 annotation，文件名 = annotation 名
    screen_text.jsonl
    semantic.subtask.jsonl
    grounding.box.jsonl
    world.hand.parquet          # 逐帧稠密数据用 parquet
    embeddings/<space_id>.parquet   # 向量：stream, kind, start_us, end_us, vector, params_hash。索引从它重建，clean 不删
    staging/  log.jsonl

my_dataset/                     # LeRobot v3
  meta/ data/ videos/
  .cerul/dataset.json           # dataset_id（首次 index 生成的 uuid）+ 根路径 + info.json 哈希
  .cerul/episodes/000012/       # 同上结构

~/.cerul/
  config.toml  providers.json  runtime/token
  registry.jsonl                # index 过的根路径 + sha256→旁车；视频移动后按哈希找回
  cache/<sha256>/proxy.mp4
  sidecars/<sha256>/
  index/<space_id>/chunks.lance  records.lance
```

单机单写：`runtime/lock` 文件锁，拿不到锁退出码 4。

**写入与恢复规则**（M1 只有这两条）：

1. 任何产物先写 `staging/` 临时文件，校验通过后原子改名到正式位置；正式位置不存在半成品。
2. `index` 与 `annotate` 幂等：旁车里已完成的站跳过，只补缺的；旁车完成但 Lance 缺行时只补索引，零模型调用。`--recompute` 整文件重写。

M2 随 `serve` 一起加：job 状态文件与 `/jobs/{id}`；随 Desktop 编辑入口一起加：人工修订记录与 `supersedes` 读取规则。M1 没有这两者的消费者。

---

## 6. 数据模型

**episode**：一次录制，一到多条流与共同时间轴。M1 识别规则只有两条：LeRobot 数据集按其元数据；其余每个视频各自是单流 episode。用户手写 `episode.json` 描述多相机目录在 M2。

**身份**：全局 `episode_id = <dataset_id>/<local_id>`。单流视频的 `dataset_id` 是视频 sha256 前 16 位，`local_id` 固定 `0`；LeRobot 的 `dataset_id` 是 `.cerul/dataset.json` 里首次 index 生成的 uuid，`local_id` 是原 episode_index，显示时保留原编号。两个数据集的 `000012` 永不相撞。

**时间**：三个时间轴，转换规则固定。源文件时间 `t_src`（该 mp4 的 PTS）；episode 时间 `t_ep = t_src - range_us[0]`（主流），其他流再经 `mappings` 的 `a, b_us`；模型输入片段时间 `t_clip`，片段从 `t_ep = clip_start_us` 切出，模型返回的任何时间 `t` 一律换算 `t_ep = clip_start_us + t`。所有 annotation 与 chunks 只存 `t_ep`。例：episode 占源视频 120–150 秒，模型对该片段返回 5 秒，落盘为 `t_ep = 5 s`，对应源视频 125 秒。

```json
{"$cerul":"episode/1","episode_id":"9f3c2a7b1e4d5c60/000012","dataset_id":"9f3c2a7b1e4d5c60","local_id":"000012",
 "streams":[
   {"id":"front","kind":"video","primary":true,"sha256":"…","path":"videos/front/file-0000.mp4","range_us":[120000000,150000000],
    "probe":{"duration_us":…,"fps":30,"width":1920,"height":1080,"has_audio":false,"codec":"h264"}},
   {"id":"state","kind":"parquet","path":"data/chunk-000/file-0000.parquet","columns":["observation.state","action"],"row_range":[3600,4500]}],
 "time":{"reference":"front","mappings":{"wrist":{"a":1.0,"b_us":0,"status":"calibrated|estimated|unknown"}}},
 "task":"Grab the black cube","source":{"format":"lerobot/3.0","root":"./my_dataset"}}
```

**annotation 文件**：jsonl，首行文件头，其后每行一条记录。

```json
{"$cerul":"annotation/1","name":"semantic.event","episode":"9f3c2a7b1e4d5c60/000012","stream":"front",
 "model":{"kind":"gemini","name":"gemini-3.8-flash"},"params":{…},"created":"…",
 "cerul_version":"0.0.3","input_hash":"sha256:…","record_schema":"semantic.event/1"}
```

记录公共字段：`id` `start_us` `end_us` `confidence`（模型自报，未校准）。读取端接受旧版本 schema，缺字段为 null；破坏性变更升主版本换文件名。

| 类 | 子项与字段 |
|---|---|
| semantic | `task{text}` `subtask{text,index}`（无缝覆盖）`event{verb,objects[],actor,outcome}` `interaction{hand,object,contact}` `state{object,attribute,before,after}` `flag{kind,note}` `progress{value,done}` |
| grounding | 坐标归一化 `[0,1]`，附 `frame_w/h`。`box{t_us,label,xyxy,track_id?}` `affordance{t_us,label,points,action_hint}` `trace{points[[t_us,x,y]],subject,label}` `keypoint` `mask{rle}` |
| world | `frame: T_<a>_from_<b>`、`unit: m\|relative`、四元数 `xyzw`、`valid`，缺值 null。`camera{t_us,T_world_from_camera[7],intrinsics?,scale,valid}` `hand{t_us,side,joints[21][3],valid}` `object` `depth{t_us,path,scale}` `points` |

**索引单元**：某视频流 30 秒区间（episode 时间），最多三行向量，`kind = video | speech | screen`。向量先落旁车 `embeddings/<space_id>.parquet`，再进 `chunks` 表。`chunks` 表列：`id, episode, stream, kind, start_us, end_us, vector[1536], text, still, space_id, params_hash`。`records` 表由旁车重建：`episode, stream, annotation, id, start_us, end_us, fields`。

默认词表 `cerul.verbs.v1`：`reach grasp regrasp lift carry place release push pull open close insert rotate pour wipe`。

---

## 7. 关键行为

**index 各站**

- transcript：音频 ≤ 10 分钟一段，输出 `{start_us,end_us,text,lang}`。
- screen_text：PP-OCRv6 det 跑 0.5 fps **原分辨率**关键帧（上限 1080p 长边），有框才跑 rec；DBNet 后处理与 CTC 解码 Rust 实现；连续相同文本合并。不用 480p 代理，屏幕小字会丢。性能与体积以实测为准（见第 10 节）。
- keyframes：0.5 fps，原分辨率（上限 1080p），按需从源文件抽到临时目录，用完即删，不落 cache。proxy：480p 2 fps h264，进 cache，只用于 embedding 与接触表。
- embed：每单元最多三次 `embedContent`，各自独立包装；请求体编码后检查真实字节数，超过端点上限（Gemini 内联 20MB）先降码率再切分，仍超则报错。向量写 `embeddings/<space_id>.parquet` 后再入 Lance。失败进 `log.jsonl`，重跑只补失败项。
- 缓存键：`(episode_id, stream, stream_sha256, range_us, 时间映射, station, space_id 或 model, params_hash, 站版本)`。身份说明是哪份数据，内容哈希说明数据有没有变，缺一不可：同一分片里的两个 episode 靠 `range_us` 区分，编号不变但视频被替换靠 `stream_sha256` 区分。

**annotate**：`--fps 2` 抽帧，5 列网格，每帧角上烧入**片段相对时间**（每个窗口从 0 起），模型返回的时间只加一次该窗口的 `clip_start_us`，不再叠加；subtask 先描述再切分；边界定义固定（持住、释放、到位、状态改变）；窗口重叠 5 秒，冲突写 `flag`。校验：边界吸附真实帧 PTS、subtask 无缝、verb 在词表、坐标在 `[0,1]`。

`--write-lerobot`（M1 只写 `subtask`；输入必须已是 v3.1，否则退出码 3 并提示用 LeRobot 官方工具升级）：按 LeRobot v3.1 语言列规范写 `language_persistent`，style 固定 `subtask`，每帧恰好一条活动 subtask，时间戳直接取源 parquet 的帧时间不重算；已有的 `language_persistent`/`language_events` 内容、action、state、tasks 原样保留，只追加或替换 style 为 `subtask` 的条目；`meta/info.json` 不改动。event/flag 没有官方 style，留在旁车，不写入。写回前后各读一次同一帧做对拍。

**search**：先解析 `--filter` → 从 `records` 表得到 `(episode, stream, [start_us,end_us))` 集合 → 只对落在集合内的 `chunks` 行做向量检索（Lance `where` 预过滤）→ 同区间取最高分并记 `matched` → 相邻合并 → `--rerank`。无 `--filter` 时对全表检索。只有 `--filter` 时跳过向量，按时间返回记录。查询向量的 `space_id` 必须与索引一致，否则退出码 3。annotation 未生成时 filter 报能力缺失（退出码 3），不是空结果。

**serve** HTTP 端点（M2）：`GET /status` `GET /episodes[/{id}]` `POST /index` `POST /annotate` `POST /search` `POST /clip` `GET /jobs/{id}`（`Accept: application/x-ndjson` 流式）`GET /annotations/{episode}/{name}?format=json|srt|vtt` `GET /media/{episode}/{stream}`（Range）。错误 `{code, message, retryable, request_id}`。OpenAPI 由 `utoipa` 生成并提交。

---

## 8. 仓库

```text
cerul/
├── Cargo.toml                  # lib + bin = cerul
├── src/
│   ├── lib.rs                  # 全部逻辑的公开入口
│   ├── main.rs cli.rs          # 只解析参数、调库、打印
│   ├── config.rs
│   ├── providers/{gemini,openai,perception}.rs
│   ├── media/{ffmpeg,probe,keyframes,proxy,contact_sheet}.rs
│   ├── ocr/{det,rec,postprocess}.rs      # tract-onnx，模型 include_bytes!
│   ├── annotations/{schema,io,records}.rs
│   ├── episode.rs lerobot.rs
│   ├── index/{discover,hash,stations,chunk,embed,lance}.rs
│   ├── annotate/{contact,prompt,validate,semantic,grounding,world}.rs
│   ├── search.rs status.rs clean.rs
│   └── serve/{http,mcp,openapi}.rs  # M2
├── models/                     # det.onnx rec.onnx 字典 + README（来源、版本、Apache-2.0）
├── prompts/                    # 每模块一个 .md，内嵌
├── schemas/                    # schemars 生成，提交入库；第一个 PR 就交付，Desktop/Cloud 对着它开工
├── tests/                      # ≤10 秒许可清晰的样例；录制的端点响应回放
├── examples/                   # 普通视频、多相机目录、LeRobot 数据集三个教程
├── dist-workspace.toml  npm/
└── README.md  DESIGN.md  LICENSE
```

依赖：`clap` `tokio` `reqwest` `serde` `serde_json` `schemars` `lancedb 0.38` `arrow` `parquet` `image` `sha2` `axum` `utoipa` `indicatif` `tracing` `tract-onnx`。`rerun` 放 feature，M2。ffmpeg 子进程调用。二进制约 60MB。

CI：每个 PR 跑离线测试与双平台构建；受保护分支跑一组真实 Gemini 小调用；fork PR 不注入 Key。

---

## 9. 里程碑与验收

| 版本 | 内容 |
|---|---|
| **M1**（首发 v0.0.3） | `index`（transcript、OCR、embed、still）、`search`（向量、image、预过滤 filter、filter 计数、save、text）、`status`、`clean`、`annotate --semantic`、LeRobot 读取与 subtask 写回、mac + Linux 发布、npm `cerul` 壳替换旧包 |
| M2 | `serve`（HTTP + MCP）、Windows、`grounding` box/affordance、perception 契约 + 云端 segment/depth/track、`--rerank`、Rerun `.rrd` |
| M3 | 云端 camera/hand、`keypoint`、`--target cloud` |

实施顺序（每个 PR 独立可跑）：① 类型与 `schemas/`、config、`status`、端点探测 ② media 四站 + 旁车 + Lance ③ `search` ④ `annotate --semantic` + LeRobot ⑤ cargo-dist 发布 + npm 壳。

M1 验收：

1. 全新 mac/Linux，一条安装命令加 `GEMINI_API_KEY`，10 分钟内 index 一段 10 分钟带语音视频并搜到正确时刻。
2. 对样例视频人工写 10 个问题（画面、语音、屏幕文字各至少 3 个），Recall@5 ≥ 8/10；语音类问题由 `speech` 行命中，屏幕文字类由 `screen` 行命中。
3. 5 个 LeRobot episode `annotate --semantic`：subtask 无缝、边界在真实帧；`--write-lerobot --out` 后用 `lerobot` 加载并读取指定帧，`language_persistent` 的 subtask 内容与时间正确，原有 action/state 与已有注释逐字段保留。
3a. 同一分片 mp4 内相邻两个 episode 分别标注，记录时间互不串位；两个数据集各自的 `000012` 互不覆盖。
4. 重跑零模型调用；改 embedding 模型只重算 embed 站。
5. `search --json`、`status --json` 输出符合 `schemas/`；`--json` 的 stderr 进度事件能被逐行解析。
6. vision 指向本地 Ollama 可完成 semantic；embedding 指向不支持图像输入的端点时探测失败、退出码 3，不建索引。
7. 删 `~/.cerul/index/` 后从旁车 `embeddings/*.parquet` 零调用重建；`index` 跑到一半 Ctrl-C，重跑只补缺的站，正式位置没有半成品文件。
7a. `--filter semantic.event.verb=regrasp` 在命中排在向量前 10 之外时仍能返回。
8. 断网时 `index --no-audio` 完成 probe、关键帧、OCR，退出码 6，报告"本地站完成、embed 未完成"，`status` 显示该 episode 未索引；联网后重跑只补 embed。
9. 内置 OCR 在 20 段样例上不低于 cerul-platform 现有 Python sidecar。
10. 二进制不依赖 Python、CUDA、动态 ML 库；只要 ffmpeg。

---

## 10. 开工时核对

- `gemini-3.8-flash` 现行 id、价格、音频转录的段级时间戳精度（不达标则默认 transcription 改 whisper 端点，配置形状不变）。
- `tract-onnx` 对 PP-OCRv6 det/rec 算子覆盖；不覆盖退 `ort` 静态链接。实测双平台 CPU 吞吐与最终二进制体积，文档里的"一小时 1–2 分钟""约 60MB"在实测前只是估计。
- `lancedb 0.38` Rust API，特别是向量检索的 `where` 预过滤。
- 样例：`~/cerul-ai/` 下的产品录屏做屏幕样例；挑一个小的公开 LeRobot 数据集并核许可。
- `cerul.verbs.v1` 十五词过目。

## 11. 不做

DAG/profile/Run 状态机、SSE、幂等键、插件、bundle 格式、`ask`、`export`、`init`、`rm`、SQLite、Python SDK/PyO3、OCR 之外的内置推理、Temporal、多租户 `serve`、知识图谱、聊天 UI、`action` 类、retargeting、训练。第五个站、第二个 embedding 供应商、第四类标注出现前不抽象接口。
