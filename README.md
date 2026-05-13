# 墨岩K线 — A股K线分析系统

> 类同花顺的 Mac 桌面端 A 股 K 线分析应用，集成 **缠论** + **威科夫量价分析**

## ✨ 功能特性

- 📊 **专业 K 线图表** — 基于 TradingView lightweight-charts，支持缩放、十字线、多级别切换
- 🔴 **缠论分析** — 分型识别、笔/线段构建、中枢识别、背驰检测、三类买卖点标注
- 🟣 **威科夫分析** — 阶段识别（吸筹/拉升/派发/下跌）、关键形态标注（Spring/JOC/LPS/UTAD）、趋势线与冰线
- 📈 **技术指标** — MACD、成交量分析
- 🔄 **多数据源同步** — 东方财富、新浪、腾讯、Tushare、网易五大数据源自动协同与降级
- 🏷️ **板块管理** — 上证主板/深证主板/创业板/科创板分类统计与按板块同步
- ✅ **数据校验** — OHLC 逻辑校验、缺口检测、跨数据源交叉验证
- 🎨 **深色主题** — 类同花顺专业深色 UI

## 🖥️ 界面预览

```
┌──────────────────────────────────────────────┐
│  顶部栏: 股票搜索 | 当前股票名称/代码/价格   │
├──────┬───────────────────────────────────────┤
│      │  工具栏: 月|周|日|60F|30F|15F|5F|1F  │
│ 自选 ├───────────────────────────────────────┤
│ 股列表│  K线主图区                            │
│      │  (含缠论笔/线段/中枢/威科夫标注)       │
│      ├───────────────────────────────────────┤
│      │  成交量副图                            │
│      ├───────────────────────────────────────┤
│      │  MACD 副图                             │
│      ├───────────────────────────────────────┤
│      │  右侧面板: 缠论勾选 | 威科夫勾选       │
│      │  笔信息 | 线段信息 | 中枢信息           │
└──────┴───────────────────────────────────────┘
```

## 🏗️ 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| 桌面框架 | Tauri 2.0 | Rust 后端 + WebView 前端，体积小性能高 |
| 后端语言 | Rust | 全部核心逻辑用 Rust 实现 |
| 前端框架 | Vue 3 + TypeScript | 响应式 UI |
| K 线图表 | lightweight-charts (TradingView) | 专业金融图表 |
| 样式 | Tailwind CSS 4 | 快速构建深色主题 |
| 数据存储 | Parquet (Polars) | 高效列式存储，适合金融时序数据 |

## 📦 项目结构

```
yifangmoyan/
├── src-tauri/                  # Tauri 应用入口 (Rust)
│   ├── src/
│   │   ├── commands.rs         # Tauri IPC 命令（前后端通信）
│   │   ├── state.rs            # 应用状态管理
│   │   └── lib.rs              # 应用初始化与插件注册
│   └── tauri.conf.json         # Tauri 配置
├── crates/
│   ├── data/                   # yifang-data — 数据层
│   │   └── src/
│   │       ├── types.rs        # KLine, TimeFrame, StockInfo 等基础类型
│   │       ├── source.rs       # DataSource trait，统一数据源抽象
│   │       ├── parquet_reader.rs # Parquet 文件读取
│   │       ├── kline_manager.rs # K 线管理，多级别合成
│   │       ├── sync.rs         # 多数据源协同同步 + 数据校验
│   │       └── cache.rs        # SQLite 本地缓存
│   ├── czsc/                   # yifang-czsc — 缠论核心
│   │   └── src/
│   │       ├── include.rs      # 去除包含关系
│   │       ├── fenxing.rs      # 分型识别
│   │       ├── bi.rs           # 笔的构建
│   │       ├── xd.rs           # 线段分析
│   │       ├── zs.rs           # 中枢识别
│   │       ├── beichi.rs       # 背驰检测
│   │       └── buy_sell.rs     # 三类买卖点
│   ├── wyckoff/                # yifang-wyckoff — 威科夫量价
│   │   └── src/
│   │       ├── phase.rs        # 阶段识别
│   │       ├── pattern.rs      # 关键形态
│   │       ├── annotation.rs   # 标注生成
│   │       └── volume.rs       # 量价分析
│   └── indicator/              # yifang-indicator — 技术指标
│       └── src/
│           ├── macd.rs         # MACD 计算
│           └── volume.rs       # 成交量指标
├── src-ui/                     # Vue 3 前端
│   └── src/
│       ├── components/
│       │   ├── DataSyncPanel.vue  # 数据同步面板
│       │   ├── ChartToolbar.vue   # 图表工具栏
│       │   ├── StockSearch.vue    # 股票搜索
│       │   └── SettingsPanel.vue  # 设置面板
│       ├── composables/        # 组合式函数
│       └── types/              # TypeScript 类型定义
├── scripts/
│   └── download_kline.py      # Python 数据下载辅助脚本
└── ARCHITECTURE.md             # 详细架构设计文档
```

## 🔄 数据源

五大 A 股数据源自动协同，主源失败自动切换备源：

| 数据源 | 级别支持 | 特点 | 最大条数 |
|--------|----------|------|----------|
| 东方财富 | 1分~日线 | 分钟级最优 | 10,000 |
| 新浪财经 | 1分~日线 | 速度快 | 2,000 |
| 腾讯财经 | 1分~周线 | 稳定 | 2,000 |
| Tushare | 1分~月线 | 数据质量最高 | — |
| 网易财经 | 日线~月线 | 历史数据 | — |

**同步策略**：
- 分钟级：东方财富 → 新浪 → 腾讯 → Tushare
- 日线/周线/月线：新浪 → 腾讯 → Tushare → 网易 → 东方财富
- 周线/月线：优先直接获取，失败则从日线重采样确保完整性

## 🏷️ 板块分类

| 板块 | 代码规则 | 说明 |
|------|----------|------|
| 🔴 上证主板 | 600/601/603/605 | 上海交易所主板 |
| 🔵 深证主板 | 000/001 | 深圳交易所主板 |
| 🟠 创业板 | 300/301 | 创业板市场 |
| 🟣 科创板 | 688/689 | 上海科创板 |
| 🌐 全 A 股 | 全部 | 所有 A 股 |

## 🚀 快速开始

### 环境要求

- [Rust](https://rustup.rs/) (1.75+)
- [Node.js](https://nodejs.org/) (18+)
- [Tauri 2.0 CLI](https://tauri.app/start/prerequisites/)

### 安装与运行

```bash
# 克隆仓库
git clone https://github.com/your-repo/yifangmoyan.git
cd yifangmoyan

# 安装前端依赖
cd src-ui && npm install && cd ..

# 开发模式运行
cd src-tauri && cargo tauri dev

# 或使用 Tauri CLI
npx tauri dev
```

### 编译发布

```bash
# 构建生产版本
cargo tauri build --release

# 编译产物位置 (macOS)
# App:     src-tauri/target/release/bundle/macos/墨岩K线.app
# 可执行:  src-tauri/target/release/墨岩K线
# DMG:     src-tauri/target/release/bundle/dmg/墨岩K线_0.1.0_aarch64.dmg
```

## 📊 数据校验

系统提供三层数据校验：

1. **OHLC 逻辑校验** — 检查开高低收的逻辑一致性（High ≥ Open/Close/Low 等）
2. **缺口检测** — 识别交易日历中的异常缺口
3. **跨数据源交叉校验** — 新浪与腾讯数据对比，发现差异超过 0.5% 的记录

校验结果按 0~1 评分，1.0 为完美数据。

## 🗺️ 开发路线

- [x] **Phase 1**: 项目骨架 + 数据层 + K 线图表
- [x] **Phase 1.5**: 多数据源同步 + 数据校验 + 板块管理
- [ ] **Phase 2**: 缠论核心（去包含、分型、笔、线段、中枢、背驰、买卖点）
- [ ] **Phase 3**: 威科夫量价（阶段识别、形态标注、趋势线）
- [ ] **Phase 4**: UI 完善 + 自选股 + 股票搜索（拼音码）+ 打包发布

## 📄 许可证

[MIT License](LICENSE)

## 🙏 致谢

- [TradingView lightweight-charts](https://github.com/nicegui/lightweight-charts) — 专业金融图表库
- [Tauri](https://tauri.app/) — 轻量级桌面应用框架
- [Polars](https://pola.rs/) — 高性能 DataFrame 库
- [缠中说禅](https://zh.wikipedia.org/wiki/缠论) — 缠论技术分析理论
- [Richard D. Wyckoff](https://en.wikipedia.org/wiki/Richard_D._Wyckoff) — 威科夫量价理论
