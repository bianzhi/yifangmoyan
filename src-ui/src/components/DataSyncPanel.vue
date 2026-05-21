<script setup lang="ts">
import { ref, onMounted, computed, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

// ═══════════════════════════════════════════════════════════════
//  类型
// ═══════════════════════════════════════════════════════════════

interface LevelStats { level: string; dir_name: string; file_count: number; sample_symbol: string | null; sample_count: number | null; sample_start: string | null; sample_end: string | null; }
interface BoardStats { id: string; name: string; count: number; }
interface BoardOnlineInfo { id: string; name: string; total_count: number; local_count: number; latest_date: string; }
interface DataStatus { data_dir: string; total_stocks: number; levels: LevelStats[]; boards: BoardStats[]; }
interface ValidationIssue { severity: string; category: string; row_index: number | null; datetime: string | null; message: string; }
interface ValidateLevelResult { level: string; total_rows: number; issues: ValidationIssue[]; score: number; }
interface ValidateStockResult { symbol: string; levels: ValidateLevelResult[]; overall_score: number; }
interface MoveDataResult { moved: number; failed: number; errors: string[]; }
interface TrimResult { trimmed_files: number; removed_files: number; rows_before: number; rows_after: number; }

// 数据保留配置
interface RetentionConfig {
  f1_months: number;   // 1分线保留月数，默认3
  f5_months: number;   // 5分线保留月数，默认3
  f15_months: number;  // 15分线保留月数，默认6
  f30_months: number;  // 30分线保留月数，默认6
}

const DEFAULT_RETENTION: RetentionConfig = {
  f1_months: 3,
  f5_months: 3,
  f15_months: 6,
  f30_months: 6,
};

// ═══════════════════════════════════════════════════════════════
//  常量
// ═══════════════════════════════════════════════════════════════

const BOARDS = [
  { id: "sh_main", name: "沪主板", icon: "上证", color: "#ef5350" },
  { id: "sz_main", name: "深主板", icon: "深证", color: "#42a5f5" },
  { id: "gem",     name: "创业板", icon: "创业", color: "#ffa726" },
  { id: "star",    name: "科创板", icon: "科创", color: "#ab47bc" },
  { id: "bse",     name: "北交所", icon: "北交", color: "#66bb6a" },
] as const;

const LEVELS = [
  { key: "m",   label: "月线" },
  { key: "w",   label: "周线" },
  { key: "d",   label: "日线" },
  { key: "f60", label: "60分" },
  { key: "f30", label: "30分" },
  { key: "f15", label: "15分" },
  { key: "f5",  label: "5分" },
  { key: "f1",  label: "1分" },
] as const;

// ═══════════════════════════════════════════════════════════════
//  状态
// ═══════════════════════════════════════════════════════════════

const dataStatus = ref<DataStatus | null>(null);
const boardOnlineInfo = ref<BoardOnlineInfo[]>([]);
const loadingStatus = ref(true);
const loadingOnlineInfo = ref(false);

// 同步
const syncing = ref(false);
const syncPreparing = ref(false); // 正在获取股票列表
const syncingBoard = ref<string | null>(null);
const syncTotal = ref(0);
const syncCompleted = ref(0);
const syncFailedDetails = ref<{ symbol: string; level: string; msg: string }[]>([]);
const currentSymbols = ref<string[]>([]);
const syncRetrying = ref(false);
const syncRetryRound = ref(0);
const syncStartTime = ref(0);
const syncElapsed = ref("");
let syncTimer: ReturnType<typeof setInterval> | null = null;
let syncPollTimer: ReturnType<typeof setInterval> | null = null;

// 同步选项
const selectedLevels = ref<string[]>(["d"]);
const START_DATE_KEY = "moyan_start_date";
function loadStartDate(): string {
  try {
    const raw = localStorage.getItem(START_DATE_KEY);
    if (raw) return raw;
  } catch { /* ignore */ }
  return "2024-01-01";
}
const startDate = ref(loadStartDate());
watch(startDate, (val) => {
  localStorage.setItem(START_DATE_KEY, val);
});
const forceSync = ref(false);
const selectedBoard = ref("all_a");

// 上一次同步结果
const lastSyncSuccess = ref(0);
const lastSyncFailed = ref(0);
const lastSyncElapsed = ref("");
const lastSyncLatestDate = ref("");
const lastSyncStockCount = ref(0);
const syncAllSkipped = ref(false);
const showSyncResult = ref(false);

// 后台同步状态（仅用于横幅提示，不自动进入 syncing 页面状态）
const bgSyncRunning = ref(false);
const bgSyncBoard = ref<string | null>(null);
const bgSyncTotal = ref(0);
const bgSyncCompleted = ref(0);
const bgSyncRetrying = ref(false);
const bgSyncRetryRound = ref(0);

// 校验
const validateSymbol = ref("");
const validateResults = ref<ValidateStockResult[]>([]);
const validating = ref(false);
const validateLevels = ref<string[]>(["d", "w", "m"]);
const crossValidateSymbol = ref("");
const crossValidateLevel = ref("d");
const crossValidateResult = ref<ValidateLevelResult | null>(null);
const crossValidating = ref(false);
const fullValidating = ref(false);
const fullValidateCompleted = ref(0);
const fullValidateTotal = ref(0);
const fullValidateResults = ref<ValidateStockResult[]>([]);

// 目录
const currentDataDir = ref("");
const movingData = ref(false);
const moveResult = ref<MoveDataResult | null>(null);

// 清理退市股
const cleaningDelisted = ref(false);
const delistedResult = ref<{ codes: string[]; files: number } | null>(null);

// 数据保留配置（持久化到 localStorage）
const RETENTION_KEY = "moyan_retention_config";
function loadRetention(): RetentionConfig {
  try {
    const raw = localStorage.getItem(RETENTION_KEY);
    if (raw) return { ...DEFAULT_RETENTION, ...JSON.parse(raw) };
  } catch { /* ignore */ }
  return { ...DEFAULT_RETENTION };
}
const retentionConfig = ref<RetentionConfig>(loadRetention());
watch(retentionConfig, (val) => {
  localStorage.setItem(RETENTION_KEY, JSON.stringify(val));
}, { deep: true });

// 清空 / 清理状态
const clearingData = ref(false);
const trimmingData = ref(false);
const trimResult = ref<TrimResult | null>(null);
const showClearConfirm = ref(false);

// UI
const error = ref("");
const showAdvanced = ref(false); // 展开高级设置
const activeTab = ref<"overview" | "validate" | "manage">("overview");

// ═══════════════════════════════════════════════════════════════
//  计算属性
// ═══════════════════════════════════════════════════════════════

const syncPercent = computed(() =>
  syncTotal.value > 0 ? Math.round((syncCompleted.value / syncTotal.value) * 100) : 0
);

const allAOnlineTotal = computed(() => {
  const info = boardOnlineInfo.value.find(b => b.id === "all_a");
  if (info && info.total_count > 0) return info.total_count;
  const sum = boardOnlineInfo.value.reduce((s, b) => s + b.total_count, 0);
  return sum > 0 ? sum : null;
});

const allALocalCount = computed(() => {
  const info = boardOnlineInfo.value.find(b => b.id === "all_a");
  if (info && info.local_count > 0) return info.local_count;
  return dataStatus.value?.total_stocks ?? 0;
});

const allASyncPercent = computed(() => {
  const total = allAOnlineTotal.value;
  if (!total) return 0;
  return Math.min(Math.round((allALocalCount.value / total) * 100), 100);
});

const syncSpeed = computed(() => {
  if (!syncing.value || syncCompleted.value === 0) return null;
  const elapsed = (Date.now() - syncStartTime.value) / 1000 / 60;
  if (elapsed < 0.05) return null;
  return Math.round(syncCompleted.value / elapsed);
});

const syncETA = computed(() => {
  const speed = syncSpeed.value;
  if (!speed || speed === 0) return null;
  const remaining = syncTotal.value - syncCompleted.value;
  const mins = remaining / speed;
  if (mins < 1) return `${Math.round(mins * 60)}秒`;
  if (mins < 60) return `${Math.round(mins)}分钟`;
  return `${Math.floor(mins / 60)}时${Math.round(mins % 60)}分`;
});

function boardOnlineTotal(id: string): number | null {
  const info = boardOnlineInfo.value.find(b => b.id === id);
  return info && info.total_count > 0 ? info.total_count : null;
}

function boardLocalCount(id: string): number {
  const info = boardOnlineInfo.value.find(b => b.id === id);
  if (info && info.local_count > 0) return info.local_count;
  if (dataStatus.value) {
    const board = dataStatus.value.boards.find(b => b.id === id);
    if (board) return board.count;
  }
  return 0;
}

function boardLatestDate(id: string): string {
  const info = boardOnlineInfo.value.find(b => b.id === id);
  return info?.latest_date ?? "";
}

function boardPercent(id: string): number {
  const total = boardOnlineTotal(id);
  const local = boardLocalCount(id);
  if (!total || total === 0) return 0;
  return Math.min(Math.round((local / total) * 100), 100);
}

function selectedBoardLabel(): string {
  if (selectedBoard.value === "all_a") return "全 A 股";
  return BOARDS.find(b => b.id === selectedBoard.value)?.name ?? selectedBoard.value;
}

function selectedBoardCount(): number {
  if (selectedBoard.value === "all_a") return allAOnlineTotal.value ?? 0;
  return boardOnlineTotal(selectedBoard.value) ?? 0;
}

// 总体进度颜色
function progressColor(pct: number): string {
  if (pct >= 100) return "#26a69a";
  if (pct >= 50) return "#ff9800";
  return "#e94560";
}

// ═══════════════════════════════════════════════════════════════
//  数据加载
// ═══════════════════════════════════════════════════════════════

async function refreshStatus() {
  loadingStatus.value = true;
  try {
    dataStatus.value = await invoke<DataStatus>("get_data_status");
    currentDataDir.value = dataStatus.value.data_dir;
  } catch (e: any) {
    error.value = e.toString();
  } finally {
    loadingStatus.value = false;
  }
}

async function loadBoardOnlineInfo() {
  loadingOnlineInfo.value = true;
  try {
    boardOnlineInfo.value = await invoke<BoardOnlineInfo[]>("get_board_online_info");
  } catch (e: any) {
    console.error("获取在线信息失败:", e);
  } finally {
    loadingOnlineInfo.value = false;
  }
}

// ═══════════════════════════════════════════════════════════════
//  同步
// ═══════════════════════════════════════════════════════════════

function toggleLevel(key: string) {
  const idx = selectedLevels.value.indexOf(key);
  if (idx >= 0) selectedLevels.value.splice(idx, 1);
  else selectedLevels.value.push(key);
}

function startSyncTimer() {
  syncStartTime.value = Date.now();
  syncTimer = setInterval(() => {
    const elapsed = Date.now() - syncStartTime.value;
    const s = Math.floor(elapsed / 1000);
    if (s < 60) syncElapsed.value = `${s}秒`;
    else if (s < 3600) syncElapsed.value = `${Math.floor(s / 60)}分${s % 60}秒`;
    else syncElapsed.value = `${Math.floor(s / 3600)}时${Math.floor((s % 3600) / 60)}分`;
  }, 1000);
}

function stopSyncTimer() {
  if (syncTimer) { clearInterval(syncTimer); syncTimer = null; }
}

async function startSync() {
  if (syncing.value) return;

  if (selectedLevels.value.length === 0) {
    error.value = "请至少选择一个同步级别";
    return;
  }

  const boardId = selectedBoard.value;
  syncingBoard.value = boardId;
  error.value = "";
  syncing.value = true;
  syncPreparing.value = true; // 正在获取股票列表或快速检查
  syncTotal.value = 0;
  syncCompleted.value = 0;
  syncFailedDetails.value = [];
  currentSymbols.value = [];
  syncRetrying.value = false;
  syncRetryRound.value = 0;
  syncElapsed.value = "0秒";
  showSyncResult.value = false;
  startSyncTimer();

  try {
    await invoke("start_sync_board", {
      board: boardId,
      levels: selectedLevels.value,
      startDate: startDate.value || null,
      force: forceSync.value,
    });
    // start_sync_board 非阻塞，立即返回。所有工作（快速检查、获取列表、同步）在后台线程。
    // 统一走轮询获取状态，不再单独 get_sync_status 判断 all_skipped
    startStatusPolling();
  } catch (e: any) {
    error.value = `启动同步失败: ${e}`;
    syncing.value = false;
    syncPreparing.value = false;
    syncingBoard.value = null;
    stopSyncTimer();
  }
}

/** 轮询后端同步状态，直到同步完成/失败/取消 */
function startStatusPolling() {
  // 清理旧的轮询
  if (syncPollTimer) { clearInterval(syncPollTimer); syncPollTimer = null; }

  const poll = async () => {
    try {
      const status = await invoke<{
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
      }>("get_sync_status");

      // 更新前端状态
      syncTotal.value = status.total;
      syncCompleted.value = status.completed;
      currentSymbols.value = status.current_symbols ?? [];
      syncRetrying.value = status.retrying;
      syncRetryRound.value = status.retry_round;
      syncPreparing.value = status.preparing;
      syncFailedDetails.value = status.failures.map(f => ({
        symbol: f[0],
        level: f[1],
        msg: f[2],
      }));

      // 检查获取列表是否失败
      if (status.prepare_error && !status.running) {
        error.value = status.prepare_error;
        syncing.value = false;
        syncingBoard.value = null;
        stopSyncTimer();
        if (syncPollTimer) { clearInterval(syncPollTimer); syncPollTimer = null; }
        return;
      }

      // 同步完成（非 preparing 且非 running）
      if (!status.running && !status.preparing) {
        if (syncPollTimer) { clearInterval(syncPollTimer); syncPollTimer = null; }
        lastSyncSuccess.value = status.success;
        lastSyncFailed.value = status.failures.length;
        lastSyncElapsed.value = status.all_skipped ? "已是最新" : syncElapsed.value;
        lastSyncLatestDate.value = status.latest_date;
        lastSyncStockCount.value = status.skipped_count;
        syncAllSkipped.value = status.all_skipped;
        syncing.value = false;
        syncingBoard.value = null;
        stopSyncTimer();
        showSyncResult.value = true;
        setTimeout(() => { showSyncResult.value = false; }, 5_000);
        refreshStatus();
        loadBoardOnlineInfo();
      }
    } catch {
      // 轮询失败不中断，继续重试
    }
  };

  // 立即调一次（快速检查几百毫秒就完成的情况，不用等1.5秒）
  poll();
  syncPollTimer = setInterval(poll, 1_500);
}

function cancelSync() {
  invoke("cancel_sync").catch(() => {});
  if (syncPollTimer) { clearInterval(syncPollTimer); syncPollTimer = null; }
  syncing.value = false;
  syncPreparing.value = false;
  syncingBoard.value = null;
  stopSyncTimer();
}

/** 停止后台同步（横幅按钮触发） */
async function stopBgSync() {
  try {
    await invoke("cancel_sync");
  } catch { /* ignore */ }
  bgSyncRunning.value = false;
  bgSyncBoard.value = null;
  bgSyncTotal.value = 0;
  bgSyncCompleted.value = 0;
  await refreshStatus();
}

// ═══════════════════════════════════════════════════════════════
//  校验
// ═══════════════════════════════════════════════════════════════

function toggleValidateLevel(key: string) {
  const idx = validateLevels.value.indexOf(key);
  if (idx >= 0) validateLevels.value.splice(idx, 1);
  else validateLevels.value.push(key);
}

async function validateSingleStock() {
  if (!validateSymbol.value.trim()) return;
  validating.value = true;
  error.value = "";
  try {
    validateResults.value = [await invoke<ValidateStockResult>("validate_stock", {
      symbol: validateSymbol.value.trim(),
      levels: validateLevels.value,
    })];
  } catch (e: any) {
    error.value = e.toString();
  } finally {
    validating.value = false;
  }
}

async function doCrossValidate() {
  if (!crossValidateSymbol.value.trim()) return;
  crossValidating.value = true;
  crossValidateResult.value = null;
  error.value = "";
  try {
    crossValidateResult.value = await invoke<ValidateLevelResult>("cross_validate_stock", {
      symbol: crossValidateSymbol.value.trim(),
      level: crossValidateLevel.value,
    });
  } catch (e: any) {
    error.value = e.toString();
  } finally {
    crossValidating.value = false;
  }
}

async function fullValidate() {
  fullValidating.value = true;
  fullValidateResults.value = [];
  fullValidateCompleted.value = 0;
  error.value = "";

  let codes: string[];
  try {
    codes = await invoke<string[]>("get_all_stock_codes");
  } catch (e: any) {
    error.value = `获取股票列表失败: ${e}`;
    fullValidating.value = false;
    return;
  }

  fullValidateTotal.value = codes.length;
  const batchSize = 5;

  for (let i = 0; i < codes.length; i += batchSize) {
    const batch = codes.slice(i, i + batchSize);
    try {
      const results = await invoke<ValidateStockResult[]>("validate_stocks_batch", {
        symbols: batch,
        levels: validateLevels.value,
      });
      for (const r of results) {
        if (r.overall_score < 0.8) fullValidateResults.value.push(r);
      }
    } catch (e: any) {
      error.value = e.toString();
    }
    fullValidateCompleted.value = Math.min(fullValidateCompleted.value + batchSize, codes.length);
    if (i + batchSize < codes.length) await new Promise(r => setTimeout(r, 100));
  }
  fullValidating.value = false;
}

// ═══════════════════════════════════════════════════════════════
//  目录
// ═══════════════════════════════════════════════════════════════

async function selectFolder() {
  try {
    const selected = await open({ directory: true, multiple: false, title: "选择数据存储目录" });
    if (selected) {
      await invoke<string>("set_data_dir", { path: typeof selected === "string" ? selected : selected });
      await refreshStatus();
    }
  } catch (e: any) {
    error.value = `选择目录失败: ${e}`;
  }
}

async function moveDataToFolder() {
  try {
    const selected = await open({ directory: true, multiple: false, title: "选择新数据目录" });
    if (!selected) return;
    movingData.value = true;
    moveResult.value = null;
    error.value = "";
    moveResult.value = await invoke<MoveDataResult>("move_data_dir", {
      newPath: typeof selected === "string" ? selected : selected,
    });
    await refreshStatus();
  } catch (e: any) {
    error.value = `迁移数据失败: ${e}`;
  } finally {
    movingData.value = false;
  }
}

async function openDataDir() {
  try {
    await invoke("open_data_dir");
  } catch (e: any) {
    error.value = `打开目录失败: ${e}`;
  }
}

// ═══════════════════════════════════════════════════════════════
//  数据清空与清理
// ═══════════════════════════════════════════════════════════════

async function doClearAllData() {
  clearingData.value = true;
  error.value = "";
  showClearConfirm.value = false;
  try {
    const count = await invoke<number>("clear_all_data");
    await refreshStatus();
    loadBoardOnlineInfo();
    error.value = `已清空 ${count} 个数据文件`;
  } catch (e: any) {
    error.value = `清空数据失败: ${e}`;
  } finally {
    clearingData.value = false;
  }
}

async function doTrimOldData() {
  trimmingData.value = true;
  trimResult.value = null;
  error.value = "";
  try {
    // 将 retentionConfig 映射为后端需要的 tf_dir_name -> months
    const retention: Record<string, number> = {
      "1m": retentionConfig.value.f1_months,
      "5m": retentionConfig.value.f5_months,
      "15m": retentionConfig.value.f15_months,
      "30m": retentionConfig.value.f30_months,
    };
    trimResult.value = await invoke<TrimResult>("trim_old_data", { retention });
    await refreshStatus();
  } catch (e: any) {
    error.value = `清理过期数据失败: ${e}`;
  } finally {
    trimmingData.value = false;
  }
}

// 清理退市股
async function doCleanDelisted() {
  cleaningDelisted.value = true;
  delistedResult.value = null;
  error.value = "";
  try {
    const result = await invoke<{ delisted_codes: string[]; removed_files: number }>("clean_delisted_stocks");
    delistedResult.value = { codes: result.delisted_codes, files: result.removed_files };
    await refreshStatus();
  } catch (e: any) {
    error.value = `清理退市股失败: ${e}`;
  } finally {
    cleaningDelisted.value = false;
  }
}

// ═══════════════════════════════════════════════════════════════
//  辅助
// ═══════════════════════════════════════════════════════════════

function scoreColor(score: number) {
  if (score >= 0.9) return "#26a69a";
  if (score >= 0.7) return "#ff9800";
  return "#ff5722";
}

function severityColor(s: string) {
  return s === "error" ? "#ff5722" : s === "warning" ? "#ff9800" : "#2196f3";
}

// ═══════════════════════════════════════════════════════════════
//  初始化
// ═══════════════════════════════════════════════════════════════

onMounted(async () => {
  // 两个请求并行发起，不阻塞 UI 渲染
  refreshStatus();
  loadBoardOnlineInfo();

  // 检查是否有后台同步正在运行
  // 注意：不自动进入 syncing 页面状态，只显示顶部横幅提示
  try {
    const status = await invoke<{
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
    }>("get_sync_status");

    if (status.running && !status.cancelled) {
      bgSyncRunning.value = true;
      bgSyncBoard.value = status.board;
      bgSyncTotal.value = status.total;
      bgSyncCompleted.value = status.completed;

      // 后台轮询更新横幅进度
      const pollTimer = setInterval(async () => {
        try {
          const s = await invoke<{
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
          }>("get_sync_status");

          bgSyncTotal.value = s.total;
          bgSyncCompleted.value = s.completed;
          bgSyncRetrying.value = s.retrying;
          bgSyncRetryRound.value = s.retry_round;

          if (!s.running) {
            clearInterval(pollTimer);
            bgSyncRunning.value = false;
            bgSyncBoard.value = null;
            refreshStatus();
            loadBoardOnlineInfo();
          }
        } catch {
          // 轮询失败不中断
        }
      }, 2_000);
    }
  } catch {
    // 获取状态失败不影响页面
  }
});
</script>

<template>
  <div class="h-full flex flex-col bg-[#1a1a2e]">

    <!-- 后台同步横幅 -->
    <div v-if="bgSyncRunning && !syncing" class="shrink-0 px-4 py-2 bg-[#e94560]/10 border-b border-[#e94560]/20 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full animate-pulse" :class="bgSyncRetrying ? 'bg-[#ff9800]' : 'bg-[#e94560]'"></div>
        <span class="text-xs" :class="bgSyncRetrying ? 'text-[#ff9800]' : 'text-[#e94560]'">
          {{ bgSyncRetrying ? `重试中 第${bgSyncRetryRound}轮（${bgSyncBoard || '全A股'}）` : `后台同步中（${bgSyncBoard || '全A股'}）` }}
          {{ bgSyncCompleted }}/{{ bgSyncTotal }}
        </span>
      </div>
      <button @click="stopBgSync" class="text-[10px] px-2 py-0.5 rounded border border-[#e94560]/40 text-[#e94560] hover:bg-[#e94560]/20 transition">
        停止
      </button>
    </div>

    <!-- ═══════════════════════════════════════════════════════════
         同步中：顶部紧凑进度条（不遮挡已有数据）
         ═══════════════════════════════════════════════════════════ -->
    <div v-if="syncing" class="shrink-0 bg-[#16213e] border-b border-[#e94560]/20">
      <!-- 第一行：状态 + 百分比 + 取消 -->
      <div class="px-4 py-2 flex items-center justify-between">
        <div class="flex items-center gap-2">
          <div class="w-2 h-2 rounded-full animate-pulse" :class="syncRetrying ? 'bg-[#ff9800]' : 'bg-[#e94560]'"></div>
          <span class="text-xs font-bold" :class="syncRetrying ? 'text-[#ff9800]' : 'text-[#e94560]'">
            <template v-if="syncPreparing">正在获取{{ selectedBoardLabel() }}股票列表...</template>
            <template v-else-if="syncRetrying">重试中 第{{ syncRetryRound }}轮</template>
            <template v-else>正在同步{{ selectedBoardLabel() }}</template>
          </span>
          <span v-if="!syncPreparing" class="text-lg font-black text-white tabular-nums">{{ syncPercent }}%</span>
        </div>
        <div class="flex items-center gap-3">
          <template v-if="!syncPreparing">
            <span class="text-[10px] text-[#9e9e9e]">{{ syncElapsed }}<span v-if="syncETA" class="text-[#ff9800]"> · 约{{ syncETA }}</span></span>
            <span class="text-[10px] text-[#666] font-mono tabular-nums">{{ syncCompleted }}/{{ syncTotal }}</span>
          </template>
          <button @click="cancelSync" class="text-[10px] px-2 py-0.5 rounded border border-[#e94560]/40 text-[#e94560] hover:bg-[#e94560]/20 transition">取消</button>
        </div>
      </div>
      <!-- 第二行：进度条 -->
      <div class="px-4 pb-2">
        <div class="w-full h-1.5 bg-[#0f3460] rounded-full overflow-hidden">
          <div v-if="syncPreparing" class="h-full rounded-full bg-[#e94560] animate-pulse" style="width: 30%"></div>
          <div v-else class="h-full rounded-full transition-all duration-300"
            :style="{ width: `${syncPercent}%`, backgroundColor: progressColor(syncPercent) }"></div>
        </div>
      </div>
      <!-- 第三行：成功/失败/剩余 + 当前同步符号 -->
      <div v-if="!syncPreparing" class="px-4 pb-2 flex items-center justify-between text-[10px]">
        <div class="flex items-center gap-3">
          <span class="text-[#26a69a]">{{ syncCompleted - syncFailedDetails.length }} 成功</span>
          <span :class="syncFailedDetails.length > 0 ? 'text-[#ff5722]' : 'text-[#555]'">{{ syncFailedDetails.length }} 失败</span>
          <span class="text-[#666]">{{ syncTotal - syncCompleted }} 剩余</span>
        </div>
        <div v-if="currentSymbols.length > 0" class="flex items-center gap-1 overflow-hidden">
          <span v-for="sym in currentSymbols.slice(0, 5)" :key="sym" class="bg-[#0f3460] px-1 py-0.5 rounded text-white font-mono whitespace-nowrap">{{ sym }}</span>
          <span v-if="currentSymbols.length > 5" class="text-[#666]">+{{ currentSymbols.length - 5 }}</span>
        </div>
      </div>
    </div>

    <!-- ═══════════════════════════════════════════════════════════
         同步完成：短暂成功提示（5秒后自动消失）
         ═══════════════════════════════════════════════════════════ -->
    <div v-if="showSyncResult" class="shrink-0 px-4 py-2 border-b flex items-center justify-between"
      :class="syncAllSkipped ? 'bg-[#2196f3]/10 border-[#2196f3]/20' : 'bg-[#26a69a]/10 border-[#26a69a]/20'">
      <div class="flex items-center gap-2">
        <svg v-if="syncAllSkipped" class="w-4 h-4 text-[#2196f3]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"/>
        </svg>
        <svg v-else class="w-4 h-4 text-[#26a69a]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
          <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/>
        </svg>
        <span class="text-xs font-bold" :class="syncAllSkipped ? 'text-[#2196f3]' : 'text-[#26a69a]'">
          {{ syncAllSkipped ? '数据已是最新' : '同步完成' }}
        </span>
        <template v-if="syncAllSkipped">
          <span class="text-[10px] text-[#9e9e9e]">
            {{ lastSyncStockCount }} 只股票，最近同步 {{ lastSyncLatestDate }}
          </span>
        </template>
        <template v-else>
          <span class="text-[10px] text-[#9e9e9e]">成功 {{ lastSyncSuccess }}，失败 {{ lastSyncFailed }}，耗时 {{ lastSyncElapsed }}</span>
        </template>
      </div>
      <button @click="showSyncResult = false" class="text-[10px] text-[#666] hover:text-white transition">✕</button>
    </div>

    <!-- ═══════════════════════════════════════════════════════════
         主操作区（始终显示，同步中禁用按钮）
         ═════════════════════════════════════════════════════════ -->
    <div class="flex-1 overflow-y-auto">
      <div class="max-w-xl mx-auto px-5 py-5 space-y-4">

        <!-- ──── 覆盖率顶栏 ──── -->
        <div class="flex items-center gap-4">
          <div class="flex items-baseline gap-1.5">
            <span class="text-3xl font-black" :style="{ color: progressColor(allASyncPercent) }">{{ allASyncPercent }}%</span>
            <span class="text-[11px] text-[#666]">覆盖</span>
          </div>
          <div class="flex-1">
            <div class="w-full h-2 bg-[#0f3460] rounded-full overflow-hidden">
              <div class="h-full rounded-full transition-all duration-700"
                :style="{ width: `${allASyncPercent}%`, backgroundColor: progressColor(allASyncPercent) }"></div>
            </div>
          </div>
          <span class="text-xs text-[#999] font-mono">{{ allALocalCount }}/{{ allAOnlineTotal ?? '—' }}</span>
          <div v-if="loadingOnlineInfo" class="w-3 h-3 border border-[#555] border-t-transparent rounded-full animate-spin"></div>
        </div>

        <!-- ──── 各板块迷你进度 ──── -->
        <div class="flex gap-1.5">
          <div v-for="bd in BOARDS" :key="bd.id" class="flex-1 bg-[#16213e] rounded-lg p-2 text-center space-y-1">
            <div class="text-[10px] font-bold" :style="{ color: bd.color }">{{ bd.icon }}</div>
            <div class="w-full h-1.5 bg-[#0f3460] rounded-full overflow-hidden">
              <div class="h-full rounded-full transition-all duration-500"
                :style="{ width: `${boardPercent(bd.id)}%`, backgroundColor: bd.color }"></div>
            </div>
            <div class="text-[9px] text-[#aaa] font-mono">{{ boardLocalCount(bd.id) }}<span class="text-[#555]">/{{ boardOnlineTotal(bd.id) ?? '—' }}</span></div>
            <div v-if="boardLatestDate(bd.id)" class="text-[8px] text-[#666] font-mono mt-0.5">{{ boardLatestDate(bd.id) }}</div>
          </div>
        </div>

        <!-- ──── 一键同步 ──── -->
        <div class="bg-[#16213e] rounded-2xl p-5">
          <div class="flex items-center justify-between mb-2">
            <div>
              <div class="text-base font-bold text-white">同步 K 线数据</div>
              <div class="text-[11px] text-[#9e9e9e] mt-0.5">
                {{ selectedLevels.map(k => LEVELS.find(l => l.key === k)?.label).join(' · ') }}
                <span class="mx-1 text-[#444]">|</span>
                {{ selectedBoardLabel() }}
                <span class="text-[#666]">（{{ selectedBoardCount() }} 只）</span>
              </div>
            </div>
            <div class="flex items-center gap-1">
              <button @click="showClearConfirm = true"
                class="p-2 rounded-lg text-[#e94560]/60 hover:text-[#e94560] hover:bg-[#e94560]/10 transition"
                title="清空数据">
                <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                </svg>
              </button>
              <button @click="showAdvanced = !showAdvanced"
                class="p-2 rounded-lg text-[#666] hover:text-white hover:bg-[#0f3460]/50 transition"
                :class="showAdvanced ? 'text-white bg-[#0f3460]/50' : ''"
                title="高级设置">
                <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/>
                  <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                </svg>
              </button>
            </div>
          </div>

          <!-- 级别选择 — 始终显示 -->
          <div class="mb-3">
            <div class="text-[10px] text-[#9e9e9e] mb-1.5">同步级别</div>
            <div class="flex flex-wrap gap-1">
              <button v-for="lv in LEVELS" :key="lv.key" @click="toggleLevel(lv.key)"
                class="px-2.5 py-1 text-[10px] rounded-md transition-all"
                :class="selectedLevels.includes(lv.key)
                  ? 'bg-[#e94560]/20 text-[#e94560] ring-1 ring-[#e94560]/40'
                  : 'bg-[#0f3460]/50 text-[#555] hover:text-[#888]'">
                {{ lv.label }}
              </button>
            </div>
          </div>

          <!-- 高级设置（折叠） -->
          <div v-if="showAdvanced" class="mb-4 space-y-3 pb-4 border-b border-[#2a2a4a]/50">
            <!-- 板块 -->
            <div>
              <div class="text-[10px] text-[#9e9e9e] mb-1.5 uppercase tracking-wider">范围</div>
              <div class="flex flex-wrap gap-1.5">
                <button @click="selectedBoard = 'all_a'"
                  class="px-3 py-1 text-[10px] rounded-md font-medium transition-all"
                  :class="selectedBoard === 'all_a'
                    ? 'bg-[#e94560]/20 text-[#e94560] ring-1 ring-[#e94560]/40'
                    : 'bg-[#0f3460]/50 text-[#666] hover:text-[#999]'">
                  全 A 股
                </button>
                <button v-for="bd in BOARDS" :key="bd.id" @click="selectedBoard = bd.id"
                  class="px-3 py-1 text-[10px] rounded-md transition-all"
                  :class="selectedBoard === bd.id
                    ? 'ring-1'
                    : 'bg-[#0f3460]/50 hover:bg-[#0f3460]'"
                  :style="selectedBoard === bd.id
                    ? { backgroundColor: bd.color + '20', color: bd.color }
                    : { color: '#666' }">
                  {{ bd.name }}
                </button>
              </div>
            </div>

            <!-- 日期 & 覆盖 -->
            <div class="flex items-center gap-4">
              <label class="flex items-center gap-1.5 text-[10px] text-[#9e9e9e]">
                起始
                <input v-model="startDate" type="date"
                  class="bg-[#0f3460] text-white px-2 py-1 rounded-md text-[10px] border border-[#2a2a4a] outline-none focus:border-[#e94560]/50 w-28" />
              </label>
              <label class="flex items-center gap-1.5 text-[10px] text-[#9e9e9e] cursor-pointer select-none">
                <input v-model="forceSync" type="checkbox" class="accent-[#e94560] w-3 h-3" />
                强制覆盖
              </label>
            </div>
          </div>

          <!-- 主按钮 -->
          <button @click="startSync" :disabled="syncing || selectedLevels.length === 0"
            class="w-full py-3.5 rounded-xl text-sm font-bold transition-all disabled:opacity-30
              bg-gradient-to-r from-[#e94560] to-[#9c27b0] text-white
              hover:brightness-110 active:scale-[0.98] shadow-lg shadow-[#e94560]/20">
            {{ syncing ? '同步中...' : '开始同步' }}
          </button>

          <!-- 清空确认（全局） -->
          <div v-if="showClearConfirm" class="mt-3 bg-[#e94560]/5 border border-[#e94560]/20 rounded-lg p-3">
            <div class="text-[11px] text-[#e94560] font-bold mb-2">⚠ 确定要清空所有数据吗？此操作不可恢复！</div>
            <div class="flex gap-2">
              <button @click="doClearAllData" :disabled="clearingData"
                class="flex-1 py-1.5 text-[11px] rounded-lg bg-[#e94560] text-white font-bold hover:brightness-110 transition disabled:opacity-50">
                {{ clearingData ? '清空中...' : '确定清空' }}
              </button>
              <button @click="showClearConfirm = false"
                class="flex-1 py-1.5 text-[11px] rounded-lg bg-[#0f3460] text-[#9e9e9e] hover:text-white transition">
                取消
              </button>
            </div>
          </div>
        </div>

        <!-- ──── 底部标签页 ──── -->
        <div class="bg-[#16213e] rounded-2xl overflow-hidden">
          <!-- 标签 -->
          <div class="flex">
            <button v-for="tab in [
              { key: 'overview' as const, label: '数据概览' },
              { key: 'validate' as const, label: '校验' },
              { key: 'manage' as const, label: '目录' },
            ]" :key="tab.key" @click="activeTab = tab.key"
              class="flex-1 py-2.5 text-[11px] text-center transition-all border-b-2"
              :class="activeTab === tab.key
                ? 'text-white border-[#e94560]'
                : 'text-[#555] border-transparent hover:text-[#888]'">
              {{ tab.label }}
            </button>
          </div>

          <!-- 数据概览 -->
          <div v-if="activeTab === 'overview' && dataStatus" class="p-4">
            <div class="grid grid-cols-4 gap-2">
              <div v-for="lv in dataStatus.levels" :key="lv.dir_name"
                class="bg-[#0f3460]/50 rounded-lg p-2.5 text-center">
                <div class="text-[10px] text-[#9e9e9e]">{{ lv.level }}</div>
                <div class="text-lg font-bold" :class="lv.file_count > 0 ? 'text-[#26a69a]' : 'text-[#444]'">
                  {{ lv.file_count }}
                </div>
                <div v-if="lv.sample_start" class="text-[8px] text-[#444] font-mono mt-0.5">
                  {{ lv.sample_start.slice(0, 10) }}
                </div>
              </div>
            </div>
            <div v-if="dataStatus.levels.length === 0" class="text-center text-[#555] text-xs py-4">
              暂无数据，点击上方"开始同步"
            </div>
          </div>

          <!-- 校验 -->
          <div v-if="activeTab === 'validate'" class="p-4 space-y-3">
            <!-- 级别 -->
            <div class="flex flex-wrap gap-1">
              <button v-for="lv in LEVELS" :key="lv.key" @click="toggleValidateLevel(lv.key)"
                class="px-2 py-0.5 text-[10px] rounded-md transition-all"
                :class="validateLevels.includes(lv.key)
                  ? 'bg-[#ff9800]/20 text-[#ff9800] ring-1 ring-[#ff9800]/30'
                  : 'bg-[#0f3460]/50 text-[#555] hover:text-[#888]'">
                {{ lv.label }}
              </button>
            </div>

            <!-- 单只 -->
            <div class="flex gap-1.5">
              <input v-model="validateSymbol" type="text" placeholder="股票代码"
                class="flex-1 bg-[#0f3460] text-white px-3 py-2 rounded-lg text-xs border border-[#2a2a4a] outline-none focus:border-[#ff9800]/40 placeholder-[#555]"
                @keyup.enter="validateSingleStock" />
              <button @click="validateSingleStock" :disabled="validating"
                class="px-4 py-2 bg-[#ff9800] text-black text-xs font-bold rounded-lg hover:brightness-110 transition disabled:opacity-40">
                校验
              </button>
            </div>

            <div v-if="validateResults.length" class="space-y-2">
              <div v-for="vr in validateResults" :key="vr.symbol" class="bg-[#0f3460]/50 rounded-lg p-3">
                <div class="flex items-center justify-between text-xs mb-2">
                  <span class="text-white font-mono font-bold">{{ vr.symbol }}</span>
                  <span :style="{ color: scoreColor(vr.overall_score) }" class="font-bold">{{ (vr.overall_score * 100).toFixed(1) }}%</span>
                </div>
                <div v-for="lv in vr.levels" :key="lv.level" class="flex items-center gap-2 text-[11px] py-0.5">
                  <span class="text-[#9e9e9e] w-8">{{ lv.level }}</span>
                  <div class="flex-1 h-1 bg-[#0f3460] rounded-full overflow-hidden">
                    <div class="h-full rounded-full" :style="{ width: `${lv.score * 100}%`, backgroundColor: scoreColor(lv.score) }"></div>
                  </div>
                  <span :style="{ color: scoreColor(lv.score) }" class="font-bold w-10 text-right text-[10px]">{{ (lv.score * 100).toFixed(1) }}%</span>
                </div>
                <div v-if="vr.levels.some(l => l.issues.length > 0)" class="mt-2 pl-2 border-l-2 border-[#2a2a4a] space-y-0.5">
                  <template v-for="lv in vr.levels" :key="lv.level">
                    <div v-for="(issue, idx) in lv.issues.slice(0, 5)" :key="idx"
                      class="text-[10px]" :style="{ color: severityColor(issue.severity) }">[{{ lv.level }}] {{ issue.message }}</div>
                  </template>
                </div>
              </div>
            </div>

            <!-- 跨源 -->
            <div class="pt-3 border-t border-[#2a2a4a]/30 space-y-2">
              <div class="text-[10px] text-[#9e9e9e]">跨源校验（新浪 vs 腾讯）</div>
              <div class="flex gap-1.5">
                <input v-model="crossValidateSymbol" type="text" placeholder="股票代码"
                  class="flex-1 bg-[#0f3460] text-white px-3 py-2 rounded-lg text-xs border border-[#2a2a4a] outline-none focus:border-[#26a69a]/40 placeholder-[#555]"
                  @keyup.enter="doCrossValidate" />
                <select v-model="crossValidateLevel"
                  class="bg-[#0f3460] text-white px-2 py-2 rounded-lg text-xs border border-[#2a2a4a] outline-none">
                  <option value="d">日线</option><option value="w">周线</option><option value="m">月线</option>
                </select>
                <button @click="doCrossValidate" :disabled="crossValidating"
                  class="px-4 py-2 bg-[#26a69a] text-black text-xs font-bold rounded-lg hover:brightness-110 transition disabled:opacity-40">
                  校验
                </button>
              </div>
              <div v-if="crossValidateResult" class="bg-[#0f3460]/50 rounded-lg p-3 text-[11px]">
                <div class="flex items-center justify-between mb-1">
                  <span class="text-[#9e9e9e]">{{ crossValidateResult.level }} · {{ crossValidateResult.total_rows }} 行</span>
                  <span :style="{ color: scoreColor(crossValidateResult.score) }" class="font-bold">{{ (crossValidateResult.score * 100).toFixed(1) }}%</span>
                </div>
                <div v-for="(issue, idx) in crossValidateResult.issues.slice(0, 10)" :key="idx"
                  class="py-0.5" :style="{ color: severityColor(issue.severity) }">{{ issue.message }}</div>
              </div>
            </div>

            <!-- 全量 -->
            <div class="pt-3 border-t border-[#2a2a4a]/30">
              <div class="flex items-center justify-between">
                <span class="text-[11px] text-[#9e9e9e]">全量校验</span>
                <button @click="fullValidate" :disabled="fullValidating"
                  class="px-3 py-1.5 text-[10px] rounded-lg bg-[#0f3460] text-[#ff9800] border border-[#ff9800]/20 hover:bg-[#1a4a7a] transition disabled:opacity-40">
                  {{ fullValidating ? '校验中...' : '开始' }}
                </button>
              </div>
              <div v-if="fullValidating" class="mt-2">
                <div class="w-full h-1.5 bg-[#0f3460] rounded-full overflow-hidden">
                  <div class="h-full rounded-full bg-[#ff9800] transition-all duration-300"
                    :style="{ width: `${fullValidateTotal > 0 ? (fullValidateCompleted / fullValidateTotal * 100) : 0}%` }"></div>
                </div>
                <div class="text-[10px] text-[#666] mt-1">{{ fullValidateCompleted }} / {{ fullValidateTotal }}</div>
              </div>
              <div v-if="!fullValidating && fullValidateCompleted > 0" class="mt-1 text-[11px]"
                :class="fullValidateResults.length > 0 ? 'text-[#ff9800]' : 'text-[#26a69a]'">
                {{ fullValidateResults.length > 0 ? `⚠ ${fullValidateResults.length} 只 < 80%` : `✅ 全部通过` }}
              </div>
              <div v-if="fullValidateResults.length > 0" class="mt-1 max-h-28 overflow-y-auto space-y-0.5">
                <div v-for="r in fullValidateResults.slice(0, 50)" :key="r.symbol"
                  class="flex items-center justify-between text-[11px] bg-[#0f3460]/40 rounded px-2 py-0.5">
                  <span class="text-white font-mono">{{ r.symbol }}</span>
                  <span :style="{ color: scoreColor(r.overall_score) }" class="font-mono">{{ (r.overall_score * 100).toFixed(1) }}%</span>
                </div>
              </div>
            </div>
          </div>

          <!-- 目录 -->
          <div v-if="activeTab === 'manage'" class="p-4 space-y-4">
            <!-- 数据目录 -->
            <div>
              <div class="text-[10px] text-[#9e9e9e] mb-1.5 uppercase tracking-wider">数据目录</div>
              <div class="text-xs text-[#9e9e9e] font-mono break-all leading-relaxed bg-[#0f3460]/30 rounded-lg p-3">{{ currentDataDir }}</div>
              <div class="flex gap-2 mt-2">
                <button @click="openDataDir"
                  class="px-3 py-2 text-[11px] rounded-lg bg-[#0f3460] text-white border border-[#2a2a4a] hover:bg-[#1a4a7a] transition flex items-center gap-1.5">
                  <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/>
                  </svg>
                  打开
                </button>
                <button @click="selectFolder"
                  class="px-3 py-2 text-[11px] rounded-lg bg-[#0f3460] text-[#26a69a] border border-[#26a69a]/20 hover:bg-[#1a4a7a] transition">
                  切换
                </button>
                <button @click="moveDataToFolder" :disabled="movingData"
                  class="px-3 py-2 text-[11px] rounded-lg bg-[#0f3460] text-[#ff9800] border border-[#ff9800]/20 hover:bg-[#1a4a7a] transition disabled:opacity-50">
                  迁移
                </button>
              </div>
              <div v-if="moveResult" class="text-[11px] mt-1">
                <span class="text-[#26a69a]">✓ {{ moveResult.moved }} 个文件已迁移</span>
                <span v-if="moveResult.failed > 0" class="text-[#ff5722]">，{{ moveResult.failed }} 个失败</span>
              </div>
            </div>

            <!-- 数据保留配置 -->
            <div class="border-t border-[#2a2a4a]/30 pt-3">
              <div class="text-[10px] text-[#9e9e9e] mb-2 uppercase tracking-wider">数据保留期限（月）</div>
              <div class="grid grid-cols-4 gap-2">
                <div class="bg-[#0f3460]/50 rounded-lg p-2 text-center">
                  <div class="text-[10px] text-[#9e9e9e]">1分线</div>
                  <input v-model.number="retentionConfig.f1_months" type="number" min="0" max="120"
                    class="w-full text-center text-sm font-bold text-white bg-transparent outline-none mt-0.5" />
                </div>
                <div class="bg-[#0f3460]/50 rounded-lg p-2 text-center">
                  <div class="text-[10px] text-[#9e9e9e]">5分线</div>
                  <input v-model.number="retentionConfig.f5_months" type="number" min="0" max="120"
                    class="w-full text-center text-sm font-bold text-white bg-transparent outline-none mt-0.5" />
                </div>
                <div class="bg-[#0f3460]/50 rounded-lg p-2 text-center">
                  <div class="text-[10px] text-[#9e9e9e]">15分线</div>
                  <input v-model.number="retentionConfig.f15_months" type="number" min="0" max="120"
                    class="w-full text-center text-sm font-bold text-white bg-transparent outline-none mt-0.5" />
                </div>
                <div class="bg-[#0f3460]/50 rounded-lg p-2 text-center">
                  <div class="text-[10px] text-[#9e9e9e]">30分线</div>
                  <input v-model.number="retentionConfig.f30_months" type="number" min="0" max="120"
                    class="w-full text-center text-sm font-bold text-white bg-transparent outline-none mt-0.5" />
                </div>
              </div>
              <div class="text-[9px] text-[#666] mt-1">设为 0 表示不限制；日线及以上级别不自动清理</div>
              <button @click="doTrimOldData" :disabled="trimmingData"
                class="w-full mt-2 py-2 text-[11px] rounded-lg bg-[#0f3460] text-[#ff9800] border border-[#ff9800]/20 hover:bg-[#1a4a7a] transition disabled:opacity-50 flex items-center justify-center gap-1.5">
                <svg v-if="trimmingData" class="w-3.5 h-3.5 animate-spin" fill="none" viewBox="0 0 24 24">
                  <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                  <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                <svg v-else class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                </svg>
                {{ trimmingData ? '清理中...' : '清理过期数据' }}
              </button>
              <div v-if="trimResult" class="text-[11px] mt-1.5 space-y-0.5">
                <div class="text-[#26a69a]">✓ 裁剪 {{ trimResult.trimmed_files }} 个文件，删除 {{ trimResult.removed_files }} 个文件</div>
                <div class="text-[#9e9e9e]">数据行: {{ trimResult.rows_before }} → {{ trimResult.rows_after }}（减少 {{ trimResult.rows_before - trimResult.rows_after }}）</div>
              </div>
            </div>

            <!-- 清理退市股 -->
            <div class="border-t border-[#2a2a4a]/30 pt-3">
              <div class="text-[10px] text-[#9e9e9e] uppercase tracking-wider">清理退市股</div>
              <div class="text-[9px] text-[#666] mt-0.5">对比在线在市列表，删除已退市股票的所有 K 线数据</div>
              <button @click="doCleanDelisted" :disabled="cleaningDelisted"
                class="w-full mt-2 py-2 text-[11px] rounded-lg bg-[#0f3460] text-[#ef5350] border border-[#ef5350]/20 hover:bg-[#1a4a7a] transition disabled:opacity-50 flex items-center justify-center gap-1.5">
                <svg v-if="cleaningDelisted" class="w-3.5 h-3.5 animate-spin" fill="none" viewBox="0 0 24 24">
                  <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                  <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                <svg v-else class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636"/>
                </svg>
                {{ cleaningDelisted ? '清理中...' : '清理退市股' }}
              </button>
              <div v-if="delistedResult" class="text-[11px] mt-1.5 space-y-0.5">
                <div v-if="delistedResult.codes.length > 0" class="text-[#26a69a]">
                  ✓ 清理 {{ delistedResult.codes.length }} 只退市股，删除 {{ delistedResult.files }} 个文件
                </div>
                <div v-else class="text-[#9e9e9e]">✓ 无退市股需要清理</div>
                <div v-if="delistedResult.codes.length > 0" class="text-[#9e9e9e] truncate" :title="delistedResult.codes.join(', ')">
                  {{ delistedResult.codes.slice(0, 10).join(', ') }}{{ delistedResult.codes.length > 10 ? '...' : '' }}
                </div>
              </div>
            </div>

            <!-- 清空数据 -->
            <div class="border-t border-[#2a2a4a]/30 pt-3">
              <div class="flex items-center justify-between">
                <div>
                  <div class="text-[10px] text-[#9e9e9e] uppercase tracking-wider">清空数据</div>
                  <div class="text-[9px] text-[#666] mt-0.5">删除所有 K 线数据文件，不可恢复</div>
                </div>
                <button @click="showClearConfirm = true"
                  class="px-3 py-1.5 text-[10px] rounded-lg bg-[#e94560]/10 text-[#e94560] border border-[#e94560]/20 hover:bg-[#e94560]/20 transition">
                  清空全部
                </button>
              </div>
              <!-- 确认对话框 -->
              <div v-if="showClearConfirm" class="mt-2 bg-[#e94560]/5 border border-[#e94560]/20 rounded-lg p-3">
                <div class="text-[11px] text-[#e94560] font-bold mb-2">⚠ 确定要清空所有数据吗？此操作不可恢复！</div>
                <div class="flex gap-2">
                  <button @click="doClearAllData" :disabled="clearingData"
                    class="flex-1 py-1.5 text-[11px] rounded-lg bg-[#e94560] text-white font-bold hover:brightness-110 transition disabled:opacity-50">
                    {{ clearingData ? '清空中...' : '确定清空' }}
                  </button>
                  <button @click="showClearConfirm = false"
                    class="flex-1 py-1.5 text-[11px] rounded-lg bg-[#0f3460] text-[#9e9e9e] hover:text-white transition">
                    取消
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>

      </div>
    </div>

    <!-- 错误条 -->
    <div v-if="error"
      class="shrink-0 px-5 py-2.5 bg-[#ff5722]/10 border-t border-[#ff5722]/30 text-xs text-[#ff5722] flex items-center justify-between">
      <span class="truncate">{{ error }}</span>
      <button @click="error = ''" class="text-[#ff5722]/60 hover:text-[#ff5722] ml-4 shrink-0">✕</button>
    </div>
  </div>
</template>
