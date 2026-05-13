<script setup lang="ts">
import { ref, onMounted, watch, computed, nextTick } from "vue";
import {
  createChart,
  ColorType,
  type IChartApi,
  type ISeriesApi,
  type CandlestickData,
  type HistogramData,
  type LineData,
  type Time,
} from "lightweight-charts";
import { getChartData, searchStocks, getAllStockCodes, getSubLevelData, autoSyncOnStartup, getSyncStatus } from "./composables/useApi";
import type { SyncProgress } from "./composables/useApi";
import {
  type ChartData,
  type AnalysisSettings,
  type TimeFrame,
  type ViewMode,
  DEFAULT_SETTINGS,
  VIEW_MODE_SETTINGS,
  CZSC_BS_COLORS,
  WYCKOFF_EVENT_COLORS,
  WYCKOFF_BULLISH_EVENTS,
} from "./types";
import ChartToolbar from "./components/ChartToolbar.vue";
import StockSearch from "./components/StockSearch.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import DataSyncPanel from "./components/DataSyncPanel.vue";
import SignalPanel from "./components/SignalPanel.vue";
import WatchlistPanel from "./components/WatchlistPanel.vue";
import { useWatchlist, usePersistedSettings } from "./composables/useStorage";

// ===== 状态 =====
const symbol = ref("000001");
const timeframe = ref<TimeFrame>("d");
const chartData = ref<ChartData | null>(null);
const loading = ref(false);
const error = ref("");
const settings = ref<AnalysisSettings>({ ...DEFAULT_SETTINGS });
const currentView = ref<"chart" | "sync">("chart");
const viewMode = ref<ViewMode>("czsc");
const searchKeyword = ref("");
const searchResults = ref<any[]>([]);
const showSearch = ref(false);

// ===== 后台同步状态 =====
const bgSyncProgress = ref<SyncProgress | null>(null);
let bgSyncTimer: ReturnType<typeof setInterval> | null = null;
const bgSyncElapsed = ref(0);
let bgSyncStartTs = 0;

// ===== 自选股 & 持久化 =====
const { addToWatchlist, isInWatchlist } = useWatchlist();
const persistedSettings = usePersistedSettings<AnalysisSettings>("analysis", DEFAULT_SETTINGS);
// 初始化设置从持久化
settings.value = JSON.parse(JSON.stringify(persistedSettings.value));
const persistedViewMode = usePersistedSettings<ViewMode>("viewMode", "czsc");
viewMode.value = persistedViewMode.value;
const persistedTf = usePersistedSettings<TimeFrame>("timeframe", "d");
timeframe.value = persistedTf.value;
const persistedSymbol = usePersistedSettings<string>("symbol", "000001");
symbol.value = persistedSymbol.value;

// ===== 图表引用 =====
const chartContainer = ref<HTMLDivElement>();
let mainChart: IChartApi | null = null;
let candleSeries: ISeriesApi<"Candlestick"> | null = null;
let volumeSeries: ISeriesApi<"Histogram"> | null = null;

// ===== 信息弹窗 =====
const tooltipInfo = ref<{
  type: string;
  data: any;
  x: number;
  y: number;
} | null>(null);

// ===== 次级别走势面板 =====
const subLevelPanel = ref<{
  xd: any;
  data: ChartData | null;
  loading: boolean;
} | null>(null);

// ===== 计算属性 =====
const currentPrice = computed(() => {
  if (!chartData.value || chartData.value.klines.length === 0) return null;
  const last = chartData.value.klines[chartData.value.klines.length - 1];
  return last;
});

const priceChange = computed(() => {
  if (!currentPrice.value) return null;
  const klines = chartData.value!.klines;
  if (klines.length < 2) return null;
  const prev = klines[klines.length - 2];
  const curr = currentPrice.value!;
  return {
    change: curr.close - prev.close,
    changePct: ((curr.close - prev.close) / prev.close) * 100,
  };
});

// ===== 数据加载 =====
async function loadData() {
  loading.value = true;
  error.value = "";
  try {
    const data = await getChartData(
      symbol.value,
      timeframe.value,
      true,   // 始终获取缠论数据，以便设置面板控制显示
      true    // 始终获取威科夫数据，以便设置面板控制显示
    );
    chartData.value = data;
    await nextTick();
    renderChart();
  } catch (e: any) {
    error.value = e.toString();
  } finally {
    loading.value = false;
  }
}

function hasAnyCzscEnabled(): boolean {
  return Object.values(settings.value.czsc).some(Boolean);
}

// ===== 时间格式化 =====
function toTime(dt: string): Time {
  const match = dt.match(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2})/);
  if (match && match[2] !== "00:00") {
    return `${match[1]} ${match[2]}` as Time;
  }
  return dt.slice(0, 10) as Time;
}

// ===== 图表渲染 =====
const MAX_VISIBLE_KLINES = 5000; // 性能阈值：超过此数量截断

function renderChart() {
  if (!chartData.value || !chartContainer.value) return;
  const data = chartData.value;

  const containerWidth = chartContainer.value.clientWidth;
  const containerHeight = chartContainer.value.clientHeight;
  if (containerWidth === 0 || containerHeight === 0) {
    requestAnimationFrame(() => renderChart());
    return;
  }

  if (mainChart) {
    mainChart.remove();
    mainChart = null;
  }

  mainChart = createChart(chartContainer.value, {
    layout: {
      background: { type: ColorType.Solid, color: "#1a1a2e" },
      textColor: "#9e9e9e",
      fontSize: 11,
    },
    grid: {
      vertLines: { color: "#2a2a4a" },
      horzLines: { color: "#2a2a4a" },
    },
    crosshair: {
      mode: 0,
      vertLine: { color: "#e94560", width: 1, style: 2 },
      horzLine: { color: "#e94560", width: 1, style: 2 },
    },
    rightPriceScale: {
      borderColor: "#2a2a4a",
      scaleMargins: { top: 0.05, bottom: 0.3 },
    },
    timeScale: {
      borderColor: "#2a2a4a",
      timeVisible: true,
      secondsVisible: false,
    },
    width: chartContainer.value.clientWidth,
    height: chartContainer.value.clientHeight,
  });

  // K 线数据（性能优化：超过阈值截断）
  const startIdx = data.klines.length > MAX_VISIBLE_KLINES
    ? data.klines.length - MAX_VISIBLE_KLINES
    : 0;
  const visibleKlines = data.klines.slice(startIdx);
  // offset 用于后续剪裁缠论/威科夫 overlay 的 index 对齐

  const candleData: CandlestickData<Time>[] = visibleKlines.map((k) => ({
    time: toTime(k.dt),
    open: k.open,
    high: k.high,
    low: k.low,
    close: k.close,
  }));

  candleSeries = mainChart.addCandlestickSeries({
    upColor: "#ef5350",
    downColor: "#26a69a",
    borderUpColor: "#ef5350",
    borderDownColor: "#26a69a",
    wickUpColor: "#ef5350",
    wickDownColor: "#26a69a",
  });
  candleSeries.setData(candleData);

  // 成交量
  const volumeData: HistogramData<Time>[] = data.klines.map((k) => ({
    time: toTime(k.dt),
    value: k.vol,
    color: k.close >= k.open ? "rgba(239,83,80,0.4)" : "rgba(38,166,154,0.4)",
  }));

  volumeSeries = mainChart.addHistogramSeries({
    priceFormat: { type: "volume" },
    priceScaleId: "volume",
  });
  volumeSeries.setData(volumeData);
  mainChart.priceScale("volume").applyOptions({
    scaleMargins: { top: 0.75, bottom: 0.55 },
  });

  // MACD 副图
  if (settings.value.chart.showMacd && data.macd && data.macd.dif.length > 0) {
    const macdData = data.macd;

    // DIF 线
    const difSeries = mainChart.addLineSeries({
      color: "#2196f3",
      lineWidth: 1,
      priceLineVisible: false,
      lastValueVisible: false,
      crosshairMarkerVisible: false,
      priceScaleId: "macd",
    });
    const difLineData: LineData<Time>[] = [];
    for (let i = 0; i < macdData.dif.length && i < data.klines.length; i++) {
      difLineData.push({ time: toTime(data.klines[i].dt), value: macdData.dif[i] });
    }
    difSeries.setData(difLineData);

    // DEA 线
    const deaSeries = mainChart.addLineSeries({
      color: "#ff9800",
      lineWidth: 1,
      priceLineVisible: false,
      lastValueVisible: false,
      crosshairMarkerVisible: false,
      priceScaleId: "macd",
    });
    const deaLineData: LineData<Time>[] = [];
    for (let i = 0; i < macdData.dea.length && i < data.klines.length; i++) {
      deaLineData.push({ time: toTime(data.klines[i].dt), value: macdData.dea[i] });
    }
    deaSeries.setData(deaLineData);

    // MACD 柱状图
    const macdHistSeries = mainChart.addHistogramSeries({
      priceFormat: { type: "price", precision: 3, minMove: 0.001 },
      priceLineVisible: false,
      lastValueVisible: false,
      priceScaleId: "macd",
    });
    const macdHistData: HistogramData<Time>[] = [];
    for (let i = 0; i < macdData.macd_hist.length && i < data.klines.length; i++) {
      const v = macdData.macd_hist[i];
      macdHistData.push({
        time: toTime(data.klines[i].dt),
        value: v,
        color: v >= 0 ? "rgba(239,83,80,0.6)" : "rgba(38,166,154,0.6)",
      });
    }
    macdHistSeries.setData(macdHistData);

    // MACD 副图位置：底部 45% 区域
    mainChart.priceScale("macd").applyOptions({
      scaleMargins: { top: 0.6, bottom: 0 },
    });
  }

  // 缠论覆盖层
  if (data.czsc) {
    renderCzscOverlays(data);
  }

  // 威科夫覆盖层
  if (data.wyckoff) {
    renderWyckoffOverlays(data);
  }

  // 融合标记
  if (data.fusion && settings.value.fusion.showFusion) {
    renderFusionOverlays(data);
  }

  mainChart.timeScale().fitContent();

  // 悬停事件
  mainChart.subscribeCrosshairMove((param) => {
    if (!param.time || !param.point) {
      tooltipInfo.value = null;
      return;
    }
    // 查找该位置的缠论/威科夫数据
    const idx = data.klines.findIndex((k) => toTime(k.dt) === param.time);
    if (idx < 0) {
      tooltipInfo.value = null;
      return;
    }

    let info: any = null;

    // 查找笔
    if (data.czsc) {
      const bi = data.czsc.bi.find(
        (b) => idx >= b.start_index && idx <= b.end_index
      );
      if (bi) {
        info = { type: "bi", data: bi, x: param.point.x, y: param.point.y };
      }

      // 查找线段
      const xd = data.czsc.xd.find(
        (x) => idx >= x.start_index && idx <= x.end_index
      );
      if (xd && !info) {
        info = { type: "xd", data: xd, x: param.point.x, y: param.point.y };
      }

      // 查找中枢
      const zs = [...data.czsc.bi_zs, ...data.czsc.xd_zs].find(
        (z) => idx >= z.start_index && idx <= z.end_index
      );
      if (zs && !info) {
        info = { type: "zs", data: zs, x: param.point.x, y: param.point.y };
      }

      // 查找买卖点
      const bs = data.czsc.buy_sell.find((b) => b.index === idx);
      if (bs && !info) {
        info = { type: "bs", data: bs, x: param.point.x, y: param.point.y };
      }
    }

    // 查找威科夫事件
    if (data.wyckoff) {
      const evt = data.wyckoff.events.find((e) => e.index === idx);
      if (evt) {
        info = info || { type: "wyckoff", data: evt, x: param.point.x, y: param.point.y };
      }
    }

    tooltipInfo.value = info;
  });
}

// ===== 缠论覆盖层 =====
function renderCzscOverlays(data: ChartData) {
  const czsc = data.czsc!;
  const allMarkers: any[] = [];

  // 分型标记 — 小三角
  if (settings.value.czsc.showFenxing && czsc.fenxing.length > 0) {
    for (const fx of czsc.fenxing) {
      const k = data.klines[fx.index];
      if (!k) continue;
      allMarkers.push({
        time: toTime(k.dt),
        position: fx.fx_type === "top" ? ("aboveBar" as const) : ("belowBar" as const),
        color: fx.fx_type === "top" ? "#4caf50" : "#ffc107",
        shape: fx.fx_type === "top" ? ("arrowUp" as const) : ("arrowDown" as const),
        size: 0.5,
        text: "",
      });
    }
  }

  // 笔 — 上升红/下降蓝 折线
  if (settings.value.czsc.showBi && czsc.bi.length > 0) {
    const biSeries = mainChart!.addLineSeries({
      color: "#4a90d9",
      lineWidth: 2,
      priceLineVisible: false,
      lastValueVisible: false,
      crosshairMarkerVisible: false,
    });

    const biData: LineData<Time>[] = [];
    for (const bi of czsc.bi) {
      const startK = data.klines[bi.start_index];
      const endK = data.klines[Math.min(bi.end_index, data.klines.length - 1)];
      if (startK && endK) {
        const startTime = toTime(startK.dt);
        if (biData.length === 0 || biData[biData.length - 1].time !== startTime) {
          biData.push({ time: startTime, value: bi.start_price });
        }
        biData.push({ time: toTime(endK.dt), value: bi.end_price });
      }
    }
    biSeries.setData(biData);
  }

  // 线段 — 3px 虚线
  if (settings.value.czsc.showXd && czsc.xd.length > 0) {
    const xdSeries = mainChart!.addLineSeries({
      color: "#b388ff",
      lineWidth: 3,
      lineStyle: 2,
      priceLineVisible: false,
      lastValueVisible: false,
      crosshairMarkerVisible: false,
    });

    const xdData: LineData<Time>[] = [];
    for (const xd of czsc.xd) {
      const startK = data.klines[xd.start_index];
      const endK = data.klines[Math.min(xd.end_index, data.klines.length - 1)];
      if (startK && endK) {
        const startTime = toTime(startK.dt);
        if (xdData.length === 0 || xdData[xdData.length - 1].time !== startTime) {
          xdData.push({ time: startTime, value: xd.start_price });
        }
        xdData.push({ time: toTime(endK.dt), value: xd.end_price });
      }
    }
    xdSeries.setData(xdData);
  }

  // 买卖点标记 — 圆形图标+文字
  if (settings.value.czsc.showBuySell && czsc.buy_sell.length > 0) {
    for (const bs of czsc.buy_sell) {
      const k = data.klines[bs.index];
      if (!k) continue;
      const isBuy = bs.bs_type.includes("buy");
      const bsConf = CZSC_BS_COLORS[bs.bs_type] || {
        color: isBuy ? "#00e676" : "#ff1744",
        text: bs.bs_type,
      };
      allMarkers.push({
        time: toTime(k.dt),
        position: isBuy ? ("belowBar" as const) : ("aboveBar" as const),
        color: bsConf.color,
        shape: "circle" as const,
        size: 2,
        text: bsConf.text,
      });
    }
  }

  // 笔中枢 — 半透明矩形用上下沿线模拟
  if (settings.value.czsc.showBiZs && czsc.bi_zs.length > 0) {
    renderZhongShu(czsc.bi_zs, data, "rgba(179,136,255,0.5)");
  }

  // 线段中枢 — 半透明橙色
  if (settings.value.czsc.showXdZs && czsc.xd_zs.length > 0) {
    renderZhongShu(czsc.xd_zs, data, "rgba(255,152,0,0.5)");
  }

  // 背驰标记
  if (settings.value.czsc.showBeichi && czsc.beichi.length > 0) {
    for (const bc of czsc.beichi) {
      const k = data.klines[bc.index];
      if (!k) continue;
      const isUp = bc.direction === "up";
      let text = "⚡";
      if (bc.bc_sub_type === "panzheng") text = "⚡盘整";
      else if (bc.bc_type === "xd_beichi") text = "⚡线段";
      else text = "⚡笔";
      allMarkers.push({
        time: toTime(k.dt),
        position: isUp ? ("aboveBar" as const) : ("belowBar" as const),
        color: "#ff9800",
        shape: "circle" as const,
        size: 1,
        text,
      });
    }
  }

  // 设置所有 markers（按时间排序）
  if (allMarkers.length > 0) {
    candleSeries!.setMarkers(
      allMarkers.sort((a, b) => (a.time as string).localeCompare(b.time as string))
    );
  }
}

// 渲染中枢
function renderZhongShu(zsList: any[], data: ChartData, color: string) {
  for (const zs of zsList) {
    const startK = data.klines[zs.start_index];
    const endK = data.klines[Math.min(zs.end_index, data.klines.length - 1)];
    if (!startK || !endK) continue;

    // zg 上沿线
    const upperLine = mainChart!.addLineSeries({
      color,
      lineWidth: 1,
      lineStyle: 1,
      priceLineVisible: false,
      lastValueVisible: false,
      crosshairMarkerVisible: false,
    });
    upperLine.setData([
      { time: toTime(startK.dt), value: zs.zg },
      { time: toTime(endK.dt), value: zs.zg },
    ]);

    // zd 下沿线
    const lowerLine = mainChart!.addLineSeries({
      color,
      lineWidth: 1,
      lineStyle: 1,
      priceLineVisible: false,
      lastValueVisible: false,
      crosshairMarkerVisible: false,
    });
    lowerLine.setData([
      { time: toTime(startK.dt), value: zs.zd },
      { time: toTime(endK.dt), value: zs.zd },
    ]);
  }
}

// ===== 威科夫覆盖层 =====
function renderWyckoffOverlays(data: ChartData) {
  const wyckoff = data.wyckoff!;
  const eventMarkers: any[] = [];

  // 阶段色带 — 用 priceLine 模拟
  if (settings.value.wyckoff.showPhase && wyckoff.phase_labels.length > 0) {
    // 渲染阶段色带：用分段不同颜色的折线表示
    // 为每个阶段绘制一条短线段
    let prevPhase = "";
    let segStart = -1;
    const phaseColorMap: Record<string, string> = {
      Accumulation: "rgba(0,188,212,0.3)",
      Markup: "rgba(76,175,80,0.3)",
      Distribution: "rgba(156,39,176,0.3)",
      Markdown: "rgba(120,144,156,0.3)",
      Unknown: "rgba(66,66,66,0.1)",
    };

    // 找 K 线的最高价，作为色带位置
    const maxHigh = Math.max(...data.klines.map((k) => k.high));

    for (let i = 0; i < wyckoff.phase_labels.length; i++) {
      const lbl = wyckoff.phase_labels[i];
      if (lbl.phase !== prevPhase) {
        if (prevPhase && segStart >= 0) {
          // 画之前的段
          drawPhaseLine(segStart, i - 1, prevPhase, maxHigh, data, phaseColorMap);
        }
        prevPhase = lbl.phase;
        segStart = i;
      }
    }
    if (prevPhase && segStart >= 0) {
      drawPhaseLine(segStart, wyckoff.phase_labels.length - 1, prevPhase, maxHigh, data, phaseColorMap);
    }
  }

  // 供需线
  if (settings.value.wyckoff.showSupplyDemand && wyckoff.supply_demand_lines.length > 0) {
    for (const sl of wyckoff.supply_demand_lines) {
      const startK = data.klines[sl.start_index];
      const endK = data.klines[Math.min(sl.end_index, data.klines.length - 1)];
      if (!startK || !endK) continue;
      const series = mainChart!.addLineSeries({
        color: sl.line_type === "supply" ? "rgba(255,23,68,0.7)" : "rgba(0,230,118,0.7)",
        lineWidth: 2,
        lineStyle: 0,
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
      });
      series.setData([
        { time: toTime(startK.dt), value: sl.start_price },
        { time: toTime(endK.dt), value: sl.end_price },
      ]);
    }
  }

  // 交易区间 + 冰线
  if (settings.value.wyckoff.showTR && wyckoff.trading_ranges.length > 0) {
    for (const tr of wyckoff.trading_ranges) {
      const startK = data.klines[tr.start_index];
      const endK = data.klines[Math.min(tr.end_index, data.klines.length - 1)];
      if (!startK || !endK) continue;

      // 上沿
      const upperLine = mainChart!.addLineSeries({
        color: "rgba(0,188,212,0.4)",
        lineWidth: 1,
        lineStyle: 0,
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
      });
      upperLine.setData([
        { time: toTime(startK.dt), value: tr.upper },
        { time: toTime(endK.dt), value: tr.upper },
      ]);

      // 下沿
      const lowerLine = mainChart!.addLineSeries({
        color: "rgba(0,188,212,0.4)",
        lineWidth: 1,
        lineStyle: 0,
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
      });
      lowerLine.setData([
        { time: toTime(startK.dt), value: tr.lower },
        { time: toTime(endK.dt), value: tr.lower },
      ]);

      // 冰线 — 虚线
      if (settings.value.wyckoff.showIceLine) {
        const iceLine = mainChart!.addLineSeries({
          color: "rgba(3,169,244,0.6)",
          lineWidth: 1,
          lineStyle: 2,
          priceLineVisible: false,
          lastValueVisible: false,
          crosshairMarkerVisible: false,
        });
        iceLine.setData([
          { time: toTime(startK.dt), value: tr.ice_line },
          { time: toTime(endK.dt), value: tr.ice_line },
        ]);
      }
    }
  }

  // 趋势线（兼容旧字段）
  if (wyckoff.trend_lines.length > 0) {
    for (const tl of wyckoff.trend_lines) {
      const startK = data.klines[tl.start_index];
      const endK = data.klines[Math.min(tl.end_index, data.klines.length - 1)];
      if (!startK || !endK) continue;
      const series = mainChart!.addLineSeries({
        color: tl.line_type === "support" ? "rgba(0,230,118,0.4)" : "rgba(255,23,68,0.4)",
        lineWidth: 1,
        lineStyle: 0,
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
      });
      series.setData([
        { time: toTime(startK.dt), value: tl.start_price },
        { time: toTime(endK.dt), value: tl.end_price },
      ]);
    }
  }

  // 威科夫事件 markers — 每种事件独立勾选
  const eventTypeSettingMap: Record<string, keyof AnalysisSettings["wyckoff"]> = {
    SC: "showSC",
    AR: "showAR",
    ST: "showST",
    Spring: "showSpring",
    SOS: "showSOS",
    LPS: "showLPS",
    JOC: "showJOC",
    PSY: "showPSY",
    BC: "showBC",
    UTAD: "showUTAD",
    SOW: "showSOW",
    LPSY: "showLPSY",
    Shakeout: "showSpring",
  };

  for (const e of wyckoff.events) {
    const settingKey = eventTypeSettingMap[e.event_type];
    if (!settingKey || !settings.value.wyckoff[settingKey]) continue;

    const k = data.klines[e.index];
    if (!k) continue;

    const isBullish = WYCKOFF_BULLISH_EVENTS.includes(e.event_type);
    const evtColor = WYCKOFF_EVENT_COLORS[e.event_type] || (isBullish ? "#00bcd4" : "#ff5722");

    eventMarkers.push({
      time: toTime(k.dt),
      position: isBullish ? ("belowBar" as const) : ("aboveBar" as const),
      color: evtColor,
      shape: "square" as const,
      size: 1,
      text: e.event_type,
    });
  }

  if (eventMarkers.length > 0) {
    const existingMarkers = candleSeries!.markers() || [];
    candleSeries!.setMarkers(
      [...existingMarkers, ...eventMarkers].sort((a, b) =>
        (a.time as string).localeCompare(b.time as string)
      )
    );
  }
}

// 画阶段色带辅助
function drawPhaseLine(
  startIdx: number,
  endIdx: number,
  phase: string,
  maxHigh: number,
  data: ChartData,
  colorMap: Record<string, string>
) {
  const startLabel = data.wyckoff!.phase_labels[startIdx];
  const endLabel = data.wyckoff!.phase_labels[endIdx];
  const startK = data.klines[startLabel.index];
  const endK = data.klines[endLabel.index];
  if (!startK || !endK) return;

  const series = mainChart!.addLineSeries({
    color: colorMap[phase] || "rgba(100,100,100,0.2)",
    lineWidth: 4,
    lineStyle: 0,
    priceLineVisible: false,
    lastValueVisible: false,
    crosshairMarkerVisible: false,
  });
  series.setData([
    { time: toTime(startK.dt), value: maxHigh * 1.02 },
    { time: toTime(endK.dt), value: maxHigh * 1.02 },
  ]);
}

// ===== 融合覆盖层 =====
function renderFusionOverlays(data: ChartData) {
  if (!data.fusion || data.fusion.signals.length === 0) return;

  const fusionMarkers = data.fusion.signals.map((sig) => {
    const k = data.klines[sig.index];
    const isBullish = sig.direction === "bullish";
    // 用特殊图标和文字
    const stars = "★".repeat(sig.strength);
    return {
      time: toTime(k?.dt || sig.dt),
      position: isBullish ? ("belowBar" as const) : ("aboveBar" as const),
      color: isBullish ? "#ffd700" : "#ff6d00",
      shape: "circle" as const,
      size: 3,
      text: `${stars} 融合`,
    };
  });

  const existingMarkers = candleSeries!.markers() || [];
  candleSeries!.setMarkers(
    [...existingMarkers, ...fusionMarkers].sort((a, b) =>
      (a.time as string).localeCompare(b.time as string)
    )
  );
}

// ===== 次级别走势 =====
async function loadSubLevel(xd: any) {
  if (!chartData.value) return;
  subLevelPanel.value = { xd, data: null, loading: true };
  try {
    const data = await getSubLevelData(
      symbol.value,
      timeframe.value,
      xd.start_dt,
      xd.end_dt,
      hasAnyCzscEnabled()
    );
    subLevelPanel.value = { xd, data, loading: false };
  } catch (e: any) {
    subLevelPanel.value = { xd, data: null, loading: false };
  }
}

function closeSubLevel() {
  subLevelPanel.value = null;
}

// ===== 搜索股票 =====
async function onSearch() {
  if (!searchKeyword.value.trim()) return;
  try {
    searchResults.value = await searchStocks(searchKeyword.value.trim());
    showSearch.value = true;
  } catch (e) {
    console.error(e);
  }
}

function selectStock(sym: string, name?: string) {
  symbol.value = sym;
  persistedSymbol.value = sym;
  showSearch.value = false;
  // 保留搜索框中显示的股票名称
  if (name) {
    searchKeyword.value = name;
  } else {
    // 从搜索结果中查找名称
    const found = searchResults.value.find((s: any) => s.symbol === sym);
    searchKeyword.value = found ? found.name : sym;
  }
  loadData();
}

// ===== 后台同步轮询 =====
function startBgSyncPolling() {
  if (bgSyncTimer) return;
  bgSyncStartTs = Date.now();
  bgSyncTimer = setInterval(async () => {
    try {
      const status = await getSyncStatus();
      bgSyncProgress.value = status;
      bgSyncElapsed.value = Math.floor((Date.now() - bgSyncStartTs) / 1000);
      // 同步完成后停止轮询
      if (!status.running) {
        if (bgSyncTimer) {
          clearInterval(bgSyncTimer);
          bgSyncTimer = null;
        }
      }
    } catch {
      // 轮询失败不影响主流程
    }
  }, 60_000);
}

// ===== 事件处理 =====
function onTimeframeChange(tf: TimeFrame) {
  timeframe.value = tf;
  persistedTf.value = tf;
  loadData();
}

function onSettingsChange(newSettings: AnalysisSettings) {
  settings.value = newSettings;
  persistedSettings.value = JSON.parse(JSON.stringify(newSettings)) as any;
  // 只重新渲染图表，不重新获取数据（设置变更不影响后端数据）
  renderChart();
}

function onViewModeChange(mode: ViewMode) {
  viewMode.value = mode;
  persistedViewMode.value = mode;
  settings.value = JSON.parse(JSON.stringify(VIEW_MODE_SETTINGS[mode]));
  persistedSettings.value = JSON.parse(JSON.stringify(settings.value)) as any;
  loadData();
}

// ===== 键盘快捷键 =====
function handleKeydown(e: KeyboardEvent) {
  // 不在输入框时才响应
  const target = e.target as HTMLElement;
  if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;

  // 1-8 切换周期
  const tfKeys: TimeFrame[] = ["m", "w", "d", "f60", "f30", "f15", "f5", "f1"];
  const num = parseInt(e.key);
  if (num >= 1 && num <= 8) {
    e.preventDefault();
    timeframe.value = tfKeys[num - 1];
    loadData();
    return;
  }

  // / 或 Cmd+F 聚焦搜索
  if (e.key === "/" || ((e.metaKey || e.ctrlKey) && e.key === "f")) {
    e.preventDefault();
    const input = document.querySelector<HTMLInputElement>("#stock-search-input");
    input?.focus();
    return;
  }

  // B 只看笔
  if (e.key === "b" || e.key === "B") {
    e.preventDefault();
    onViewModeChange("czsc");
    return;
  }

  // W 切换威科夫
  if (e.key === "w" || e.key === "W") {
    e.preventDefault();
    onViewModeChange("wyckoff");
    return;
  }

  // F 切换融合
  if (e.key === "f" || e.key === "F") {
    e.preventDefault();
    onViewModeChange("fusion");
    return;
  }

  // 0 纯K线
  if (e.key === "0") {
    e.preventDefault();
    onViewModeChange("pure");
    return;
  }

  // ↑↓ 缩放（同花顺风格）
  if (e.key === "ArrowUp") {
    e.preventDefault();
    if (mainChart) {
      const ts = mainChart.timeScale();
      const range = ts.getVisibleLogicalRange();
      if (range) {
        const barCount = range.to - range.from;
        const zoomFactor = 0.8; // 每次缩小20%可见范围
        const center = (range.from + range.to) / 2;
        const newHalf = (barCount * zoomFactor) / 2;
        ts.setVisibleLogicalRange({ from: center - newHalf, to: center + newHalf });
      }
    }
    return;
  }
  if (e.key === "ArrowDown") {
    e.preventDefault();
    if (mainChart) {
      const ts = mainChart.timeScale();
      const range = ts.getVisibleLogicalRange();
      if (range) {
        const barCount = range.to - range.from;
        const zoomFactor = 1.25; // 每次扩大25%可见范围
        const center = (range.from + range.to) / 2;
        const newHalf = (barCount * zoomFactor) / 2;
        ts.setVisibleLogicalRange({ from: center - newHalf, to: center + newHalf });
      }
    }
    return;
  }

  // ←→ 平移时间窗口（同花顺风格：每次移动可见范围的1/3）
  if (e.key === "ArrowLeft") {
    e.preventDefault();
    if (mainChart) {
      const ts = mainChart.timeScale();
      const range = ts.getVisibleLogicalRange();
      if (range) {
        const shift = (range.to - range.from) / 3;
        ts.setVisibleLogicalRange({ from: range.from - shift, to: range.to - shift });
      }
    }
    return;
  }
  if (e.key === "ArrowRight") {
    e.preventDefault();
    if (mainChart) {
      const ts = mainChart.timeScale();
      const range = ts.getVisibleLogicalRange();
      if (range) {
        const shift = (range.to - range.from) / 3;
        ts.setVisibleLogicalRange({ from: range.from + shift, to: range.to + shift });
      }
    }
    return;
  }

  // +/- 缩放（补充）
  if (e.key === "+" || e.key === "=") {
    mainChart?.timeScale().scrollToRealTime();
  }

  // Cmd+S 添加到自选股
  if ((e.metaKey || e.ctrlKey) && e.key === "s") {
    e.preventDefault();
    if (!isInWatchlist(symbol.value)) {
      addToWatchlist(symbol.value);
    }
  }
}

// ===== 生命周期 =====
onMounted(async () => {
  try {
    const codes = await getAllStockCodes();
    if (codes.length > 0) {
      symbol.value = codes[0];
    }
  } catch {
    // 获取失败则保持默认 "000001"
  }

  nextTick(() => loadData());

  // 启动后台自动增量同步（非阻塞，失败自动重试直到0失败）
  try {
    await autoSyncOnStartup(["m", "w", "d", "f60", "f30", "f15", "f5", "f1"]);
    // 开始轮询同步状态
    startBgSyncPolling();
  } catch {
    // 自动同步启动失败不影响主流程
  }

  // 响应窗口大小变化
  const resizeObserver = new ResizeObserver(() => {
    if (mainChart && chartContainer.value) {
      mainChart.applyOptions({
        width: chartContainer.value.clientWidth,
        height: chartContainer.value.clientHeight,
      });
    }
  });
  if (chartContainer.value) {
    resizeObserver.observe(chartContainer.value);
  }

  // 键盘快捷键
  document.addEventListener("keydown", handleKeydown);
});

// 监听 timeframe 变化
watch(timeframe, () => loadData());
</script>

<template>
  <div class="flex flex-col h-screen bg-[#1a1a2e] text-white">
    <!-- 顶部栏 -->
    <header class="flex items-center justify-between px-4 h-12 bg-[#16213e] border-b border-[#2a2a4a] shrink-0">
      <div class="flex items-center gap-4">
        <h1 class="text-lg font-bold text-[#e94560]">墨岩K线</h1>
        <!-- 视图切换 -->
        <div class="flex items-center gap-1">
          <button
            v-for="mode in ([
              { key: 'pure', label: '纯K线' },
              { key: 'czsc', label: '缠论' },
              { key: 'wyckoff', label: '威科夫' },
              { key: 'fusion', label: '融合' },
            ] as const)"
            :key="mode.key"
            @click="onViewModeChange(mode.key as any)"
            class="px-2.5 py-1 text-xs rounded transition-all duration-150"
            :class="viewMode === mode.key
              ? 'bg-[#e94560] text-white font-semibold'
              : 'text-[#9e9e9e] hover:bg-[#0f3460] hover:text-white'"
          >
            {{ mode.label }}
          </button>
        </div>

        <div class="w-px h-5 bg-[#2a2a4a]"></div>

        <!-- 数据同步入口 -->
        <button
          @click="currentView = currentView === 'sync' ? 'chart' : 'sync'"
          class="px-2.5 py-1 text-xs rounded transition-all flex items-center gap-1"
          :class="currentView === 'sync' ? 'bg-[#e94560] text-white' : 'text-[#9e9e9e] hover:bg-[#0f3460] hover:text-white'"
        >
          数据同步
          <span v-if="bgSyncProgress?.running" class="inline-block w-1.5 h-1.5 rounded-full bg-green-400 animate-pulse"></span>
        </button>

        <!-- 后台同步进度条 -->
        <div v-if="bgSyncProgress?.running" class="flex items-center gap-2 text-[10px] text-[#9e9e9e] min-w-[180px]">
          <div class="flex-1 h-1 bg-[#0f3460] rounded-full overflow-hidden">
            <div class="h-full bg-green-500 rounded-full transition-all duration-500"
              :style="{ width: `${bgSyncProgress.total > 0 ? Math.round(bgSyncProgress.completed / bgSyncProgress.total * 100) : 0}%` }"></div>
          </div>
          <span class="whitespace-nowrap">{{ bgSyncProgress.completed }}/{{ bgSyncProgress.total }}
            <span v-if="bgSyncProgress.retrying" class="text-[#ff9800]">重试#{{ bgSyncProgress.retry_round }}</span>
          </span>
        </div>

        <StockSearch
          v-if="currentView === 'chart'"
          v-model="searchKeyword"
          :results="searchResults"
          :show="showSearch"
          @search="onSearch"
          @select="selectStock"
          @close="showSearch = false"
        />
      </div>

      <div v-if="currentView === 'chart' && currentPrice" class="flex items-center gap-4 text-sm">
        <span class="font-mono text-lg" :class="currentPrice.close >= currentPrice.open ? 'text-[#ef5350]' : 'text-[#26a69a]'">
          {{ currentPrice.close.toFixed(2) }}
        </span>
        <span v-if="priceChange" :class="priceChange.change >= 0 ? 'text-[#ef5350]' : 'text-[#26a69a]'">
          {{ priceChange.change >= 0 ? '+' : '' }}{{ priceChange.change.toFixed(2) }}
          ({{ priceChange.change >= 0 ? '+' : '' }}{{ priceChange.changePct.toFixed(2) }}%)
        </span>
        <span class="text-gray-400 font-mono">{{ symbol }}</span>
        <span v-if="chartData?.name" class="text-gray-300">{{ chartData.name }}</span>

        <!-- 自选股按钮 -->
        <button
          @click="isInWatchlist(symbol) ? null : addToWatchlist(symbol)"
          class="text-sm transition-all"
          :class="isInWatchlist(symbol) ? 'text-[#ffd700]' : 'text-[#666] hover:text-[#ffd700]'"
          :title="isInWatchlist(symbol) ? '已在自选股' : '添加到自选股 (Cmd+S)'"
        >
          {{ isInWatchlist(symbol) ? "★" : "☆" }}
        </button>
      </div>
    </header>

    <!-- 数据同步视图 -->
    <template v-if="currentView === 'sync'">
      <main class="flex-1 flex overflow-hidden">
        <DataSyncPanel class="flex-1" />
      </main>
    </template>

    <!-- K 线图视图 -->
    <template v-else>
      <!-- 工具栏 -->
      <ChartToolbar
        :timeframe="timeframe"
        :settings="settings"
        :view-mode="viewMode"
        @timeframe-change="onTimeframeChange"
        @settings-change="onSettingsChange"
        @view-mode-change="onViewModeChange"
      />

      <!-- 主内容区 -->
      <main class="flex-1 flex overflow-hidden">
        <!-- 自选股侧栏 -->
        <div class="w-40 shrink-0 border-r border-[#2a2a4a]">
          <WatchlistPanel
            :current-symbol="symbol"
            @select="selectStock"
            @remove="() => {}"
          />
        </div>

        <!-- K 线图区域 -->
        <div class="flex-1 flex flex-col relative">
          <div ref="chartContainer" class="flex-1 min-h-0"></div>
          <!-- 加载/错误/空数据提示 -->
          <div v-if="loading" class="absolute inset-0 flex items-center justify-center bg-[#1a1a2e]/80 z-10">
            <div class="text-[#9e9e9e] animate-pulse">加载中...</div>
          </div>
          <div v-else-if="error" class="absolute inset-0 flex items-center justify-center bg-[#1a1a2e]/80 z-10">
            <div class="text-[#ff5722]">{{ error }}</div>
          </div>
          <div v-else-if="chartData && chartData.klines.length === 0" class="absolute inset-0 flex items-center justify-center bg-[#1a1a2e]/80 z-10">
            <div class="text-[#9e9e9e]">暂无数据 — 请先同步该股票的 K 线数据</div>
          </div>

          <!-- 悬停信息弹窗 -->
          <div
            v-if="tooltipInfo"
            class="absolute z-20 bg-[#16213e] border border-[#2a2a4a] rounded shadow-lg p-2 text-xs max-w-xs"
            :style="{ left: Math.min(tooltipInfo.x + 10, 400) + 'px', top: Math.min(tooltipInfo.y - 60, 50) + 'px' }"
          >
            <template v-if="tooltipInfo.type === 'bi'">
              <div class="text-[#4a90d9] font-bold">笔</div>
              <div>方向: {{ tooltipInfo.data.direction === 'up' ? '上升' : '下降' }}</div>
              <div>{{ tooltipInfo.data.start_price.toFixed(2) }} → {{ tooltipInfo.data.end_price.toFixed(2) }}</div>
              <div>幅度: {{ ((tooltipInfo.data.end_price - tooltipInfo.data.start_price) / tooltipInfo.data.start_price * 100).toFixed(2) }}%</div>
            </template>
            <template v-else-if="tooltipInfo.type === 'xd'">
              <div class="text-[#b388ff] font-bold">线段</div>
              <div>方向: {{ tooltipInfo.data.direction === 'up' ? '上升' : '下降' }}</div>
              <div>{{ tooltipInfo.data.start_price.toFixed(2) }} → {{ tooltipInfo.data.end_price.toFixed(2) }}</div>
              <button
                class="mt-1 text-[#00bcd4] hover:text-white underline"
                @click="loadSubLevel(tooltipInfo.data)"
              >
                查看次级别 →
              </button>
            </template>
            <template v-else-if="tooltipInfo.type === 'zs'">
              <div class="text-[#b388ff] font-bold">{{ tooltipInfo.data.zs_type === 'bi_zs' ? '笔中枢' : '段中枢' }}</div>
              <div>zg: {{ tooltipInfo.data.zg.toFixed(2) }} zd: {{ tooltipInfo.data.zd.toFixed(2) }}</div>
              <div>gg: {{ tooltipInfo.data.gg.toFixed(2) }} dd: {{ tooltipInfo.data.dd.toFixed(2) }}</div>
            </template>
            <template v-else-if="tooltipInfo.type === 'bs'">
              <div class="font-bold" :class="tooltipInfo.data.bs_type.includes('buy') ? 'text-[#00e676]' : 'text-[#ff1744]'">
                {{ tooltipInfo.data.bs_type }}
              </div>
              <div>价格: {{ tooltipInfo.data.price.toFixed(2) }}</div>
              <div>时间: {{ tooltipInfo.data.dt }}</div>
            </template>
            <template v-else-if="tooltipInfo.type === 'wyckoff'">
              <div class="font-bold" :style="{ color: WYCKOFF_EVENT_COLORS[tooltipInfo.data.event_type] || '#fff' }">
                {{ tooltipInfo.data.event_type }}
              </div>
              <div>{{ tooltipInfo.data.description }}</div>
            </template>
          </div>

          <!-- 次级别走势面板 -->
          <div
            v-if="subLevelPanel"
            class="absolute bottom-0 left-0 right-0 h-48 bg-[#16213e] border-t border-[#2a2a4a] z-20 flex flex-col"
          >
            <div class="flex items-center justify-between px-3 py-1 border-b border-[#2a2a4a]">
              <span class="text-xs text-[#b388ff]">
                次级别走势：{{ subLevelPanel.xd.start_dt?.slice(0,10) }} → {{ subLevelPanel.xd.end_dt?.slice(0,10) }}
              </span>
              <button @click="closeSubLevel" class="text-[#9e9e9e] hover:text-white text-xs">✕</button>
            </div>
            <div v-if="subLevelPanel.loading" class="flex-1 flex items-center justify-center text-[#9e9e9e] text-xs animate-pulse">
              加载中...
            </div>
            <div v-else-if="subLevelPanel.data" class="flex-1 text-[#9e9e9e] text-xs p-2 overflow-auto">
              <div>共 {{ subLevelPanel.data.klines.length }} 根K线</div>
              <div v-if="subLevelPanel.data.czsc && subLevelPanel.data.czsc.bi.length > 0" class="mt-1">
                笔: {{ subLevelPanel.data.czsc.bi.length }} | 中枢: {{ subLevelPanel.data.czsc.bi_zs.length }}
              </div>
            </div>
            <div v-else class="flex-1 flex items-center justify-center text-[#666] text-xs">
              暂无次级别数据
            </div>
          </div>
        </div>

        <!-- 右侧面板 -->
        <div class="flex flex-col w-72 shrink-0 border-l border-[#2a2a4a]">
          <!-- 信号摘要面板 -->
          <SignalPanel
            v-if="chartData"
            :chart-data="chartData"
            :settings="settings"
            class="flex-1 overflow-y-auto"
          />

          <!-- 勾选设置面板 -->
          <SettingsPanel
            :settings="settings"
            @change="onSettingsChange"
          />
        </div>
      </main>
    </template>
  </div>
</template>
