// Tauri API 类型声明
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

export interface FenXing {
  fx_type: string;
  index: number;
  dt: string;
  price: number;
}

export interface Bi {
  direction: string;
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
  zs_type: string;
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
  bs_type: string;
  index: number;
  dt: string;
  price: number;
}

export interface BeiChi {
  bc_type: string;
  index: number;
  dt: string;
  direction: string;
}

export interface CzscResult {
  fenxing: FenXing[];
  bi: Bi[];
  xd: XianDuan[];
  bi_zs: ZhongShu[];
  xd_zs: ZhongShu[];
  buy_sell: BuySellPoint[];
  beichi: BeiChi[];
}

export interface TrendLine {
  line_type: string;
  start_index: number;
  end_index: number;
  start_price: number;
  end_price: number;
}

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

export interface WyckoffResult {
  trend_lines: TrendLine[];
  events: WyckoffEvent[];
  trading_ranges: TradingRange[];
}

export interface ChartData {
  symbol: string;
  name: string;
  timeframe: string;
  klines: KLine[];
  macd: MacdData;
  czsc: CzscResult | null;
  wyckoff: WyckoffResult | null;
}

export interface StockInfo {
  symbol: string;
  name: string;
  pinyin: string;
  market: string;
}

// 分析勾选设置
export interface AnalysisSettings {
  czsc: {
    showBi: boolean;
    showXd: boolean;
    showBiZs: boolean;
    showXdZs: boolean;
    showBuySell: boolean;
    showBeichi: boolean;
  };
  wyckoff: {
    showTrendLines: boolean;
    showTR: boolean;
    showIceLine: boolean;
    showLPS: boolean;
    showJOC: boolean;
    showSpring: boolean;
    showUTAD: boolean;
  };
}

export const DEFAULT_SETTINGS: AnalysisSettings = {
  czsc: {
    showBi: true,
    showXd: true,
    showBiZs: true,
    showXdZs: false,
    showBuySell: true,
    showBeichi: true,
  },
  wyckoff: {
    showTrendLines: true,
    showTR: true,
    showIceLine: true,
    showLPS: true,
    showJOC: true,
    showSpring: true,
    showUTAD: true,
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
