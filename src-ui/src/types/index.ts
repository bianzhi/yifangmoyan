// Tauri API 类型声明 — 对齐后端 data/types.rs

export interface KLine {
  symbol: string;
  timeframe: string;
  dt: string;
  id: number;
  open: number;
  close: number;
  high: number;
  low: number;
  vol: number;
  amount: number;
}

export interface MacdData {
  dif: number[];
  dea: number[];
  macd_hist: number[];
}

// ===== 缠论类型 =====

export interface FenXing {
  fx_type: string; // "top" / "bottom"
  index: number;
  dt: string;
  price: number;
}

export interface Bi {
  direction: string; // "up" / "down"
  start_index: number;
  end_index: number;
  start_dt: string;
  end_dt: string;
  start_price: number;
  end_price: number;
  is_finished: boolean;
}

export interface XianDuan {
  direction: string;
  start_index: number;
  end_index: number;
  start_dt: string;
  end_dt: string;
  start_price: number;
  end_price: number;
  is_finished: boolean;
}

export interface ZhongShu {
  zs_type: string; // "bi_zs" / "xd_zs"
  start_index: number;
  end_index: number;
  start_dt: string;
  end_dt: string;
  zg: number;
  zd: number;
  gg: number;
  dd: number;
}

export interface BuySellPoint {
  bs_type: string; // "1buy", "2buy", "3buy", "1sell", "2sell", "3sell"
  index: number;
  dt: string;
  price: number;
}

export interface BeiChi {
  bc_type: string; // "bi_beichi" / "xd_beichi" / "zoushi_beichi"
  index: number;
  dt: string;
  direction: string; // "up" / "down"
  bc_sub_type: string; // "trend" / "panzheng"
  reason: string; // 背驰判断理由
}

export interface ZouShi {
  direction: string;
  zs_type: string; // "trend" / "panzheng"
  start_index: number;
  end_index: number;
  start_dt: string;
  end_dt: string;
  start_price: number;
  end_price: number;
  zs_count: number;
}

export interface QuJianTaoSignal {
  level: string;
  index: number;
  dt: string;
  signal_type: string;
  direction: string;
  higher_bc_type: string;
  lower_bc_type: string;
}

export interface CzscResult {
  fenxing: FenXing[];
  bi: Bi[];
  xd: XianDuan[];
  bi_zs: ZhongShu[];
  xd_zs: ZhongShu[];
  buy_sell: BuySellPoint[];
  beichi: BeiChi[];
  zoushi: ZouShi[];
  qujian_tao: QuJianTaoSignal[];
}

// ===== 威科夫类型 =====

export interface WyckoffEvent {
  event_type: string;
  index: number;
  dt: string;
  price: number;
  description: string;
}

export interface TradingRange {
  start_index: number;
  end_index: number;
  upper: number;
  lower: number;
  ice_line: number;
}

export interface TrendLine {
  line_type: string; // "support" / "resistance" / "channel"
  start_index: number;
  end_index: number;
  start_price: number;
  end_price: number;
}

export type WyckoffPhase =
  | "Accumulation"
  | "Markup"
  | "Distribution"
  | "Markdown"
  | "Unknown";

export interface PhaseLabel {
  index: number;
  dt: string;
  phase: WyckoffPhase;
  sub_phase: string;
}

export interface EffortResult {
  index: number;
  dt: string;
  effort: number;
  result: number;
  harmony: string; // "harmonious" / "divergent"
  interpretation: string; // "demand_dominant" / "supply_dominant" / "neutral"
}

export interface SupplyDemandLine {
  line_type: string; // "supply" / "demand"
  start_index: number;
  end_index: number;
  start_price: number;
  end_price: number;
  slope: number;
}

export interface WyckoffResult {
  phase_labels: PhaseLabel[];
  events: WyckoffEvent[];
  trading_ranges: TradingRange[];
  trend_lines: TrendLine[];
  supply_demand_lines: SupplyDemandLine[];
  effort_results: EffortResult[];
}

// ===== 融合信号 =====

export interface FusionSignal {
  czsc_type: string; // 买卖点类型
  wyckoff_events: string[]; // 关联的威科夫事件
  index: number;
  dt: string;
  price: number;
  interpretation: string;
  strength: number; // 1-5 星
  direction: string; // "bullish" / "bearish"
}

export interface FusionResult {
  signals: FusionSignal[];
}

// ===== 聚合类型 =====

export interface ChartData {
  symbol: string;
  name: string;
  timeframe: string;
  klines: KLine[];
  macd: MacdData;
  czsc: CzscResult | null;
  wyckoff: WyckoffResult | null;
  fusion: FusionResult | null;
}

export interface StockInfo {
  symbol: string;
  name: string;
  pinyin: string;
  market: string;
}

// ===== 分析勾选设置 =====

export interface LineStyle {
  color: string;
  lineWidth: number;
}

export interface ZhongShuStyle {
  borderColor: string;
  borderWidth: number;
  fillColor: string;
}

export interface ChartStyles {
  bi: LineStyle;
  xd: LineStyle;
  biZs: ZhongShuStyle;
  xdZs: ZhongShuStyle;
}

export const DEFAULT_CHART_STYLES: ChartStyles = {
  bi: { color: "#4a90d9", lineWidth: 1 },
  xd: { color: "#b388ff", lineWidth: 3 },
  biZs: { borderColor: "#b388ff", borderWidth: 2, fillColor: "rgba(179,136,255,0.08)" },
  xdZs: { borderColor: "#ff9800", borderWidth: 2, fillColor: "rgba(255,152,0,0.08)" },
};

export interface AnalysisSettings {
  czsc: {
    showFenxing: boolean;
    showBi: boolean;
    showXd: boolean;
    showBiZs: boolean;
    showXdZs: boolean;
    show1buy: boolean;
    show2buy: boolean;
    show3buy: boolean;
    show1sell: boolean;
    show2sell: boolean;
    show3sell: boolean;
    showBeichi: boolean;
  };
  wyckoff: {
    showPhase: boolean;
    showTR: boolean;
    showIceLine: boolean;
    showSupplyDemand: boolean;
    showSC: boolean;
    showAR: boolean;
    showST: boolean;
    showSpring: boolean;
    showSOS: boolean;
    showLPS: boolean;
    showJOC: boolean;
    showPSY: boolean;
    showBC: boolean;
    showUTAD: boolean;
    showSOW: boolean;
    showLPSY: boolean;
  };
  fusion: {
    showFusion: boolean;
  };
  chart: {
    showMacd: boolean;
  };
  styles: ChartStyles;
}

export const DEFAULT_SETTINGS: AnalysisSettings = {
  czsc: {
    showFenxing: false,
    showBi: true,
    showXd: true,
    showBiZs: true,
    showXdZs: false,
    show1buy: true, show2buy: true, show3buy: true, show1sell: true, show2sell: true, show3sell: true,
    showBeichi: true,
  },
  wyckoff: {
    showPhase: true,
    showTR: true,
    showIceLine: true,
    showSupplyDemand: true,
    showSC: true,
    showAR: true,
    showST: true,
    showSpring: true,
    showSOS: false,
    showLPS: true,
    showJOC: true,
    showPSY: true,
    showBC: true,
    showUTAD: true,
    showSOW: false,
    showLPSY: false,
  },
  fusion: {
    showFusion: true,
  },
  chart: {
    showMacd: true,
  },
  styles: { ...DEFAULT_CHART_STYLES },
};

// ===== 视图模式 =====

export type ViewMode = "pure" | "czsc" | "wyckoff" | "fusion";

export const VIEW_MODE_SETTINGS: Record<ViewMode, Omit<AnalysisSettings, "styles">> = {
  pure: {
    czsc: {
      showFenxing: false, showBi: false, showXd: false,
      showBiZs: false, showXdZs: false, show1buy: false, show2buy: false, show3buy: false, show1sell: false, show2sell: false, show3sell: false, showBeichi: false,
    },
    wyckoff: {
      showPhase: false, showTR: false, showIceLine: false, showSupplyDemand: false,
      showSC: false, showAR: false, showST: false, showSpring: false,
      showSOS: false, showLPS: false, showJOC: false,
      showPSY: false, showBC: false, showUTAD: false, showSOW: false, showLPSY: false,
    },
    fusion: { showFusion: false },
    chart: { showMacd: true },
  },
  czsc: {
    czsc: {
      showFenxing: false, showBi: true, showXd: true,
      showBiZs: true, showXdZs: false, show1buy: true, show2buy: true, show3buy: true, show1sell: true, show2sell: true, show3sell: true, showBeichi: true,
    },
    wyckoff: {
      showPhase: false, showTR: false, showIceLine: false, showSupplyDemand: false,
      showSC: false, showAR: false, showST: false, showSpring: false,
      showSOS: false, showLPS: false, showJOC: false,
      showPSY: false, showBC: false, showUTAD: false, showSOW: false, showLPSY: false,
    },
    fusion: { showFusion: false },
    chart: { showMacd: true },
  },
  wyckoff: {
    czsc: {
      showFenxing: false, showBi: false, showXd: false,
      showBiZs: false, showXdZs: false, show1buy: false, show2buy: false, show3buy: false, show1sell: false, show2sell: false, show3sell: false, showBeichi: false,
    },
    wyckoff: {
      showPhase: true, showTR: true, showIceLine: true, showSupplyDemand: true,
      showSC: true, showAR: true, showST: true, showSpring: true,
      showSOS: true, showLPS: true, showJOC: true,
      showPSY: true, showBC: true, showUTAD: true, showSOW: true, showLPSY: true,
    },
    fusion: { showFusion: false },
    chart: { showMacd: true },
  },
  fusion: {
    czsc: {
      showFenxing: false, showBi: true, showXd: true,
      showBiZs: true, showXdZs: false, show1buy: true, show2buy: true, show3buy: true, show1sell: true, show2sell: true, show3sell: true, showBeichi: true,
    },
    wyckoff: {
      showPhase: true, showTR: true, showIceLine: true, showSupplyDemand: true,
      showSC: true, showAR: true, showST: true, showSpring: true,
      showSOS: false, showLPS: true, showJOC: true,
      showPSY: true, showBC: true, showUTAD: true, showSOW: false, showLPSY: false,
    },
    fusion: { showFusion: true },
    chart: { showMacd: true },
  },
};

export type TimeFrame = "m" | "w" | "d" | "f60" | "f30" | "f15" | "f5" | "f1";

export const TIME_FRAMES: { key: TimeFrame; label: string }[] = [
  { key: "m", label: "月线" },
  { key: "w", label: "周线" },
  { key: "d", label: "日线" },
  { key: "f60", label: "60F" },
  { key: "f30", label: "30F" },
  { key: "f15", label: "15F" },
  { key: "f5", label: "5F" },
  { key: "f1", label: "1F" },
];

// ===== 威科夫事件类型分类 =====

export const WYCKOFF_BULLISH_EVENTS = ["SC", "AR", "ST", "Spring", "SOS", "LPS", "JOC", "Shakeout"];
export const WYCKOFF_BEARISH_EVENTS = ["PSY", "BC", "UTAD", "SOW", "LPSY"];
export const WYCKOFF_ALL_EVENTS = [...WYCKOFF_BULLISH_EVENTS, ...WYCKOFF_BEARISH_EVENTS];

export const WYCKOFF_EVENT_COLORS: Record<string, string> = {
  SC: "#ff5722",
  AR: "#4caf50",
  ST: "#00bcd4",
  Spring: "#ff9800",
  Shakeout: "#ff9800",
  SOS: "#2e7d32",
  LPS: "#66bb6a",
  JOC: "#00e676",
  PSY: "#9c27b0",
  BC: "#e91e63",
  UTAD: "#7b1fa2",
  SOW: "#c62828",
  LPSY: "#ef5350",
};

export const WYCKOFF_PHASE_COLORS: Record<string, string> = {
  Accumulation: "#00bcd4",
  Markup: "#4caf50",
  Distribution: "#9c27b0",
  Markdown: "#78909c",
  Unknown: "#424242",
};

// ===== 缠论买卖点颜色 =====

export const CZSC_BS_COLORS: Record<string, { color: string; text: string; shape: string; label: string }> = {
  "1buy":  { color: "#00e676", text: "1B", shape: "arrowUp",   label: "一买" },
  "2buy":  { color: "#69f0ae", text: "2B", shape: "circle",     label: "二买" },
  "3buy":  { color: "#b9f6ca", text: "3B", shape: "square",     label: "三买" },
  "1sell": { color: "#ff1744", text: "1S", shape: "arrowDown",  label: "一卖" },
  "2sell": { color: "#ff5252", text: "2S", shape: "circle",     label: "二卖" },
  "3sell": { color: "#ff8a80", text: "3S", shape: "square",     label: "三卖" },
};
