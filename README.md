# 墨岩K线 — A股K线分析系统

> 类同花顺的 Mac 桌面端 A 股 K 线分析应用，集成 **缠论** + **威科夫量价分析** + **融合解读**

## ✨ 功能特性

- 📊 **专业 K 线图表** — 基于 TradingView lightweight-charts，支持缩放、十字线、8级时间周期
- 🔴 **缠论分析** — 严格对齐缠论原著：去包含→分型→笔→线段→中枢→背驰→三类买卖点→走势递归→区间套
- 🟣 **威科夫分析** — 严格对齐威科夫三大法则：供需线→交易区间→11种事件→5子阶段→努力与结果
- ⚡ **融合解读** — 缠论买卖点×威科夫事件自动关联，11条精确规则+通用匹配，1-5星信号强度
- 📈 **技术指标** — MACD、成交量分析
- 🔄 **多数据源同步** — 东方财富、新浪、腾讯、Tushare 四大数据源自动协同与降级
- 🏷️ **板块管理** — 上证主板/深证主板/创业板/科创板分类统计与按板块同步
- ✅ **数据校验** — OHLC 逻辑校验、缺口检测、跨数据源交叉验证
- 🎨 **深色主题** — 类同花顺专业深色 UI
- ⌨️ **快捷键** — 0/B/W/F 视图切换、1-8 周期切换、/ 搜索、Cmd+S 自选

## 🖥️ 界面预览

```
┌──────────────────────────────────────────────────────────┐
│  墨岩K线  [纯K线][缠论][威科夫][融合]  🔍搜索  ★自选    │
├──────┬───────────────────────────────────────────────────┤
│      │  月|周|日|60F|30F|15F|5F|1F  笔 线段 买卖 背驰   │
│ 自选 ├───────────────────────────────────────────────────┤
│ 股列表│  K线主图区 + 缠论笔/线段/中枢/买卖点/背驰       │
│      │  + 威科夫阶段/TR/冰线/供需线/事件标签            │
│      │  + 融合★标注                                      │
│      ├───────────────────────────────────────────────────┤
│      │  右侧面板: 当前阶段 + 信号摘要                    │
│      │  缠论信号 | 威科夫信号 | ⚡融合解读               │
│      │  ──────────────────────────────────               │
│      │  勾选: 分型/笔/线段/中枢/买卖/背驰               │
│      │  阶段/TR/冰线/供需线/SC/AR/ST/Spring...          │
└──────┴───────────────────────────────────────────────────┘
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
│   └── src/
│       ├── commands.rs         # Tauri IPC 命令（前后端通信）
│       ├── fusion.rs           # 缠论+威科夫融合引擎
│       ├── state.rs            # 应用状态管理
│       └── lib.rs              # 应用初始化与插件注册
├── crates/
│   ├── data/                   # yifang-data — 数据层
│   │   └── src/
│   │       ├── types.rs        # 全局类型（KLine/TimeFrame/CzscResult/WyckoffResult/FusionResult...）
│   │       ├── source.rs       # DataSource trait，统一数据源抽象
│   │       ├── kline_manager.rs # K 线管理，多级别合成
│   │       └── sync.rs         # 多数据源协同同步 + 数据校验
│   ├── czsc/                   # yifang-czsc — 缠论核心
│   │   └── src/
│   │       ├── include.rs      # 去包含关系（对齐czsc 0.9.9）
│   │       ├── fenxing.rs      # 分型识别（check_fxs）
│   │       ├── bi.rs           # 笔的构建（check_bi）
│   │       ├── xd.rs           # 线段分析（特征序列法）
│   │       ├── zs.rs           # 中枢识别（zg/zd修正）
│   │       ├── beichi.rs       # 背驰检测（趋势+盘整背驰）
│   │       ├── buy_sell.rs     # 三类买卖点
│   │       ├── zoushi.rs       # 走势递归分解
│   │       ├── qujian_tao.rs   # 区间套信号
│   │       └── analyzer.rs     # 缠论分析入口
│   ├── wyckoff/                # yifang-wyckoff — 威科夫量价
│   │   └── src/
│   │       ├── pattern.rs      # 11种事件识别（SC/AR/ST/Spring/SOS/LPS/JOC/PSY/BC/UTAD/SOW/LPSY）
│   │       ├── phase.rs        # 阶段识别（5子阶段）
│   │       ├── trading_range.rs # 交易区间（事件驱动+自动合并）
│   │       ├── supply_demand.rs # 供需线（供给线+需求线+突破判断）
│   │       ├── effort.rs       # 努力与结果分析（威科夫第三法则）
│   │       ├── annotation.rs   # 标注生成
│   │       └── analyzer.rs     # 威科夫分析入口
│   └── indicator/              # yifang-indicator — 技术指标
│       └── src/
│           ├── macd.rs         # MACD 计算
│           └── volume.rs       # 成交量指标
├── src-ui/                     # Vue 3 前端
│   └── src/
│       ├── App.vue             # 主界面（图表渲染+覆盖层+交互）
│       ├── components/
│       │   ├── ChartToolbar.vue   # 图表工具栏
│       │   ├── DataSyncPanel.vue  # 数据同步面板
│       │   ├── SignalPanel.vue    # 信号摘要面板
│       │   ├── SettingsPanel.vue  # 勾选设置面板
│       │   ├── StockSearch.vue    # 股票搜索（拼音防抖）
│       │   └── WatchlistPanel.vue # 自选股列表
│       ├── composables/
│       │   ├── useApi.ts       # Tauri IPC API 封装
│       │   └── useStorage.ts   # localStorage 持久化
│       └── types/
│           └── index.ts        # TypeScript 类型（对齐后端）
└── scripts/
    └── download_kline.py       # Python 数据下载辅助脚本
```

## 🔄 数据源

四大 A 股数据源自动协同，主源失败自动切换备源：

| 数据源 | 级别支持 | 特点 | 最大条数 |
|--------|----------|------|----------|
| 东方财富 | 1分~日线 | 分钟级最优 | 10,000 |
| 新浪财经 | 1分~日线 | 速度快 | 2,000 |
| 腾讯财经 | 1分~周线 | 稳定 | 2,000 |
| Tushare | 1分~月线 | 数据质量最高 | — |

**同步策略**：
- 分钟级：东方财富 → 新浪 → 腾讯 → Tushare
- 日线/周线/月线：新浪 → 腾讯 → Tushare → 东方财富
- 周线/月线：优先直接获取，失败则从日线重采样确保完整性

## 🏷️ 板块分类

| 板块 | 代码规则 | 说明 |
|------|----------|------|
| 🔴 上证主板 | 600/601/603/605 | 上海交易所主板 |
| 🔵 深证主板 | 000/001 | 深圳交易所主板 |
| 🟠 创业板 | 300/301 | 创业板市场 |
| 🟣 科创板 | 688/689 | 上海科创板 |
| 🌐 全 A 股 | 全部 | 所有 A 股 |

## ⌨️ 快捷键

| 快捷键 | 功能 |
|--------|------|
| `0` | 纯K线模式 |
| `B` | 缠论模式 |
| `W` | 威科夫模式 |
| `F` | 融合模式 |
| `1`-`8` | 切换周期：月/周/日/60F/30F/15F/5F/1F |
| `/` | 聚焦搜索框 |
| `Cmd+S` | 添加到自选股 |

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
- [x] **Phase 2**: 缠论核心（去包含→分型→笔→线段→中枢→背驰→买卖点→走势递归→区间套）
- [x] **Phase 3**: 威科夫量价（三大法则→11事件→5子阶段→供需线→努力结果）
- [x] **Phase 4**: 缠论+威科夫融合解读（11条精确规则+1-5星信号强度）
- [x] **Phase 5**: 体验打磨（自选股/拼音搜索/快捷键/视图模式/次级别/持久化）
- [ ] **Phase 6**: 发布准备（性能压测/macOS签名/DMG打包/用户文档）

## 📄 许可证

[MIT License](LICENSE)

## 🙏 致谢

- [TradingView lightweight-charts](https://github.com/nicegui/lightweight-charts) — 专业金融图表库
- [Tauri](https://tauri.app/) — 轻量级桌面应用框架
- [Polars](https://pola.rs/) — 高性能 DataFrame 库
- [czsc 0.9.9](https://github.com/zengbin93/czsc) — 缠论 Python 参考实现
- 缠中说禅 — 缠论技术分析理论
- Richard D. Wyckoff — 威科夫量价理论
