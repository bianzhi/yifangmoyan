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
import { getChartData, searchStocks } from "./composables/useApi";
import {
  type ChartData,
  type AnalysisSettings,
  type TimeFrame,
  DEFAULT_SETTINGS,
} from "./types";
import ChartToolbar from "./components/ChartToolbar.vue";
import StockSearch from "./components/StockSearch.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import DataSyncPanel from "./components/DataSyncPanel.vue";

// ===== 状态 =====
const symbol = ref("000001");
const timeframe = ref<TimeFrame>("d");
const chartData = ref<ChartData | null>(null);
const loading = ref(false);
const error = ref("");
const settings = ref<AnalysisSettings>({ ...DEFAULT_SETTINGS });
const currentView = ref<"chart" | "sync">("chart");
const searchKeyword = ref("");
const searchResults = ref<any[]>([]);
const showSearch = ref(false);

// ===== 图表引用 =====
const chartContainer = ref<HTMLDivElement>();
let mainChart: IChartApi | null = null;
let candleSeries: ISeriesApi<"Candlestick"> | null = null;
let volumeSeries: ISeriesApi<"Histogram"> | null = null;

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
      hasAnyCzscEnabled(),
      hasAnyWyckoffEnabled()
    );
    chartData.value = data;
    // 确保在 DOM 更新后再渲染，避免容器尺寸为 0
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

function hasAnyWyckoffEnabled(): boolean {
  return Object.values(settings.value.wyckoff).some(Boolean);
}

// ===== 时间格式化 =====
// lightweight-charts 要求 "YYYY-MM-DD" 或 UTCTimestamp
// 日线/周线/月线的 dt 可能是 "2023-01-03 00:00:00"，需截取为 "2023-01-03"
function toTime(dt: string): Time {
  // 分钟级别带 "HH:MM"，需要 "YYYY-MM-DD HH:MM" 格式
  // 日线及以上只取日期部分
  const match = dt.match(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2})/);
  if (match && match[2] !== "00:00") {
    return `${match[1]} ${match[2]}` as Time;
  }
  // 只保留日期部分
  return dt.slice(0, 10) as Time;
}

// ===== 图表渲染 =====
function renderChart() {
  if (!chartData.value || !chartContainer.value) return;
  const data = chartData.value;
  
  // 确保容器有有效尺寸
  const containerWidth = chartContainer.value.clientWidth;
  const containerHeight = chartContainer.value.clientHeight;
  if (containerWidth === 0 || containerHeight === 0) {
    // DOM 还没渲染完成，延迟重试
    requestAnimationFrame(() => renderChart());
    return;
  }

  // 清除旧图表
  if (mainChart) {
    mainChart.remove();
    mainChart = null;
  }

  // 创建主图
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
      scaleMargins: { top: 0.1, bottom: 0.25 },
    },
    timeScale: {
      borderColor: "#2a2a4a",
      timeVisible: true,
      secondsVisible: false,
    },
    width: chartContainer.value.clientWidth,
    height: chartContainer.value.clientHeight,
  });

  // K 线数据
  const candleData: CandlestickData<Time>[] = data.klines.map((k) => ({
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
    scaleMargins: { top: 0.8, bottom: 0 },
  });

  // 渲染缠论覆盖层
  if (data.czsc) {
    renderCzscOverlays(data);
  }

  // 渲染威科夫覆盖层
  if (data.wyckoff) {
    renderWyckoffOverlays(data);
  }

  mainChart.timeScale().fitContent();
}

function renderCzscOverlays(data: ChartData) {
  const czsc = data.czsc!;

  // 笔 — 蓝色(上) / 橙色(下) 折线
  if (settings.value.czsc.showBi && czsc.bi.length > 0) {
    // 每笔作为一条线段 (line series)
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
        if (biData.length === 0 || biData[biData.length - 1].time !== (toTime(startK.dt))) {
          biData.push({ time: toTime(startK.dt), value: bi.start_price });
        }
        biData.push({ time: toTime(endK.dt), value: bi.end_price });
      }
    }
    biSeries.setData(biData);
  }

  // 线段 — 紫色粗线
  if (settings.value.czsc.showXd && czsc.xd.length > 0) {
    const xdSeries = mainChart!.addLineSeries({
      color: "#b388ff",
      lineWidth: 3,
      lineStyle: 2, // 虚线
      priceLineVisible: false,
      lastValueVisible: false,
      crosshairMarkerVisible: false,
    });

    const xdData: LineData<Time>[] = [];
    for (const xd of czsc.xd) {
      const startK = data.klines[xd.start_index];
      const endK = data.klines[Math.min(xd.end_index, data.klines.length - 1)];
      if (startK && endK) {
        if (xdData.length === 0 || xdData[xdData.length - 1].time !== (toTime(startK.dt))) {
          xdData.push({ time: toTime(startK.dt), value: xd.start_price });
        }
        xdData.push({ time: toTime(endK.dt), value: xd.end_price });
      }
    }
    xdSeries.setData(xdData);
  }

  // 买卖点 — markers
  if (settings.value.czsc.showBuySell && czsc.buy_sell.length > 0) {
    const markers = czsc.buy_sell.map((bs) => {
      const k = data.klines[bs.index];
      const isBuy = bs.bs_type.includes("buy");
      return {
        time: toTime(k?.dt || bs.dt),
        position: isBuy ? "belowBar" as const : "aboveBar" as const,
        color: isBuy ? "#00e676" : "#ff1744",
        shape: isBuy ? ("arrowUp" as const) : ("arrowDown" as const),
        text: bs.bs_type,
      };
    });
    candleSeries!.setMarkers(markers.sort((a, b) => (a.time as string).localeCompare(b.time as string)));
  }

  // 中枢 — 用面积图模拟 (简化版用矩形标识)
  // 笔中枢
  if (settings.value.czsc.showBiZs && czsc.bi_zs.length > 0) {
    for (const zs of czsc.bi_zs) {
      const startK = data.klines[zs.start_index];
      const endK = data.klines[Math.min(zs.end_index, data.klines.length - 1)];
      if (startK && endK) {
        // 用上下沿线表示
        const upperLine = mainChart!.addLineSeries({
          color: "rgba(179,136,255,0.5)",
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

        const lowerLine = mainChart!.addLineSeries({
          color: "rgba(179,136,255,0.5)",
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
  }

  // 背驰标记
  if (settings.value.czsc.showBeichi && czsc.beichi.length > 0) {
    const beichiMarkers = czsc.beichi.map((bc) => {
      const k = data.klines[bc.index];
      return {
        time: toTime(k?.dt || bc.dt),
        position: bc.direction === "up" ? ("aboveBar" as const) : ("belowBar" as const),
        color: "#ff9800",
        shape: "circle" as const,
        text: "⚠背驰",
      };
    });
    // 合并到现有 markers
    const existingMarkers = candleSeries!.markers() || [];
    candleSeries!.setMarkers(
      [...existingMarkers, ...beichiMarkers].sort((a, b) =>
        (a.time as string).localeCompare(b.time as string)
      )
    );
  }
}

function renderWyckoffOverlays(data: ChartData) {
  const wyckoff = data.wyckoff!;

  // 趋势线
  if (settings.value.wyckoff.showTrendLines && wyckoff.trend_lines.length > 0) {
    for (const tl of wyckoff.trend_lines) {
      const startK = data.klines[tl.start_index];
      const endK = data.klines[Math.min(tl.end_index, data.klines.length - 1)];
      if (startK && endK) {
        const series = mainChart!.addLineSeries({
          color: tl.line_type === "support" ? "rgba(0,230,118,0.6)" : "rgba(255,23,68,0.6)",
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
  }

  // 交易区间 (TR) + 冰线
  if (settings.value.wyckoff.showTR && wyckoff.trading_ranges.length > 0) {
    for (const tr of wyckoff.trading_ranges) {
      const startK = data.klines[tr.start_index];
      const endK = data.klines[Math.min(tr.end_index, data.klines.length - 1)];
      if (startK && endK) {
        const upperLine = mainChart!.addLineSeries({
          color: "rgba(255,152,0,0.4)",
          lineWidth: 1,
          lineStyle: 2,
          priceLineVisible: false,
          lastValueVisible: false,
          crosshairMarkerVisible: false,
        });
        upperLine.setData([
          { time: toTime(startK.dt), value: tr.upper },
          { time: toTime(endK.dt), value: tr.upper },
        ]);

        const lowerLine = mainChart!.addLineSeries({
          color: "rgba(255,152,0,0.4)",
          lineWidth: 1,
          lineStyle: 2,
          priceLineVisible: false,
          lastValueVisible: false,
          crosshairMarkerVisible: false,
        });
        lowerLine.setData([
          { time: toTime(startK.dt), value: tr.lower },
          { time: toTime(endK.dt), value: tr.lower },
        ]);

        // 冰线
        if (settings.value.wyckoff.showIceLine) {
          const iceLine = mainChart!.addLineSeries({
            color: "rgba(3,169,244,0.6)",
            lineWidth: 1,
            lineStyle: 1,
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
  }

  // 威科夫事件 markers
  const eventMarkers = wyckoff.events
    .filter((e) => {
      switch (e.event_type) {
        case "LPS": return settings.value.wyckoff.showLPS;
        case "JOC": return settings.value.wyckoff.showJOC;
        case "Spring": return settings.value.wyckoff.showSpring;
        case "UTAD": return settings.value.wyckoff.showUTAD;
        default: return true;
      }
    })
    .map((e) => {
      const k = data.klines[e.index];
      const isBullish = ["SC", "AR", "Spring", "LPS", "SOS", "JOC"].includes(e.event_type);
      return {
        time: toTime(k?.dt || e.dt),
        position: isBullish ? ("belowBar" as const) : ("aboveBar" as const),
        color: isBullish ? "#00bcd4" : "#ff5722",
        shape: "square" as const,
        text: e.event_type,
      };
    });

  if (eventMarkers.length > 0) {
    const existingMarkers = candleSeries!.markers() || [];
    candleSeries!.setMarkers(
      [...existingMarkers, ...eventMarkers].sort((a, b) =>
        (a.time as string).localeCompare(b.time as string)
      )
    );
  }
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

function selectStock(sym: string) {
  symbol.value = sym;
  showSearch.value = false;
  searchKeyword.value = "";
  loadData();
}

// ===== 事件处理 =====
function onTimeframeChange(tf: TimeFrame) {
  timeframe.value = tf;
  loadData();
}

function onSettingsChange(newSettings: AnalysisSettings) {
  settings.value = newSettings;
  loadData(); // 重新加载并渲染
}

// ===== 生命周期 =====
onMounted(() => {
  nextTick(() => loadData());

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
});

// 监听 timeframe 变化
watch(timeframe, () => loadData());
</script>

<template>
  <!-- 顶部栏 -->
  <header class="flex items-center justify-between px-4 h-12 bg-[#16213e] border-b border-[#2a2a4a] shrink-0">
    <div class="flex items-center gap-4">
      <h1 class="text-lg font-bold text-[#e94560]">墨岩K线</h1>
      <!-- 视图切换 -->
      <div class="flex items-center gap-1">
        <button
          @click="currentView = 'chart'"
          class="px-3 py-1 text-xs rounded transition-all"
          :class="currentView === 'chart' ? 'bg-[#e94560] text-white' : 'text-[#9e9e9e] hover:bg-[#0f3460] hover:text-white'"
        >
          K 线图
        </button>
        <button
          @click="currentView = 'sync'"
          class="px-3 py-1 text-xs rounded transition-all"
          :class="currentView === 'sync' ? 'bg-[#e94560] text-white' : 'text-[#9e9e9e] hover:bg-[#0f3460] hover:text-white'"
        >
          数据同步
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
    </div>

    <div v-if="currentView === 'chart' && currentPrice" class="flex items-center gap-4 text-sm">
      <span class="font-mono text-lg" :class="currentPrice.close >= currentPrice.open ? 'text-[#ef5350]' : 'text-[#26a69a]'">
        {{ currentPrice.close.toFixed(2) }}
      </span>
      <span v-if="priceChange" :class="priceChange.change >= 0 ? 'text-[#ef5350]' : 'text-[#26a69a]'">
        {{ priceChange.change >= 0 ? '+' : '' }}{{ priceChange.change.toFixed(2) }}
        ({{ priceChange.change >= 0 ? '+' : '' }}{{ priceChange.changePct.toFixed(2) }}%)
      </span>
      <span class="text-gray-400">{{ chartData?.name || symbol }}</span>
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
      @timeframe-change="onTimeframeChange"
      @settings-change="onSettingsChange"
    />

    <!-- 主内容区 -->
    <main class="flex-1 flex overflow-hidden">
      <!-- K 线图区域 -->
      <div class="flex-1 flex flex-col relative">
        <!-- 主图 + 成交量 — 始终存在，避免 v-if 销毁容器 -->
        <div ref="chartContainer" class="flex-1 min-h-0"></div>
        <!-- 加载/错误提示 — 覆盖在图表上方 -->
        <div v-if="loading" class="absolute inset-0 flex items-center justify-center bg-[#1a1a2e]/80 z-10">
          <div class="text-[#9e9e9e] animate-pulse">加载中...</div>
        </div>
        <div v-else-if="error" class="absolute inset-0 flex items-center justify-center bg-[#1a1a2e]/80 z-10">
          <div class="text-[#ff5722]">{{ error }}</div>
        </div>
      </div>

      <!-- 右侧设置面板 -->
      <SettingsPanel
        :settings="settings"
        @change="onSettingsChange"
      />
    </main>
  </template>
</template>
