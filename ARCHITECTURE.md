# 墨岩 K 线分析系统 — 架构设计

> 类同花顺的 Mac 桌面端 A 股 K 线分析应用，集成缠论 + 威科夫量价分析

## 1. 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| 桌面框架 | Tauri 2.0 | Rust 后端 + WebView 前端，体积小性能高 |
| 后端语言 | Rust | 全部核心逻辑用 Rust 实现 |
| 前端框架 | Vue 3 + TypeScript | 响应式 UI |
| K 线图表 | lightweight-charts (TradingView) | 专业金融图表，支持缩放/十字线/覆盖层 |
| 样式 | Tailwind CSS 4 | 快速构建类同花顺深色主题 |
| 数据源 | 本地 Parquet + AKShare API | 参考 moyan-project 的数据架构 |

## 2. 整体架构

```
┌──────────────────────────────────────────────────────────────┐
│                     Tauri 2.0 Desktop App                    │
├──────────────────────────┬───────────────────────────────────┤
│   Vue 3 Frontend (TS)    │      Rust Backend                 │
│                          │                                   │
│  ┌─────────────────┐     │  ┌─────────────────────────────┐  │
│  │ KLineChart      │◀──▶  │  │  data (crate)              │  │
│  │  - K线主图      │  IPC  │  │   - Parquet/KLine读取      │  │
│  │  - 成交量副图   │     │  │   - 多级别K线合成            │  │
│  │  - MACD副图     │     │  │   - 缓存管理               │  │
│  ├─────────────────┤     │  ├─────────────────────────────┤  │
│  │ CzscOverlay     │◀──▶  │  │  czsc (crate)              │  │
│  │  - 笔/线段标注  │  IPC  │  │   - 分型/笔/线段           │  │
│  │  - 中枢区域     │     │  │   - 中枢识别               │  │
│  │  - 买卖点标记   │     │  │   - 背驰检测               │  │
│  ├─────────────────┤     │  │   - 买卖点                 │  │
│  │ WyckoffOverlay  │◀──▶  │  ├─────────────────────────────┤  │
│  │  - 趋势线      │  IPC  │  │  wyckoff (crate)           │  │
│  │  - TR/冰线     │     │  │   - 阶段识别               │  │
│  │  - LPS/JOC等   │     │  │   - 关键标注               │  │
│  ├─────────────────┤     │  │   - 量价分析               │  │
│  │ ChartToolbar    │     │  ├─────────────────────────────┤  │
│  │ StockSearch     │     │  │  indicator (crate)          │  │
│  │ TFSelector      │     │  │   - MACD                   │  │
│  │ SettingsPanel   │     │  │   - 成交量分析             │  │
│  └─────────────────┘     │  └─────────────────────────────┘  │
└──────────────────────────┴───────────────────────────────────┘
```

## 3. Rust Crate 划分

### 3.1 `yifang-data` — 数据层
- `types.rs`: KLine, TimeFrame, StockInfo 等基础类型
- `source.rs`: DataSource trait，统一数据源抽象
- `parquet_reader.rs`: 读取 moyan-project 的 parquet 文件
- `akshare.rs`: AKShare API 数据源（在线更新）
- `kline_manager.rs`: K 线管理，多级别合成（1F→5F→15F→30F→60F→日→周→月）
- `cache.rs`: SQLite 本地缓存

### 3.2 `yifang-czsc` — 缠论核心
- `types.rs`: FX(分型), BI(笔), XD(线段), ZS(中枢), BuySellPoint 等
- `include.rs`: 去除包含关系（参考 czsc remove_include）
- `fenxing.rs`: 分型识别（check_fx/check_fxs）
- `bi.rs`: 笔的构建（check_bi）
- `xd.rs`: 线段分析（特征序列+破坏规则）
- `zs.rs`: 中枢识别（笔中枢、线段中枢）
- `beichi.rs`: 背驰检测（MACD/力度/区间套）
- `buy_sell.rs`: 三类买卖点识别

### 3.3 `yifang-wyckoff` — 威科夫量价
- `types.rs`: WyckoffPhase, WyckoffEvent, TrendLine 等
- `phase.rs`: 阶段识别（吸筹/拉升/派发/下跌）
- `pattern.rs`: 关键形态（TR, Spring, JOC, LPS, UTAD 等）
- `annotation.rs`: 标注生成（趋势线/冰线/支撑阻力）
- `volume.rs`: 量价分析（OBV/量增价跌/量缩价涨）

### 3.4 `yifang-indicator` — 技术指标
- `macd.rs`: MACD 计算
- `volume.rs`: 成交量相关指标

### 3.5 `yifang-app` — Tauri 应用入口
- `commands/`: Tauri IPC 命令
- `state.rs`: 应用状态管理

## 4. 前端组件架构

### 4.1 页面布局（参考同花顺）
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

### 4.2 核心组件
- `KLineChart.vue`: 基于 lightweight-charts 的主 K 线图
- `VolumePanel.vue`: 成交量柱状图
- `MacdPanel.vue`: MACD 指标
- `CzscOverlay.vue`: 缠论覆盖层（笔线段/中枢/买点）
- `WyckoffOverlay.vue`: 威科夫覆盖层
- `ChartToolbar.vue`: 时间周期切换 + 指标勾选
- `StockSearch.vue`: 股票搜索（拼音/代码/名称）
- `FavList.vue`: 自选股列表

## 5. 核心数据流

```
用户选择股票 + 级别
       │
       ▼
Tauri Command: get_kline(symbol, timeframe)
       │
       ▼
yifang-data: 读取/获取 K 线数据
       │
       ├──▶ yifang-czsc: 缠论分析 → 笔/线段/中枢/买卖点
       ├──▶ yifang-wyckoff: 威科夫分析 → 阶段/标注
       ├──▶ yifang-indicator: MACD 计算
       │
       ▼
聚合为 ChartData 返回前端
       │
       ▼
前端渲染: K线 + 覆盖层(按勾选显示)
```

## 6. 工作分解（WBS）

### Phase 1: 项目骨架 + 数据层（本次）
- [x] Tauri 2.0 项目初始化
- [x] Rust workspace 搭建（4 个 crate）
- [x] yifang-data: 基础类型 + Parquet 读取
- [x] yifang-indicator: MACD 计算
- [x] Tauri IPC 基础命令
- [x] Vue 3 前端骨架 + K 线图表

### Phase 2: 缠论核心
- [ ] yifang-czsc: 去包含 + 分型 + 笔
- [ ] yifang-czsc: 线段 + 中枢
- [ ] yifang-czsc: 背驰 + 买卖点
- [ ] 前端缠论覆盖层

### Phase 3: 威科夫量价
- [ ] yifang-wyckoff: 阶段识别 + 形态
- [ ] yifang-wyckoff: 趋势线 + 标注
- [ ] 前端威科夫覆盖层

### Phase 4: UI 完善 + 集成
- [ ] 自选股管理
- [ ] 股票搜索（拼音码）
- [ ] 线段选中 → 次级别走势（架构预留）
- [ ] 打包发布

## 7. 线段→次级别走势（架构预留）

```rust
/// 点击某一线段时，前端发送:
/// get_sub_level_kline(symbol, xd_start_dt, xd_end_dt, sub_timeframe)
/// 后端返回该时间段的次级别K线 + 缠论分析结果
/// 
/// 例: 选中日线某线段 → 加载该时段的 60F K线 + 笔/中枢
```

## 8. 缠论与威科夫勾选机制

前端独立维护勾选状态：
```typescript
interface AnalysisSettings {
  czsc: {
    showBi: boolean;        // 显示笔
    showXd: boolean;        // 显示线段
    showBiZs: boolean;      // 显示笔中枢
    showXdZs: boolean;      // 显示线段中枢
    showBuySell: boolean;   // 显示买卖点
    showBeichi: boolean;    // 显示背驰
  };
  wyckoff: {
    showTrendLines: boolean;  // 趋势线
    showTR: boolean;          // 交易区间
    showIceLine: boolean;     // 冰线
    showLPS: boolean;         // LPS
    showJOC: boolean;         // JOC
    showSpring: boolean;      // Spring
    showUTAD: boolean;        // UTAD
  };
}
```

后端始终计算完整结果，前端根据勾选项选择性渲染覆盖层。
