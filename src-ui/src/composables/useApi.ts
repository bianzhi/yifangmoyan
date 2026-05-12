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
