<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed, nextTick } from "vue";
import {
  createChart,
  ColorType,
  type IChartApi,
  type ISeriesApi,
  type CandlestickData,
  type HistogramData,
  type LineData,
  type Time,
  type ISeriesPrimitive,
  type ISeriesPrimitivePaneView,
  type ISeriesPrimitivePaneRenderer,
  type SeriesAttachedParameter,
} from "lightweight-charts";
import { getChartData, searchStocks, getAllStockCodes, getSubLevelData, cancelSync, triggerSingleSync, pollSingleSync, saveAnalysisReport } from "./composables/useApi";
import type { SyncProgress } from "./composables/useApi";
import {
  type ChartData,
  type AnalysisSettings,
  type KLine,
  type TimeFrame,
  type ViewMode,
  DEFAULT_SETTINGS,
  VIEW_MODE_SETTINGS,
  CZSC_BS_COLORS,
  WYCKOFF_EVENT_COLORS,
  WYCKOFF_BULLISH_EVENTS,
  WYCKOFF_PHASE_COLORS,
  WYCKOFF_EVENT_DESC,
} from "./types";
import ChartToolbar from "./components/ChartToolbar.vue";
import StockSearch from "./components/StockSearch.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import DataSyncPanel from "./components/DataSyncPanel.vue";
import SignalPanel from "./components/SignalPanel.vue";
import WatchlistPanel from "./components/WatchlistPanel.vue";
import { useWatchlist, usePersistedSettings } from "./composables/useStorage";

// ===== 矩形图元：用于绘制中枢矩形 =====
interface RectangleProps {
  startTime: Time;
  endTime: Time;
  topPrice: number;
  bottomPrice: number;
  borderColor: string;
  borderWidth: number;
  fillColor: string;
}

class RectanglePaneView implements ISeriesPrimitivePaneView {
  private _props: RectangleProps;
  private _series: ISeriesApi<any> | null = null;
  private _chart: IChartApi | null = null;
  constructor(props: RectangleProps) { this._props = props; }
  setContext(series: ISeriesApi<any>, chart: IChartApi) { this._series = series; this._chart = chart; }
  zOrder(): "bottom" | "top" | "normal" { return "bottom"; }
  renderer(): ISeriesPrimitivePaneRenderer | null {
    return {
      draw: (target: any) => {
        if (!this._series || !this._chart) return;
        const tScale = this._chart.timeScale();
        const x1 = tScale.timeToCoordinate(this._props.startTime);
        const x2 = tScale.timeToCoordinate(this._props.endTime);
        const y1 = this._series.priceToCoordinate(this._props.topPrice);
        const y2 = this._series.priceToCoordinate(this._props.bottomPrice);
        if (x1 === null || x2 === null || y1 === null || y2 === null) return;
        // lightweight-charts v4 使用 fancy-canvas 的 CanvasRenderingTarget2D
        target.useMediaCoordinateSpace((scope: { context: CanvasRenderingContext2D }) => {
          const ctx = scope.context;
          ctx.save();
          // 填充
          ctx.fillStyle = this._props.fillColor;
          ctx.fillRect(x1, y1, x2 - x1, y2 - y1);
          // 边框
          ctx.strokeStyle = this._props.borderColor;
          ctx.lineWidth = this._props.borderWidth;
          ctx.strokeRect(x1, y1, x2 - x1, y2 - y1);
          ctx.restore();
        });
      },
    } as ISeriesPrimitivePaneRenderer;
  }
}

class RectanglePrimitive implements ISeriesPrimitive<Time> {
  private _paneView: RectanglePaneView;
  private _props: RectangleProps;
  constructor(
    startTime: Time, endTime: Time,
    topPrice: number, bottomPrice: number,
    borderColor: string, borderWidth: number, fillColor: string,
  ) {
    this._props = { startTime, endTime, topPrice, bottomPrice, borderColor, borderWidth, fillColor };
    this._paneView = new RectanglePaneView(this._props);
  }
  attached(param: SeriesAttachedParameter<Time>): void {
    this._paneView.setContext(param.series as unknown as ISeriesApi<any>, param.chart as unknown as IChartApi);
  }
  detached(): void {}
  paneViews(): readonly ISeriesPrimitivePaneView[] { return [this._paneView]; }
}

// ===== 状态 =====
const symbol = ref("000001");
const timeframe = ref<TimeFrame>("d");
const chartData = ref<ChartData | null>(null);
const loading = ref(false);
const syncing = ref(false);  // 正在从网络同步数据（get_chart_data 内置）
const syncingHistory = ref(false); // 正在扩展历史数据（光标左移触发）
const syncingHistoryMsg = ref(""); // 扩展历史数据的反馈消息（"暂无更早数据" 等）
const historyReachedEnd = ref(false); // 当前股票/级别是否已到最早数据（阻止重复触发）
let unsubVisChange: ((range: any) => void) | null = null; // 取消 onVisibleRangeChange 订阅
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
interface TooltipItem {
  type: string;   // "bi" | "xd" | "zs" | "bs" | "beichi" | "wyckoff"
  data: any;
}
const tooltipInfo = ref<{
  items: TooltipItem[];
  x: number;
  y: number;
} | null>(null);

// ===== 光标位置 K 线信息 =====
const crosshairKline = ref<KLine | null>(null);

// ===== 次级别走势面板 =====
const subLevelPanel = ref<{
  xd: any;
  data: ChartData | null;
  loading: boolean;
} | null>(null);

// ===== 右栏窗格拖拽 =====
/** 两个窗格的像素高度（缠论信号 / 威科夫信号）；设置窗格自动填充剩余高度 */
const czscPaneH = ref(200);
const wyckoffPaneH = ref(200);

/** 正在拖拽的分隔线：'czsc-wy' 或 'wy-settings' */
const draggingDivider = ref<string | null>(null);
const dragStartY = ref(0);
const dragStartHeights = ref({ a: 0, b: 0 });

/** 最小窗格高度（px） */
const MIN_PANE_H = 40;

function startDividerDrag(e: MouseEvent, divider: string) {
  e.preventDefault();
  draggingDivider.value = divider;
  dragStartY.value = e.clientY;
  if (divider === "czsc-wy") {
    dragStartHeights.value = { a: czscPaneH.value, b: wyckoffPaneH.value };
  } else {
    // 仅记录 wyckoff 高度，设置窗格由 flex:1 自动填充
    dragStartHeights.value = { a: wyckoffPaneH.value, b: 0 };
  }
  const onMove = (ev: MouseEvent) => {
    if (!draggingDivider.value) return;
    const delta = ev.clientY - dragStartY.value;
    let newA = dragStartHeights.value.a + delta;
    newA = Math.max(MIN_PANE_H, newA);
    if (draggingDivider.value === "czsc-wy") {
      let newB = dragStartHeights.value.b - delta;
      newB = Math.max(MIN_PANE_H, newB);
      czscPaneH.value = newA;
      wyckoffPaneH.value = newB;
    } else {
      wyckoffPaneH.value = newA;
      // 设置窗格由 flex:1 自动填充剩余空间
    }
  };
  const onUp = () => {
    draggingDivider.value = null;
    document.removeEventListener("mousemove", onMove);
    document.removeEventListener("mouseup", onUp);
  };
  document.addEventListener("mousemove", onMove);
  document.addEventListener("mouseup", onUp);
}

// ===== 计算属性 =====
const currentPrice = computed(() => {
  if (!chartData.value || chartData.value.klines.length === 0) return null;
  // 光标悬停时显示光标所在K线，否则显示最后一根
  return crosshairKline.value || chartData.value.klines[chartData.value.klines.length - 1];
});

const priceChange = computed(() => {
  if (!chartData.value || chartData.value.klines.length === 0) return null;
  const klines = chartData.value.klines;
  const curr = currentPrice.value;
  if (!curr) return null;
  // 找到当前K线的index
  const idx = klines.findIndex((k) => k.dt === curr.dt);
  if (idx < 1) return null;
  const prev = klines[idx - 1];
  return {
    change: curr.close - prev.close,
    changePct: ((curr.close - prev.close) / prev.close) * 100,
    open: curr.open,
    high: curr.high,
    low: curr.low,
    close: curr.close,
    vol: curr.vol,
    dt: curr.dt,
  };
});

// ===== 数据加载 =====
async function loadData() {
  loading.value = true;
  syncing.value = false;
  error.value = "";

  // 1秒后如果仍在加载，显示"同步中"提示
  const syncTimer = setTimeout(() => {
    if (loading.value) {
      syncing.value = true;
    }
  }, 1000);

  try {
    const data = await getChartData(
      symbol.value,
      timeframe.value,
      true,   // 始终获取缠论数据，以便设置面板控制显示
      true    // 始终获取威科夫数据，以便设置面板控制显示
    );
    chartData.value = data;
    await nextTick();
    try {
      renderChart();
    } catch (renderErr) {
      console.error("renderChart error:", renderErr);
    }
  } catch (e: any) {
    error.value = e.toString();
  } finally {
    clearTimeout(syncTimer);
    loading.value = false;
    syncing.value = false;
  }
}

function hasAnyCzscEnabled(): boolean {
  const c = settings.value.czsc;
  return c.showFenxing || c.showBi || c.showXd || c.showBiZs || c.showXdZs
    || c.show1buy || c.show2buy || c.show3buy || c.show1sell || c.show2sell || c.show3sell
    || c.showBeichi;
}

// ===== 保存判定报告 =====
async function saveReport() {
  if (!chartData.value) return;
  try {
    const filepath = await saveAnalysisReport(
      symbol.value,
      chartData.value.name || symbol.value,
      timeframe.value,
      chartData.value,
    );
    alert(`报告已保存：${filepath}`);
  } catch (e: any) {
    alert(`保存失败：${e}`);
  }
}

// ===== 时间格式化 =====
function toTime(dt: string): Time {
  // lightweight-charts v4 的 Time 类型接受：
  //   1. UTCTimestamp (Unix 秒数，整数)
  //   2. BusinessDay 字符串 "YYYY-MM-DD"（仅日线及以上，timeVisible=true 时也可用）
  //   3. BusinessDay 对象 { year, month, day }
  //
  // 关键：Date.parse("2024-01-15") 按 UTC 解析，而 Date.parse("2024-01-15 09:30") 按本地时间解析，
  // 混用会导致时区偏移不一致，K线时间错乱无法渲染。
  // 因此：纯日期格式直接用字符串（BusinessDay），带时间的统一用本地时间转 UTCTimestamp。

  if (/^\d{4}-\d{2}-\d{2}$/.test(dt)) {
    // 纯日期 "YYYY-MM-DD" → 直接作为 BusinessDay 字符串
    return dt as Time;
  }
  // "YYYY-MM-DD HH:MM" 或 "YYYY-MM-DD HH:MM:SS" → 手动解析为本地时间 UTCTimestamp
  // 避免用 Date.parse()，因为它对 "YYYY-MM-DD" 和 "YYYY-MM-DD HH:MM" 的时区处理不一致
  const m = dt.match(/^(\d{4})-(\d{2})-(\d{2})\s+(\d{2}):(\d{2})/);
  if (m) {
    const [, y, mo, d, h, mi] = m;
    const sec = Math.floor(new Date(+y, +mo - 1, +d, +h, +mi).getTime() / 1000);
    return sec as Time;
  }
  // 兜底：尝试 Date.parse
  const ms = Date.parse(dt);
  if (!isNaN(ms)) {
    return Math.floor(ms / 1000) as Time;
  }
  // 最终兜底：截取日期部分
  return dt.slice(0, 10) as Time;
}

// ===== 图表渲染 =====
const MAX_VISIBLE_KLINES = 5000; // 性能阈值：超过此数量截断

function renderChart(historyAdded?: number) {
  if (!chartData.value || !chartContainer.value) {
    return;
  }
  const data = chartData.value;

  // autoSize: true 模式下，图表会自动追踪容器尺寸
  // 但首次渲染时容器可能还没完成布局，需要确保有尺寸
  const containerWidth = chartContainer.value.clientWidth;
  const containerHeight = chartContainer.value.clientHeight;
  if (containerWidth === 0 || containerHeight === 0) {
    setTimeout(() => renderChart(historyAdded), 100);
    return;
  }

  if (mainChart) {
    mainChart.remove();
    mainChart = null;
  }

  mainChart = createChart(chartContainer.value, {
    autoSize: true,
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
      scaleMargins: { top: 0.05, bottom: 0.35 },
    },
    timeScale: {
      borderColor: "#2a2a4a",
      timeVisible: true,
      secondsVisible: false,
    },
    handleScale: {
      mouseWheel: false,  // 禁用默认滚轮缩放，改用手动处理（以光标为锚点）
    },
  });

  // K 线数据（扩展历史数据时显示全量，否则截断到阈值）
 const fullDisplay = historyAdded != null && historyAdded > 0;
 const startIdx = fullDisplay
   ? 0  // 历史扩展模式：显示全部数据，让新添加的旧数据可见
   : (data.klines.length > MAX_VISIBLE_KLINES
     ? data.klines.length - MAX_VISIBLE_KLINES
     : 0);
  const visibleKlines = data.klines.slice(startIdx);


  const candleData: CandlestickData<Time>[] = visibleKlines.map((k) => ({
    time: toTime(k.dt),
    open: k.open,
    high: k.high,
    low: k.low,
    close: k.close,
  }));

  // 检查数据有效性
  for (let i = 1; i < candleData.length; i++) {
    if (candleData[i].time === candleData[i - 1].time) {
      console.warn("重复时间: i=" + i + " time=" + String(candleData[i].time));
    }
  }

  candleSeries = mainChart.addCandlestickSeries({
    upColor: "#ef5350",
    downColor: "#26a69a",
    borderUpColor: "#ef5350",
    borderDownColor: "#26a69a",
    wickUpColor: "#ef5350",
    wickDownColor: "#26a69a",
  });
  candleSeries.setData(candleData);

  // 成交量（与 K 线使用同一份截断数据，保证时间对齐）
  const volumeData: HistogramData<Time>[] = visibleKlines.map((k) => ({
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
    scaleMargins: { top: 0.7, bottom: 0.2 },
  });

  // MACD 副图（使用截断后的数据，保证与 K 线时间对齐）
  if (settings.value.chart?.showMacd && data.macd && data.macd.dif.length > 0) {
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
      if (i < startIdx) continue;  // 只渲染可见范围
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
      if (i < startIdx) continue;
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
      if (i < startIdx) continue;
      const v = macdData.macd_hist[i];
      macdHistData.push({
        time: toTime(data.klines[i].dt),
        value: v,
        color: v >= 0 ? "rgba(239,83,80,0.6)" : "rgba(38,166,154,0.6)",
      });
    }
    macdHistSeries.setData(macdHistData);

    // MACD 副图位置：底部 20% 区域
    mainChart.priceScale("macd").applyOptions({
      scaleMargins: { top: 0.8, bottom: 0 },
    });
  }

  // ===== 缠论覆盖层 =====
  // 分步执行，精准定位问题
  let _diagnosePhase = "";
  if (data.czsc) {
    const czsc = data.czsc;
    const allMarkers: any[] = [];

    // 1) 分型标记
    _diagnosePhase = "分型";
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

    // 2) 笔
    _diagnosePhase = "笔";
    if (settings.value.czsc.showBi && czsc.bi.length > 0) {
      try {
      const biStyle = settings.value.styles.bi;
      const biSeries = mainChart!.addLineSeries({
        color: biStyle.color,
        lineWidth: biStyle.lineWidth as 1 | 2 | 3 | 4,
        priceLineVisible: false,
        lastValueVisible: false,
        crosshairMarkerVisible: false,
      });

      const biData: LineData<Time>[] = [];
      for (const bi of czsc.bi) {
        const startK = data.klines[bi.start_index];
        const endK = data.klines[Math.min(bi.end_index, data.klines.length - 1)];
        if (startK && endK) {
          if (biData.length === 0 || biData.length > 0 && biData[biData.length - 1].time !== (toTime(startK.dt))) {
            biData.push({ time: toTime(startK.dt), value: bi.start_price });
          }
          biData.push({ time: toTime(endK.dt), value: bi.end_price });
        }
      }
      biSeries.setData(biData);
      } catch (e) { console.error("[笔渲染异常]", e); }
    }

    // 3) 线段 — 每条线段用独立的 LineSeries（因为线段时间可能重叠，不能合并）
    _diagnosePhase = "线段";
    if (settings.value.czsc.showXd && czsc.xd.length > 0) {
      try {
      const xdStyle = settings.value.styles.xd;
      for (const xd of czsc.xd) {
        const startK = data.klines[xd.start_index];
        const endK = data.klines[Math.min(xd.end_index, data.klines.length - 1)];
        if (startK && endK) {
          const xdSeries = mainChart!.addLineSeries({
            color: xdStyle.color,
            lineWidth: xdStyle.lineWidth as 1 | 2 | 3 | 4,
            lineStyle: 2,
            priceLineVisible: false,
            lastValueVisible: false,
            crosshairMarkerVisible: false,
          });
          xdSeries.setData([
            { time: toTime(startK.dt), value: xd.start_price },
            { time: toTime(endK.dt), value: xd.end_price },
          ]);
        }
      }
      } catch (e) { console.error("[线段渲染异常]", e); }
    }

    // 4) 买卖点（6类独立控制，不同形状标记）
    _diagnosePhase = "买卖点";
    if (czsc.buy_sell.length > 0) {
      const bsVisibility: Record<string, boolean> = {
        "1buy": settings.value.czsc.show1buy,
        "2buy": settings.value.czsc.show2buy,
        "2buy_break": settings.value.czsc.show2buy,
        "3buy": settings.value.czsc.show3buy,
        "2+3buy": settings.value.czsc.show2buy && settings.value.czsc.show3buy,
        "2+3buy_break": settings.value.czsc.show2buy && settings.value.czsc.show3buy,
        "1sell": settings.value.czsc.show1sell,
        "2sell": settings.value.czsc.show2sell,
        "2sell_break": settings.value.czsc.show2sell,
        "3sell": settings.value.czsc.show3sell,
        "2+3sell": settings.value.czsc.show2sell && settings.value.czsc.show3sell,
        "2+3sell_break": settings.value.czsc.show2sell && settings.value.czsc.show3sell,
      };
      for (const bs of czsc.buy_sell) {
        if (!bsVisibility[bs.bs_type]) continue;
        const k = data.klines[bs.index];
        if (!k) continue;
        const isBuy = bs.bs_type.includes("buy");
        const bsConf = CZSC_BS_COLORS[bs.bs_type] || {
          color: isBuy ? "#00e676" : "#ff1744",
          text: bs.bs_type,
          shape: "circle",
        };
        allMarkers.push({
          time: toTime(k.dt),
          position: isBuy ? ("belowBar" as const) : ("aboveBar" as const),
          color: bsConf.color,
          shape: bsConf.shape as "circle" | "square" | "arrowUp" | "arrowDown",
          size: 1,
          text: bsConf.text,
        });
      }
    }

    // 5) 笔中枢
    _diagnosePhase = "笔中枢";
    if (settings.value.czsc.showBiZs && czsc.bi_zs.length > 0) {
      const zsStyle = settings.value.styles.biZs;
      renderZhongShu(czsc.bi_zs, data, zsStyle);
    }

    // 6) 线段中枢
    _diagnosePhase = "线段中枢";
    if (settings.value.czsc.showXdZs && czsc.xd_zs.length > 0) {
      const zsStyle = settings.value.styles.xdZs;
      renderZhongShu(czsc.xd_zs, data, zsStyle);
    }

    // 7) 背驰
    _diagnosePhase = "背驰";
    if (settings.value.czsc.showBeichi && czsc.beichi.length > 0) {
      for (const bc of czsc.beichi) {
        const k = data.klines[bc.index];
        if (!k) continue;
        const isUp = bc.direction === "up";
        let text = "⚡";
        if (bc.bc_sub_type === "panzheng") text = "⚡盘整";
        else if (bc.bc_type === "xd_beichi") text = "⚡线段";
        else text = "⚡笔";
        const bcColor = isUp ? "#ff5252" : "#69f0ae";
        allMarkers.push({
          time: toTime(k.dt),
          position: isUp ? ("aboveBar" as const) : ("belowBar" as const),
          color: bcColor,
          shape: "circle" as const,
          size: 1,
          text,
        });
      }
    }

    // 设置所有 markers
    _diagnosePhase = "markers排序";
    if (allMarkers.length > 0) {
      try {
        candleSeries!.setMarkers(
          allMarkers.sort((a, b) => {
            const ta = typeof a.time === 'number' ? a.time : String(a.time);
            const tb = typeof b.time === 'number' ? b.time : String(b.time);
            return ta < tb ? -1 : ta > tb ? 1 : 0;
          })
        );
      } catch (e) {
        console.error("[setMarkers异常]", e, "markers_count=", allMarkers.length);
      }
    }
    _diagnosePhase = "完成";

    console.log("[DIAG] 缠论覆盖层渲染完成", {
      fenxing: czsc.fenxing.length,
      bi: czsc.bi.length,
      xd: czsc.xd.length,
      bi_zs: czsc.bi_zs.length,
      xd_zs: czsc.xd_zs.length,
      buy_sell: czsc.buy_sell.length,
      beichi: czsc.beichi.length,
      zoushi: czsc.zoushi.length,
      phase: _diagnosePhase,
      settings: {
        showFenxing: settings.value.czsc.showFenxing,
        showBi: settings.value.czsc.showBi,
        showXd: settings.value.czsc.showXd,
        showBiZs: settings.value.czsc.showBiZs,
        showXdZs: settings.value.czsc.showXdZs,
        show1buy: settings.value.czsc.show1buy,
        show2buy: settings.value.czsc.show2buy,
        show3buy: settings.value.czsc.show3buy,
        show1sell: settings.value.czsc.show1sell,
        show2sell: settings.value.czsc.show2sell,
        show3sell: settings.value.czsc.show3sell,
        showBeichi: settings.value.czsc.showBeichi,
      },
    });
  }

  // 威科夫覆盖层
  if (data.wyckoff) {
    try {
      renderWyckoffOverlays(data);
    } catch (e) {
      console.error("[威科夫覆盖层渲染异常]", e);
    }
  }

  // 融合标记
  if (data.fusion && settings.value.fusion.showFusion) {
    try {
      renderFusionOverlays(data);
    } catch (e) {
      console.error("[融合覆盖层渲染异常]", e);
    }
  }

  // 扩展历史数据后，定位到新增数据的起始处（让用户看到新旧交界）
  if (historyAdded != null && historyAdded > 0 && visibleKlines.length > historyAdded) {
    // 显示从新增数据末尾往前 200 根K线开始的范围（让用户看到新数据并可以继续往左翻）
    const ts = mainChart.timeScale();
    const targetFrom = Math.max(0, historyAdded - 200);
    const targetTo = Math.min(visibleKlines.length, historyAdded + 50);
    try {
      ts.setVisibleLogicalRange({ from: targetFrom, to: targetTo });
    } catch (e) {
      ts.fitContent();
    }
  } else {
    mainChart.timeScale().fitContent();
  }

  // ── 可见范围变化 → 检测是否需要扩展历史数据 ──
 const timeScale = mainChart.timeScale();
 const onVisibleRangeChange = () => {
   const visRange = timeScale.getVisibleLogicalRange();
   if (!visRange || !chartData.value || chartData.value.klines.length === 0) return;

   // dataStartIdx 需与上面 candleSeries 的 startIdx 一致
   const dataStartIdx = fullDisplay
     ? 0  // 历史扩展模式：candleSeries 包含全量数据
     : (chartData.value.klines.length > MAX_VISIBLE_KLINES
       ? chartData.value.klines.length - MAX_VISIBLE_KLINES
       : 0);

    // Phase 1: 可见数据左边还有已加载但未显示的数据 → 先展开所有已加载数据
    if (visRange.from < 50 && dataStartIdx > 0 && !fullDisplay && !syncingHistory.value && !historyReachedEnd.value) {
      console.log(`[数据展开] 展开已加载数据，dataStartIdx=${dataStartIdx}`);
      // Reveal all loaded data by re-rendering in fullDisplay mode
      if (unsubVisChange) { timeScale.unsubscribeVisibleLogicalRangeChange(unsubVisChange); unsubVisChange = null; }
      renderChart(1);
      return;
    }

    // Phase 2: 全文索引接近 0，触发后端扩展历史数据
    // 已到达最早数据时不再触发，避免无限循环
    const fullIndex = dataStartIdx + visRange.from;
    if (fullIndex < 50 && !syncingHistory.value && !historyReachedEnd.value) {
      const earliestK = chartData.value.klines[dataStartIdx];
      if (!earliestK) return;

      // 计算需要扩展到的 target_start_date（最早日期往前推）
      const earlyDt = new Date(earliestK.dt);
      let targetStart: string;
      if (timeframe.value === "d") {
        // 日线：往前推3年
        earlyDt.setFullYear(earlyDt.getFullYear() - 3);
        targetStart = earlyDt.toISOString().split("T")[0];
      } else if (timeframe.value === "w" || timeframe.value === "m") {
        // 周线/月线：往前推5年
        earlyDt.setFullYear(earlyDt.getFullYear() - 5);
        targetStart = earlyDt.toISOString().split("T")[0];
      } else {
        // 分钟级别：往前推1年
        earlyDt.setFullYear(earlyDt.getFullYear() - 1);
        targetStart = earlyDt.toISOString().split("T")[0];
      }

      syncingHistory.value = true;
      console.log(`[历史扩展] ${symbol.value} ${timeframe.value} 最早=${earliestK.dt} → 请求 ${targetStart}`);

      // 触发后台同步 + 轮询
      triggerSingleSync(symbol.value, timeframe.value, targetStart).then(() => {
        // 轮询直到完成
        const pollTimer = setInterval(async () => {
          try {
            const state = await pollSingleSync(symbol.value, timeframe.value);
            if (!state.running && state.done) {
              clearInterval(pollTimer);
              syncingHistory.value = false;
              if (state.status === "ok") {
                console.log(`[历史扩展] 完成，${state.count} 条数据`);
                // 重新加载数据（全部数据）
                const data = await getChartData(symbol.value, timeframe.value, true, true);
                const oldKlineCount = chartData.value?.klines.length || 0;
                const newKlineCount = data.klines.length;
                const addedCount = newKlineCount - oldKlineCount;
                chartData.value = data;
                await nextTick();
                // 无新增数据时提示用户，不重渲染图表
                if (addedCount <= 0) {
                  syncingHistoryMsg.value = "已是最早数据";
                  historyReachedEnd.value = true;  // 已到最早，阻止后续重复触发
                  setTimeout(() => { syncingHistoryMsg.value = ""; }, 2000);
                } else {
                  // 重新渲染，保留用户当前视角位置
                  // 传递新增的历史数据量，让 renderChart 从适当位置开始显示
                  renderChart(addedCount);
                }
              } else {
                console.log(`[历史扩展] 失败: ${state.msg}`);
                syncingHistoryMsg.value = "历史数据扩展失败";
                setTimeout(() => { syncingHistoryMsg.value = ""; }, 2000);
              }
            }
          } catch (e) {
            clearInterval(pollTimer);
            syncingHistory.value = false;
            console.error("[历史扩展] 轮询异常:", e);
          }
        }, 1000);
      }).catch((e) => {
        syncingHistory.value = false;
        console.error("[历史扩展] 触发同步异常:", e);
      });
    }
  };
  // 取消旧订阅（renderChart 可能被多次调用），再注册新的
  if (unsubVisChange) timeScale.unsubscribeVisibleLogicalRangeChange(unsubVisChange);
  unsubVisChange = onVisibleRangeChange;
  timeScale.subscribeVisibleLogicalRangeChange(onVisibleRangeChange);

  // ── 滚轮缩放：以光标位置为锚点（仅垂直滚轮，水平留给触摸板平移）──
  const onWheel = (e: WheelEvent) => {
    // 水平滚动占主导 = 触摸板左右滑动 = 不缩放，让图表自身处理平移
    if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) return;
    e.preventDefault();
    const ts = mainChart!.timeScale();
    const visRange = ts.getVisibleLogicalRange();
    if (!visRange || visRange.from === null || visRange.to === null) return;

    // 鼠标在 chart 容器中的 X 坐标
    const rect = chartContainer.value!.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;

    // 将坐标转换为逻辑索引（光标位置的 bar）
    const cursorLogical = ts.coordinateToLogical(mouseX);
    if (cursorLogical === null) return;

    // 缩放系数：向上滚放大（范围缩小），向下滚缩小（范围扩大）
    const zoomFactor = e.deltaY > 0 ? 1.15 : 0.87;
    const currentRange = visRange.to - visRange.from;
    let newRange = Math.max(5, Math.floor(currentRange * zoomFactor));
    // 防止缩得太小或太大
    if (newRange < 5) newRange = 5;
    if (newRange > visibleKlines.length) newRange = visibleKlines.length;

    // 保持光标位置的 bar 在视口中的比例不变
    const cursorRatio = (cursorLogical - visRange.from) / currentRange;
    let newFrom = cursorLogical - newRange * cursorRatio;
    let newTo = newFrom + newRange;

    // 边界限制
    if (newFrom < 0) { newFrom = 0; newTo = newRange; }
    if (newTo > visibleKlines.length) { newTo = visibleKlines.length; newFrom = newTo - newRange; }

    try {
      ts.setVisibleLogicalRange({ from: newFrom, to: newTo });
    } catch (e) { /* 忽略范围设置错误 */ }
  };
  chartContainer.value!.addEventListener("wheel", onWheel, { passive: false });

  // 悬停事件
  mainChart.subscribeCrosshairMove((param) => {
    if (!param.time || !param.point) {
      tooltipInfo.value = null;
      crosshairKline.value = null;
      return;
    }
    // 查找该位置的 K 线 & 缠论/威科夫数据
    const idx = data.klines.findIndex((k) => toTime(k.dt) === param.time);
    if (idx < 0) {
      tooltipInfo.value = null;
      crosshairKline.value = null;
      return;
    }

    // 更新光标位置 K 线信息（用于 header 显示涨跌幅）
    crosshairKline.value = data.klines[idx];

    // 聚合该K线位置的所有缠论+威科夫信息
    const items: TooltipItem[] = [];

    // 查找缠论信息
    if (data.czsc) {
      // 查找笔
      const bi = data.czsc.bi.find(
        (b) => idx >= b.start_index && idx <= b.end_index
      );
      if (bi) {
        items.push({ type: "bi", data: bi });
      }

      // 查找线段
      const xd = data.czsc.xd.find(
        (x) => idx >= x.start_index && idx <= x.end_index
      );
      if (xd) {
        items.push({ type: "xd", data: xd });
      }

      // 查找中枢
      const zs = [...data.czsc.bi_zs, ...data.czsc.xd_zs].find(
        (z) => idx >= z.start_index && idx <= z.end_index
      );
      if (zs) {
        items.push({ type: "zs", data: zs });
      }

      // 查找买卖点
      const bs = data.czsc.buy_sell.find((b) => b.index === idx);
      if (bs) {
        items.push({ type: "bs", data: bs });
      }

      // 查找背驰
      const bc = data.czsc.beichi.find((b) => b.index === idx);
      if (bc) {
        items.push({ type: "beichi", data: bc });
      }
    }

    // 查找威科夫事件（同一K线可能有多个事件）
    if (data.wyckoff) {
      const evts = data.wyckoff.events.filter((e) => e.index === idx);
      for (const evt of evts) {
        items.push({ type: "wyckoff", data: evt });
      }
      // 供需线：当光标所在K线在某条供需线范围内时显示
      const sdLines = data.wyckoff.supply_demand_lines?.filter(
        (l) => idx >= l.start_index && idx <= l.end_index
      );
      if (sdLines && sdLines.length > 0) {
        for (const l of sdLines) {
          items.push({ type: "supply_demand_line", data: l });
        }
      }
    }

    tooltipInfo.value = items.length > 0
      ? { items, x: param.point.x, y: param.point.y }
      : null;
  });
}

// 渲染中枢（矩形框+半透明填充）
function renderZhongShu(zsList: any[], data: ChartData, style: import("./types").ZhongShuStyle) {
  for (const zs of zsList) {
    const startK = data.klines[zs.start_index];
    const endK = data.klines[Math.min(zs.end_index, data.klines.length - 1)];
    if (!startK || !endK) continue;

    // 使用 candleSeries 的 primitive 来绘制矩形
    const primitive = new RectanglePrimitive(
      toTime(startK.dt),
      toTime(endK.dt),
      zs.zg,
      zs.zd,
      style.borderColor,
      style.borderWidth,
      style.fillColor,
    );
    candleSeries!.attachPrimitive(primitive);
  }
}

// ===== 信号跳转 =====
function navigateToSignal(dt: string, price?: number) {
  if (!mainChart || !candleSeries || !chartData.value) return;
  const time = toTime(dt) as Time;
  const ts = mainChart.timeScale();

  // 通过时间找到该 K 线的逻辑索引，然后滚动到该位置居中
  const coord = ts.timeToCoordinate(time);
  if (coord !== null) {
    // 获取可见逻辑范围
    const visRange = ts.getVisibleLogicalRange();
    if (visRange) {
      const barCount = visRange.to - visRange.from;
      // 通过 coordinate 找到逻辑索引
      // timeToCoordinate 返回像素坐标，我们需要 logical index
      // 使用 candleSeries 的 data 来找到 index
      const klines = chartData.value.klines;
      const ki = klines.findIndex((k) => toTime(k.dt) === time);
      if (ki >= 0) {
        const logicalIdx = ki;  // 在截断数据中的索引 = 逻辑索引
        ts.scrollToPosition(logicalIdx - barCount / 2, true);
      }
    }
  }

  // 设置十字光标到该位置
  if (price !== undefined && price > 0) {
    mainChart.setCrosshairPosition(price, time, candleSeries);
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

  // 趋势线 — 仅在供需线开关开启时显示
  if (settings.value.wyckoff.showSupplyDemand && wyckoff.trend_lines.length > 0) {
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
      [...existingMarkers, ...eventMarkers].sort((a, b) => {
        const ta = typeof a.time === 'number' ? a.time : String(a.time);
        const tb = typeof b.time === 'number' ? b.time : String(b.time);
        return ta < tb ? -1 : ta > tb ? 1 : 0;
      })
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
    [...existingMarkers, ...fusionMarkers].sort((a, b) => {
      const ta = typeof a.time === 'number' ? a.time : String(a.time);
      const tb = typeof b.time === 'number' ? b.time : String(b.time);
      return ta < tb ? -1 : ta > tb ? 1 : 0;
    })
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
  historyReachedEnd.value = false;  // 切换股票，重置最早数据标记
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
async function stopBgSyncFromChart() {
  try {
    await cancelSync();
  } catch { /* ignore */ }
  if (bgSyncTimer) {
    clearInterval(bgSyncTimer);
    bgSyncTimer = null;
  }
  bgSyncProgress.value = { running: false, board: "", levels: [], total: 0, completed: 0, success: 0, failures: [], retrying: false, retry_round: 0, cancelled: false, current_symbols: [], preparing: false, prepare_error: "", all_skipped: false, skipped_count: 0, latest_date: "" };
}

// ===== 事件处理 =====
function onTimeframeChange(tf: TimeFrame) {
  timeframe.value = tf;
  persistedTf.value = tf;
  historyReachedEnd.value = false;  // 切换级别，重置最早数据标记
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
  const modeSettings = JSON.parse(JSON.stringify(VIEW_MODE_SETTINGS[mode]));
  // 保留用户自定义的 styles 配置
  modeSettings.styles = JSON.parse(JSON.stringify(settings.value.styles));
  settings.value = modeSettings as AnalysisSettings;
  persistedSettings.value = JSON.parse(JSON.stringify(settings.value));
  loadData();
}

// ===== 一键开关全部威科夫事件 =====
const ALL_WYCKOFF_EVENT_KEYS: (keyof AnalysisSettings["wyckoff"])[] = [
  "showSC", "showAR", "showST", "showSpring", "showSOS", "showLPS", "showJOC",
  "showPSY", "showBC", "showUTAD", "showSOW", "showLPSY",
];

function toggleAllWyckoffEvents() {
  const wyckoff = settings.value.wyckoff;
  const allOn = ALL_WYCKOFF_EVENT_KEYS.every((k) => wyckoff[k]);
  const newVal = !allOn;
  const newSettings = JSON.parse(JSON.stringify(settings.value));
  for (const k of ALL_WYCKOFF_EVENT_KEYS) {
    newSettings.wyckoff[k] = newVal;
  }
  settings.value = newSettings;
  persistedSettings.value = JSON.parse(JSON.stringify(newSettings));
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

  // W 切换威科夫视图
  if (e.key === "w" || e.key === "W") {
    e.preventDefault();
    onViewModeChange("wyckoff");
    return;
  }

  // E 一键开关全部威科夫事件信号
  if (e.key === "e" || e.key === "E") {
    e.preventDefault();
    toggleAllWyckoffEvents();
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

  // 自动同步已禁用 —— 用户可在数据同步面板手动启动
  // try {
  //   await autoSyncOnStartup(["d"]);
  //   startBgSyncPolling();
  // } catch {
  //   // 自动同步启动失败不影响主流程
  // }

  // autoSize: true 会自动追踪容器大小，无需手动 ResizeObserver
  // 仅在 autoSize 不生效时作为兜底
  const onWindowResize = () => {
    if (mainChart && chartContainer.value) {
      // autoSize 模式下无需手动设置 size
    }
  };
  window.addEventListener("resize", onWindowResize);

  // 键盘快捷键
  document.addEventListener("keydown", handleKeydown);
});

onUnmounted(() => {
  // 清理图表资源
  if (mainChart) {
    mainChart.remove();
    mainChart = null;
  }
});

// 监听 timeframe 变化
watch(timeframe, () => loadData());

// 监听视图切换回图表时，重新渲染
watch(currentView, (val) => {
  if (val === "chart" && chartData.value) {
    // 视图切换回来时，chartContainer 刚恢复显示，autoSize 会自动处理尺寸
    // 但需要延迟让 DOM 完成 reflow
    nextTick(() => {
      requestAnimationFrame(() => {
        if (mainChart && chartContainer.value) {
          // autoSize 模式下，通过 resize 触发重新计算
          // 直接调用内部 resize 逻辑
          try {
            (mainChart as any).resize?.();
          } catch {}
          // 如果 resize 不可用，重新创建图表
          renderChart();
        } else {
          // 图表不存在，重新创建
          renderChart();
        }
      });
    });
  }
});
</script>

<template>
  <div class="flex flex-col h-full bg-[#1a1a2e] text-white">
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
          <button @click="stopBgSyncFromChart" class="px-1.5 py-0.5 rounded border border-[#e94560]/40 text-[#e94560] hover:bg-[#e94560]/20 transition text-[10px]"
            title="停止后台同步">
            停止
          </button>
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

        <!-- 保存判定报告按钮 -->
        <button
          v-if="currentView === 'chart' && chartData?.czsc"
          @click="saveReport"
          class="px-2.5 py-1 text-xs rounded transition-all bg-[#0f3460] text-[#9e9e9e] hover:bg-[#e94560] hover:text-white"
          title="保存当前股票当前级别的缠论买卖点判定报告"
        >
          📄 保存报告
        </button>
      </div>

      <div v-if="currentView === 'chart' && currentPrice" class="flex items-center gap-3 text-sm font-mono">
        <!-- 日期 -->
        <span class="text-gray-400 text-xs">{{ priceChange?.dt?.slice(0, 10) }}</span>
        <!-- 收盘价 -->
        <span class="text-lg" :class="currentPrice.close >= currentPrice.open ? 'text-[#ef5350]' : 'text-[#26a69a]'">
          {{ currentPrice.close.toFixed(2) }}
        </span>
        <!-- 涨跌幅 -->
        <span v-if="priceChange" :class="priceChange.change >= 0 ? 'text-[#ef5350]' : 'text-[#26a69a]'">
          {{ priceChange.change >= 0 ? '+' : '' }}{{ priceChange.change.toFixed(2) }}
          ({{ priceChange.change >= 0 ? '+' : '' }}{{ priceChange.changePct.toFixed(2) }}%)
        </span>
        <!-- OHLC -->
        <span class="text-gray-400 text-xs">O <span :class="priceChange && priceChange.open >= priceChange.close ? 'text-[#26a69a]' : 'text-[#ef5350]'">{{ priceChange?.open?.toFixed(2) ?? currentPrice.open.toFixed(2) }}</span></span>
        <span class="text-gray-400 text-xs">H <span class="text-[#ef5350]">{{ priceChange?.high?.toFixed(2) ?? currentPrice.high.toFixed(2) }}</span></span>
        <span class="text-gray-400 text-xs">L <span class="text-[#26a69a]">{{ priceChange?.low?.toFixed(2) ?? currentPrice.low.toFixed(2) }}</span></span>
        <span class="text-gray-400 text-xs">C <span :class="priceChange && priceChange.close >= priceChange.open ? 'text-[#ef5350]' : 'text-[#26a69a]'">{{ priceChange?.close?.toFixed(2) ?? currentPrice.close.toFixed(2) }}</span></span>
        <!-- 成交量 -->
        <span class="text-gray-400 text-xs">V {{ (priceChange?.vol ?? currentPrice.vol) >= 10000 ? ((priceChange?.vol ?? currentPrice.vol) / 10000).toFixed(0) + '万' : (priceChange?.vol ?? currentPrice.vol).toFixed(0) }}</span>
        <!-- 代码 & 名称 -->
        <span class="text-gray-400">{{ symbol }}</span>
        <span v-if="chartData?.name" class="text-gray-300">{{ chartData.name }}</span>

        <!-- 光标位置背驰信息 -->
        <template v-if="crosshairKline && chartData?.czsc?.beichi">
          <template v-for="bc in chartData.czsc.beichi.filter(b => b.index === crosshairKline!.id)" :key="bc.index + bc.direction">
            <span class="text-xs font-semibold" :class="bc.direction === 'up' ? 'text-[#ff5252]' : 'text-[#69f0ae]'">
              {{ bc.direction === 'up' ? '⚡顶背驰' : '⚡底背驰' }}
              <span class="font-normal text-[#9e9e9e]">({{ bc.bc_type === 'xd_beichi' ? '线段' : '笔' }}{{ bc.bc_sub_type === 'panzheng' ? '·盘整' : bc.bc_sub_type === 'trend' ? '·趋势' : '' }})</span>
            </span>
            <span v-if="bc.reason" class="text-[10px] text-[#9e9e9e] max-w-[200px] truncate" :title="bc.reason">{{ bc.reason }}</span>
          </template>
        </template>

        <!-- 光标位置买卖点信息 -->
        <template v-if="crosshairKline && chartData?.czsc?.buy_sell">
          <template v-for="bs in chartData.czsc.buy_sell.filter(b => b.index === crosshairKline!.id)" :key="bs.index + bs.bs_type">
            <span class="text-xs font-semibold" :class="bs.bs_type.includes('buy') ? 'text-[#00e676]' : 'text-[#ff1744]'">
              {{ CZSC_BS_COLORS[bs.bs_type]?.label || bs.bs_type }}
            </span>
            <span class="text-[10px] text-[#9e9e9e]">{{ bs.price.toFixed(2) }}</span>
            <span v-if="bs.reason" class="text-[10px] text-[#9e9e9e] max-w-[200px] truncate" :title="bs.reason">{{ bs.reason }}</span>
          </template>
        </template>

        <!-- 光标位置威科夫事件 -->
        <template v-if="crosshairKline">
          <!-- 威科夫事件 -->
          <template v-if="chartData?.wyckoff?.events">
            <template v-for="(evt, ei) in chartData.wyckoff.events.filter(e => e.index === crosshairKline!.id)" :key="evt.index + '-' + evt.event_type + '-' + ei">
              <span class="text-xs font-semibold" :style="{ color: WYCKOFF_EVENT_COLORS[evt.event_type] || '#fff' }">
                {{ evt.event_type }}
              </span>
              <span v-if="evt.reason" class="text-[10px] text-[#9e9e9e] max-w-[200px] truncate" :title="evt.reason">{{ evt.reason }}</span>
            </template>
          </template>
          <!-- 供需线 -->
          <template v-if="chartData?.wyckoff?.supply_demand_lines">
            <template v-for="sdl in chartData.wyckoff.supply_demand_lines.filter(l => crosshairKline!.id >= l.start_index && crosshairKline!.id <= l.end_index)" :key="sdl.start_index + sdl.line_type">
              <span class="text-[10px]" :class="sdl.line_type === 'supply' ? 'text-[#ff5722]' : sdl.line_type === 'ice_line' ? 'text-[#80deea]' : 'text-[#66bb6a]'">
                {{ sdl.line_type === 'supply' ? '供' : sdl.line_type === 'ice_line' ? '冰' : '需' }}={{ sdl.start_price.toFixed(2) }}
              </span>
              <span v-if="sdl.reason" class="text-[10px] text-[#9e9e9e] max-w-[180px] truncate" :title="sdl.reason">{{ sdl.reason }}</span>
            </template>
          </template>
          <!-- 交易区间 -->
          <template v-if="chartData?.wyckoff?.trading_ranges">
            <template v-for="tr in chartData.wyckoff.trading_ranges.filter(r => crosshairKline!.id >= r.start_index && crosshairKline!.id <= r.end_index)" :key="tr.start_index">
              <span class="text-[10px] text-[#9e9e9e]">TR=[{{ tr.lower.toFixed(2) }}~{{ tr.upper.toFixed(2) }}] 冰={{ tr.ice_line.toFixed(2) }}</span>
            </template>
          </template>
          <!-- 阶段 -->
          <template v-if="chartData?.wyckoff?.phase_labels">
            <template v-for="pl in chartData.wyckoff.phase_labels.filter(p => p.index === crosshairKline!.id)" :key="pl.index">
              <span v-if="pl.phase !== 'Unknown'" class="text-[10px] font-semibold" :style="{ color: WYCKOFF_PHASE_COLORS[pl.phase] || '#9e9e9e' }">
                {{ pl.phase }}{{ pl.sub_phase ? '-' + pl.sub_phase : '' }}
              </span>
            </template>
          </template>
          <!-- 努力与结果 -->
          <template v-if="chartData?.wyckoff?.effort_results">
            <template v-for="er in chartData.wyckoff.effort_results.filter(e => e.index === crosshairKline!.id)" :key="er.index">
              <span class="text-[10px]" :class="er.interpretation === 'demand_dominant' ? 'text-[#66bb6a]' : er.interpretation === 'supply_dominant' ? 'text-[#ff5722]' : 'text-[#9e9e9e]'">
                E-R: {{ er.harmony === 'harmonious' ? '协调' : '背离' }}
              </span>
            </template>
          </template>
        </template>

        <!-- 自选股按钮 -->
        <button
          @click="isInWatchlist(symbol) ? null : addToWatchlist(symbol)"
          class="text-xl transition-all mr-3"
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
    <div v-show="currentView === 'chart'" class="flex-1 flex flex-col min-h-0">
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
          <div ref="chartContainer" class="flex-1 min-h-0 relative"></div>
          <!-- 加载/错误/空数据提示 -->
          <div v-if="loading" class="absolute inset-0 flex items-center justify-center bg-[#1a1a2e]/80 z-10">
            <div class="text-[#9e9e9e] animate-pulse">{{ syncing ? '同步中...' : '加载中...' }}</div>
          </div>
          <div v-else-if="error" class="absolute inset-0 flex items-center justify-center bg-[#1a1a2e]/80 z-10">
            <div class="text-[#ff5722]">{{ error }}</div>
          </div>
          <div v-else-if="chartData && chartData.klines.length === 0" class="absolute inset-0 flex items-center justify-center bg-[#1a1a2e]/80 z-10">
            <div class="text-[#9e9e9e]">暂无数据 — 正在自动同步，请稍候</div>
          </div>
          <!-- 历史数据扩展提示（光标左移触发） -->
          <div v-if="syncingHistory" class="absolute top-2 left-1/2 -translate-x-1/2 bg-[#2196f3]/90 text-white text-xs px-3 py-1 rounded-full z-10 animate-pulse">
            正在扩展历史数据...
          </div>
          <!-- 历史数据扩展结果提示 -->
          <div v-if="syncingHistoryMsg" class="absolute top-8 left-1/2 -translate-x-1/2 bg-[#ff9800]/90 text-white text-xs px-3 py-1 rounded-full z-10">
            {{ syncingHistoryMsg }}
          </div>

          <!-- 悬停信息弹窗 -->
          <div
            v-if="tooltipInfo"
            class="absolute z-20 bg-[#16213e] border border-[#2a2a4a] rounded shadow-lg p-2 text-xs max-w-xs"
            :style="{ left: Math.min(tooltipInfo.x + 10, 400) + 'px', top: Math.min(tooltipInfo.y - 60, 50) + 'px' }"
          >
            <template v-for="(item, i) in tooltipInfo.items" :key="i">
              <!-- 分隔线 -->
              <div v-if="i > 0" class="border-t border-[#2a2a4a] my-1"></div>
              <!-- 笔 -->
              <template v-if="item.type === 'bi'">
                <div class="text-[#4a90d9] font-bold">笔</div>
                <div>方向: {{ item.data.direction === 'up' ? '上升' : '下降' }}</div>
                <div>{{ item.data.start_price.toFixed(2) }} → {{ item.data.end_price.toFixed(2) }}</div>
                <div>幅度: {{ ((item.data.end_price - item.data.start_price) / item.data.start_price * 100).toFixed(2) }}%</div>
              </template>
              <!-- 线段 -->
              <template v-else-if="item.type === 'xd'">
                <div class="text-[#b388ff] font-bold">线段</div>
                <div>方向: {{ item.data.direction === 'up' ? '上升' : '下降' }}</div>
                <div>{{ item.data.start_price.toFixed(2) }} → {{ item.data.end_price.toFixed(2) }}</div>
                <button
                  class="mt-1 text-[#00bcd4] hover:text-white underline"
                  @click="loadSubLevel(item.data)"
                >
                  查看次级别 →
                </button>
              </template>
              <!-- 中枢 -->
              <template v-else-if="item.type === 'zs'">
                <div class="text-[#b388ff] font-bold">{{ item.data.zs_type === 'bi_zs' ? '笔中枢' : '段中枢' }}</div>
                <div>zg: {{ item.data.zg.toFixed(2) }} zd: {{ item.data.zd.toFixed(2) }}</div>
                <div>gg: {{ item.data.gg.toFixed(2) }} dd: {{ item.data.dd.toFixed(2) }}</div>
              </template>
              <!-- 买卖点 -->
              <template v-else-if="item.type === 'bs'">
                <div class="font-bold" :class="item.data.bs_type.includes('buy') ? 'text-[#00e676]' : 'text-[#ff1744]'">
                  {{ CZSC_BS_COLORS[item.data.bs_type]?.label || item.data.bs_type }}
                </div>
                <div>价格: {{ item.data.price.toFixed(2) }}</div>
                <div v-if="item.data.dt">时间: {{ item.data.dt }}</div>
                <div v-if="item.data.reason" class="text-[#ccc] mt-0.5 leading-snug">{{ item.data.reason }}</div>
              </template>
              <!-- 背驰 -->
              <template v-else-if="item.type === 'beichi'">
                <div class="font-bold" :class="item.data.direction === 'up' ? 'text-[#ff5252]' : 'text-[#69f0ae]'">
                  {{ item.data.direction === 'up' ? '顶背驰' : '底背驰' }}
                  <span class="font-normal text-[#9e9e9e]">
                    ({{ item.data.bc_type === 'xd_beichi' ? '线段' : '笔' }}{{ item.data.bc_sub_type === 'panzheng' ? '·盘整' : item.data.bc_sub_type === 'trend' ? '·趋势' : '' }})
                  </span>
                </div>
                <div v-if="item.data.reason" class="text-[#ccc] mt-0.5 leading-snug">{{ item.data.reason }}</div>
              </template>
              <!-- 威科夫 -->
              <template v-else-if="item.type === 'wyckoff'">
                <div class="font-bold" :style="{ color: WYCKOFF_EVENT_COLORS[item.data.event_type] || '#fff' }">
                  {{ item.data.event_type }}
                </div>
                <div>{{ WYCKOFF_EVENT_DESC[item.data.event_type] || item.data.description || '' }}</div>
                <div v-if="item.data.reason" class="text-[#ccc] mt-0.5 leading-snug">{{ item.data.reason }}</div>
              </template>
              <!-- 供需线 -->
              <template v-else-if="item.type === 'supply_demand_line'">
                <div class="font-bold" :class="item.data.line_type === 'supply' ? 'text-[#ff5722]' : item.data.line_type === 'ice_line' ? 'text-[#80deea]' : 'text-[#66bb6a]'">
                  {{ item.data.line_type === 'supply' ? '供给线' : item.data.line_type === 'ice_line' ? '冰线' : '需求线' }}
                </div>
                <div>{{ item.data.start_price.toFixed(2) }} → {{ item.data.end_price.toFixed(2) }} (斜率: {{ item.data.slope.toFixed(4) }})</div>
                <div v-if="item.data.reason" class="text-[#ccc] mt-0.5 leading-snug">{{ item.data.reason }}</div>
              </template>
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

        <!-- 右侧面板：三个可拖拽分隔的窗格 -->
        <div id="right-panel" class="flex flex-col w-64 shrink-0 border-l border-[#2a2a4a] select-none">
          <!-- 缠论信号窗格 -->
          <div class="flex flex-col min-h-0 border-b border-[#2a2a4a] overflow-hidden" :style="{ height: czscPaneH + 'px' }">
            <div class="shrink-0 px-3 py-1.5 border-b border-[#2a2a4a]/50 flex items-center gap-1.5">
              <span class="w-2 h-2 rounded-full bg-[#e94560]"></span>
              <span class="text-[10px] font-bold text-[#e94560] uppercase tracking-wider">缠论信号</span>
            </div>
            <div class="flex-1 overflow-y-auto min-h-0" v-if="chartData">
              <SignalPanel
                :chart-data="chartData"
                :settings="settings"
                mode="czsc"
                @navigate="navigateToSignal"
              />
            </div>
            <div v-else class="flex-1 flex items-center justify-center text-[#666] text-xs">暂无数据</div>
          </div>

          <!-- 拖拽分隔条 1 -->
          <div
            class="shrink-0 h-1.5 cursor-row-resize bg-[#2a2a4a]/60 hover:bg-[#e94560]/40 transition-colors flex items-center justify-center"
            @mousedown="startDividerDrag($event, 'czsc-wy')"
          >
            <div class="w-6 h-0.5 rounded-full bg-[#9e9e9e]/30"></div>
          </div>

          <!-- 威科夫信号窗格 -->
          <div class="flex flex-col min-h-0 border-b border-[#2a2a4a] overflow-hidden" :style="{ height: wyckoffPaneH + 'px' }">
            <div class="shrink-0 px-3 py-1.5 border-b border-[#2a2a4a]/50 flex items-center gap-1.5">
              <span class="w-2 h-2 rounded-full bg-[#26a69a]"></span>
              <span class="text-[10px] font-bold text-[#26a69a] uppercase tracking-wider">威科夫信号</span>
            </div>
            <div class="flex-1 overflow-y-auto min-h-0" v-if="chartData">
              <SignalPanel
                :chart-data="chartData"
                :settings="settings"
                mode="wyckoff"
                @navigate="navigateToSignal"
              />
            </div>
            <div v-else class="flex-1 flex items-center justify-center text-[#666] text-xs">暂无数据</div>
          </div>

          <!-- 拖拽分隔条 2 -->
          <div
            class="shrink-0 h-1.5 cursor-row-resize bg-[#2a2a4a]/60 hover:bg-[#26a69a]/40 transition-colors flex items-center justify-center"
            @mousedown="startDividerDrag($event, 'wy-settings')"
          >
            <div class="w-6 h-0.5 rounded-full bg-[#9e9e9e]/30"></div>
          </div>

          <!-- 设置窗格（自动填充剩余高度） -->
          <div class="flex flex-col min-h-0 flex-1 overflow-hidden">
            <div class="shrink-0 px-3 py-1.5 border-b border-[#2a2a4a]/50 flex items-center gap-1.5">
              <span class="w-2 h-2 rounded-full bg-[#b388ff]"></span>
              <span class="text-[10px] font-bold text-[#b388ff] uppercase tracking-wider">设置</span>
            </div>
            <div class="flex-1 overflow-y-auto min-h-0">
              <SettingsPanel
                :settings="settings"
                @change="onSettingsChange"
              />
            </div>
          </div>
        </div>
      </main>
    </div>
  </div>
</template>
