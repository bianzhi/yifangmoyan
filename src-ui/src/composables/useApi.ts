import { invoke } from "@tauri-apps/api/core";
import type { ChartData, StockInfo } from "../types";

export async function getChartData(
  symbol: string,
  timeframe: string,
  enableCzsc: boolean,
  enableWyckoff: boolean
): Promise<ChartData> {
  return invoke<ChartData>("get_chart_data", {
    symbol,
    timeframe,
    enableCzsc,
    enableWyckoff,
  });
}

export async function searchStocks(keyword: string): Promise<StockInfo[]> {
  return invoke<StockInfo[]>("search_stocks", { keyword });
}

export async function getStockInfo(symbol: string): Promise<StockInfo> {
  return invoke<StockInfo>("get_stock_info", { symbol });
}

export async function getSubLevelData(
  symbol: string,
  timeframe: string,
  startDt: string,
  endDt: string,
  enableCzsc: boolean
): Promise<ChartData> {
  return invoke<ChartData>("get_sub_level_data", {
    symbol,
    timeframe,
    startDt,
    endDt,
    enableCzsc,
  });
}

// ===== 数据同步 API =====

export interface LevelStats {
  level: string;
  dir_name: string;
  file_count: number;
  sample_symbol: string | null;
  sample_count: number | null;
  sample_start: string | null;
  sample_end: string | null;
}

export interface DataStatus {
  data_dir: string;
  total_stocks: number;
  levels: LevelStats[];
}

export interface SyncLevelResult {
  level: string;
  status: string;
  count: number;
  msg: string;
}

export interface SyncStockResult {
  symbol: string;
  levels: SyncLevelResult[];
}

export async function getDataStatus(): Promise<DataStatus> {
  return invoke<DataStatus>("get_data_status");
}

export async function syncStock(
  symbol: string,
  levels: string[],
  startDate?: string,
  force: boolean = false
): Promise<SyncStockResult> {
  return invoke<SyncStockResult>("sync_stock", {
    symbol,
    levels,
    startDate: startDate || null,
    force,
  });
}

export async function syncStocksBatch(
  symbols: string[],
  levels: string[],
  startDate?: string,
  force: boolean = false
): Promise<SyncStockResult[]> {
  return invoke<SyncStockResult[]>("sync_stocks_batch", {
    symbols,
    levels,
    startDate: startDate || null,
    force,
  });
}

export async function getAllStockCodes(): Promise<string[]> {
  return invoke<string[]>("get_all_stock_codes");
}

export async function autoSyncOnStartup(levels: string[]): Promise<void> {
  return invoke("auto_sync_on_startup", { levels });
}

export async function getSyncStatus(): Promise<SyncProgress> {
  return invoke<SyncProgress>("get_sync_status");
}

export async function cancelSync(): Promise<void> {
  return invoke("cancel_sync");
}

export interface SyncProgress {
  running: boolean;
  board: string;
  levels: string[];
  total: number;
  completed: number;
  success: number;
  failures: [string, string, string][];
  retrying: boolean;
  retry_round: number;
  cancelled: boolean;
  current_symbols: string[];
  preparing: boolean;
  prepare_error: string;
  all_skipped: boolean;
  skipped_count: number;
  latest_date: string;
}

export interface SyncFailureRecord {
  symbol: string;
  level: string;
  msg: string;
}

export async function getLastSyncFailures(): Promise<SyncFailureRecord[]> {
  return invoke<SyncFailureRecord[]>("get_last_sync_failures");
}

export async function clearSyncFailures(): Promise<void> {
  return invoke("clear_sync_failures");
}

export async function retryFailedSyncs(startDate?: string): Promise<void> {
  return invoke("retry_failed_syncs", { startDate: startDate || null });
}

// ═══════════════════════════════════════════════════════════
//  单股票按需同步（无数据自动同步 / 光标左移历史扩展）
// ═══════════════════════════════════════════════════════════

export interface SingleSyncState {
  symbol: string;
  timeframe: string;
  running: boolean;
  done: boolean;
  status: string;   // "ok" | "fail" | ""
  count: number;
  msg: string;
}

/// 触发后台同步单只股票。start_date 为 None 时同步最新数据。
export async function triggerSingleSync(
  symbol: string,
  timeframe: string,
  startDate?: string,
): Promise<void> {
  return invoke("trigger_single_sync", {
    symbol,
    timeframe,
    startDate: startDate || null,
  });
}

/// 轮询单股票同步状态
export async function pollSingleSync(
  symbol: string,
  timeframe: string,
): Promise<SingleSyncState> {
  return invoke<SingleSyncState>("poll_single_sync", { symbol, timeframe });
}
