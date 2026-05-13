<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

// ===== 类型 =====
interface LevelStats {
  level: string;
  dir_name: string;
  file_count: number;
  sample_symbol: string | null;
  sample_count: number | null;
  sample_start: string | null;
  sample_end: string | null;
}

interface BoardStats {
  id: string;
  name: string;
  count: number;
}

interface DataStatus {
  data_dir: string;
  total_stocks: number;
  levels: LevelStats[];
  boards: BoardStats[];
}

interface SyncLevelResult {
  level: string;
  status: string;
  count: number;
  source: string;
  msg: string;
}

interface SyncStockResult {
  symbol: string;
  levels: SyncLevelResult[];
}

interface ValidationIssue {
  severity: string;
  category: string;
  row_index: number | null;
  datetime: string | null;
  message: string;
}

interface ValidateLevelResult {
  level: string;
  total_rows: number;
  issues: ValidationIssue[];
  score: number;
}

interface ValidateStockResult {
  symbol: string;
  levels: ValidateLevelResult[];
  overall_score: number;
}

interface MoveDataResult {
  moved: number;
  failed: number;
  errors: string[];
}
// ===== 状态 =====
const dataStatus = ref<DataStatus | null>(null);
const loading = ref(false);
const syncing = ref(false);
const validating = ref(false);
const syncProgress = ref("");
const syncResults = ref<SyncStockResult[]>([]);
const validateResults = ref<ValidateStockResult[]>([]);
const stockCodes = ref<string[]>([]);
const syncSymbol = ref("");
const selectedLevels = ref<string[]>(["m", "w", "d"]);
const startDate = ref("2023-01-01");
const forceSync = ref(false);
const error = ref("");
const activeTab = ref<"sync" | "validate">("sync");

// 同步进度详情
const syncTotal = ref(0);
const syncCompleted = ref(0);
const syncFailedDetails = ref<{ symbol: string; level: string; msg: string }[]>([]);

// 校验相关
const validateSymbol = ref("");
const validateLevels = ref<string[]>(["d", "w", "m"]);
const crossValidateSymbol = ref("");
const crossValidateLevel = ref("d");
const crossValidateResult = ref<ValidateLevelResult | null>(null);
const crossValidating = ref(false);

// 数据目录相关
const currentDataDir = ref("");
const moveResult = ref<MoveDataResult | null>(null);
const movingData = ref(false);

// 全量验证
const fullValidating = ref(false);
const fullValidateProgress = ref("");
const fullValidateResults = ref<ValidateStockResult[]>([]);
const fullValidateCompleted = ref(0);
const fullValidateTotal = ref(0);

// 按板块同步
const syncingBoard = ref<string | null>(null);
const boardSyncProgress = ref("");

// ===== 板块定义 =====
const boardDefs = [
  { id: "sh_main", name: "上证主板", color: "#e94560", icon: "🔴" },
  { id: "sz_main", name: "深证主板", color: "#2196f3", icon: "🔵" },
  { id: "gem", name: "创业板", color: "#ff9800", icon: "🟠" },
  { id: "star", name: "科创板", color: "#9c27b0", icon: "🟣" },
];

// ===== 级别选项 =====
const levelOptions = [
  { key: "m", label: "月线", dir: "1mo" },
  { key: "w", label: "周线", dir: "1wk" },
  { key: "d", label: "日线", dir: "1d" },
  { key: "f60", label: "60F", dir: "1h" },
  { key: "f30", label: "30F", dir: "30m" },
  { key: "f15", label: "15F", dir: "15m" },
  { key: "f5", label: "5F", dir: "5m" },
  { key: "f1", label: "1F", dir: "1m" },
];

const validateLevelOptions = [
  { key: "d", label: "日线" },
  { key: "w", label: "周线" },
  { key: "m", label: "月线" },
  { key: "f60", label: "60F" },
  { key: "f30", label: "30F" },
  { key: "f15", label: "15F" },
  { key: "f5", label: "5F" },
  { key: "f1", label: "1F" },
];

function toggleLevel(key: string) {
  const idx = selectedLevels.value.indexOf(key);
  if (idx >= 0) {
    selectedLevels.value.splice(idx, 1);
  } else {
    selectedLevels.value.push(key);
  }
}

function toggleValidateLevel(key: string) {
  const idx = validateLevels.value.indexOf(key);
  if (idx >= 0) {
    validateLevels.value.splice(idx, 1);
  } else {
    validateLevels.value.push(key);
  }
}

// ===== 加载数据状态 =====
async function refreshStatus() {
  loading.value = true;
  error.value = "";
  try {
    dataStatus.value = await invoke<DataStatus>("get_data_status");
    currentDataDir.value = dataStatus.value.data_dir;
  } catch (e: any) {
    error.value = e.toString();
  } finally {
    loading.value = false;
  }
}

// ===== 获取股票列表 =====
async function loadStockCodes() {
  try {
    stockCodes.value = await invoke<string[]>("get_all_stock_codes");
  } catch (e: any) {
    console.error("获取股票列表失败:", e);
  }
}

// 收集失败详情
function collectFailures(results: SyncStockResult[]) {
  for (const r of results) {
    for (const lv of r.levels) {
      if (lv.status !== "ok") {
        syncFailedDetails.value.push({
          symbol: r.symbol,
          level: lv.level,
          msg: lv.msg || lv.status,
        });
      }
    }
  }
}

// ===== 同步单只股票 =====
async function syncSingleStock() {
  if (!syncSymbol.value.trim()) return;
  syncing.value = true;
  syncProgress.value = `正在同步 ${syncSymbol.value} ...`;
  syncResults.value = [];
  syncTotal.value = 1;
  syncCompleted.value = 0;
  syncFailedDetails.value = [];
  error.value = "";

  try {
    const result = await invoke<SyncStockResult>("sync_stock", {
      symbol: syncSymbol.value.trim(),
      levels: selectedLevels.value,
      startDate: startDate.value || null,
      force: forceSync.value,
    });
    syncResults.value = [result];
    syncCompleted.value = 1;
    collectFailures([result]);
    syncProgress.value = `✅ ${syncSymbol.value} 同步完成`;
  } catch (e: any) {
    error.value = e.toString();
    syncProgress.value = `❌ 同步失败: ${e}`;
    syncFailedDetails.value.push({
      symbol: syncSymbol.value.trim(),
      level: "-",
      msg: e.toString(),
    });
  } finally {
    syncing.value = false;
    refreshStatus();
  }
}

// ===== 批量同步 =====
async function syncBatch() {
  if (stockCodes.value.length === 0) {
    await loadStockCodes();
  }

  const codes = stockCodes.value;
  if (codes.length === 0) {
    error.value = "没有可同步的股票代码";
    return;
  }

  syncing.value = true;
  syncResults.value = [];
  syncTotal.value = codes.length;
  syncCompleted.value = 0;
  syncFailedDetails.value = [];
  error.value = "";

  const batchSize = 5;

  for (let i = 0; i < codes.length; i += batchSize) {
    const batch = codes.slice(i, i + batchSize);
    syncProgress.value = `正在同步 ${syncCompleted.value + 1}-${Math.min(syncCompleted.value + batchSize, codes.length)} / ${codes.length}`;

    try {
      const results = await invoke<SyncStockResult[]>("sync_stocks_batch", {
        symbols: batch,
        levels: selectedLevels.value,
        startDate: startDate.value || null,
        force: forceSync.value,
      });
      syncResults.value.push(...results);
      collectFailures(results);
    } catch (e: any) {
      error.value = e.toString();
    }

    syncCompleted.value = Math.min(syncCompleted.value + batchSize, codes.length);

    if (i + batchSize < codes.length) {
      await new Promise((r) => setTimeout(r, 200));
    }
  }

  const okCount = syncResults.value.filter(r => r.levels.every(l => l.status === "ok")).length;
  const failCount = syncResults.value.length - okCount;
  syncProgress.value = failCount > 0
    ? `⚠️ 批量同步完成：${okCount} 成功，${failCount} 失败，共 ${codes.length} 只`
    : `✅ 批量同步完成，共 ${codes.length} 只股票`;
  syncing.value = false;
  refreshStatus();
}

// ===== 按板块同步 =====
async function syncByBoard(boardId: string) {
  const boardDef = boardDefs.find(b => b.id === boardId);
  const boardName = boardDef ? boardDef.name : (boardId === "all_a" ? "全 A 股" : boardId);

  syncingBoard.value = boardId;
  boardSyncProgress.value = `正在获取 ${boardName} 股票列表...`;
  error.value = "";

  let codes: string[];
  try {
    codes = await invoke<string[]>("get_stock_codes_by_board", { board: boardId });
  } catch (e: any) {
    error.value = `获取 ${boardName} 股票列表失败: ${e}`;
    syncingBoard.value = null;
    return;
  }

  if (codes.length === 0) {
    error.value = `${boardName} 没有可同步的股票`;
    syncingBoard.value = null;
    return;
  }

  syncing.value = true;
  syncResults.value = [];
  syncTotal.value = codes.length;
  syncCompleted.value = 0;
  syncFailedDetails.value = [];

  const batchSize = 5;

  for (let i = 0; i < codes.length; i += batchSize) {
    const batch = codes.slice(i, i + batchSize);
    boardSyncProgress.value = `[${boardName}] 同步 ${syncCompleted.value + 1}-${Math.min(syncCompleted.value + batchSize, codes.length)} / ${codes.length}`;
    syncProgress.value = boardSyncProgress.value;

    try {
      const results = await invoke<SyncStockResult[]>("sync_stocks_batch", {
        symbols: batch,
        levels: selectedLevels.value,
        startDate: startDate.value || null,
        force: forceSync.value,
      });
      syncResults.value.push(...results);
      collectFailures(results);
    } catch (e: any) {
      error.value = e.toString();
    }

    syncCompleted.value = Math.min(syncCompleted.value + batchSize, codes.length);

    if (i + batchSize < codes.length) {
      await new Promise((r) => setTimeout(r, 200));
    }
  }

  const okCount = syncResults.value.filter(r => r.levels.every(l => l.status === "ok")).length;
  const failCount = syncResults.value.length - okCount;
  boardSyncProgress.value = failCount > 0
    ? `⚠️ ${boardName} 同步完成：${okCount} 成功，${failCount} 失败，共 ${codes.length} 只`
    : `✅ ${boardName} 同步完成，共 ${codes.length} 只股票`;
  syncProgress.value = boardSyncProgress.value;
  syncingBoard.value = null;
  syncing.value = false;
  refreshStatus();
}

// ===== 校验单只股票 =====
async function validateSingleStock() {
  if (!validateSymbol.value.trim()) return;
  validating.value = true;
  error.value = "";

  try {
    const result = await invoke<ValidateStockResult>("validate_stock", {
      symbol: validateSymbol.value.trim(),
      levels: validateLevels.value,
    });
    validateResults.value = [result];
  } catch (e: any) {
    error.value = e.toString();
  } finally {
    validating.value = false;
  }
}

// ===== 跨源交叉校验 =====
async function doCrossValidate() {
  if (!crossValidateSymbol.value.trim()) return;
  crossValidating.value = true;
  crossValidateResult.value = null;
  error.value = "";

  try {
    const result = await invoke<ValidateLevelResult>("cross_validate_stock", {
      symbol: crossValidateSymbol.value.trim(),
      level: crossValidateLevel.value,
    });
    crossValidateResult.value = result;
  } catch (e: any) {
    error.value = e.toString();
  } finally {
    crossValidating.value = false;
  }
}

// ===== 数据目录管理 =====
async function selectFolder() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择数据存储目录",
    });
    if (selected) {
      const dir = typeof selected === "string" ? selected : selected;
      await invoke<string>("set_data_dir", { path: dir });
      await refreshStatus();
    }
  } catch (e: any) {
    error.value = `选择目录失败: ${e}`;
  }
}

async function moveDataToFolder() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择数据迁移目标目录",
    });
    if (selected) {
      const dir = typeof selected === "string" ? selected : selected;
      movingData.value = true;
      moveResult.value = null;
      error.value = "";

      const result = await invoke<MoveDataResult>("move_data_dir", { newPath: dir });
      moveResult.value = result;
      await refreshStatus();
    }
  } catch (e: any) {
    error.value = `数据迁移失败: ${e}`;
  } finally {
    movingData.value = false;
  }
}

// ===== 全量验证 =====
async function fullValidate() {
  if (stockCodes.value.length === 0) {
    await loadStockCodes();
  }
  const codes = stockCodes.value;
  if (codes.length === 0) {
    error.value = "没有可验证的股票代码";
    return;
  }

  fullValidating.value = true;
  fullValidateResults.value = [];
  fullValidateCompleted.value = 0;
  fullValidateTotal.value = codes.length;
  fullValidateProgress.value = "开始全量验证...";
  error.value = "";

  const levels = ["d", "w", "m"]; // 全量验证核心级别
  const batchSize = 5;

  for (let i = 0; i < codes.length; i += batchSize) {
    const batch = codes.slice(i, i + batchSize);
    fullValidateProgress.value = `验证中 ${fullValidateCompleted.value + 1}-${Math.min(fullValidateCompleted.value + batchSize, codes.length)} / ${codes.length}`;

    for (const sym of batch) {
      try {
        const result = await invoke<ValidateStockResult>("validate_stock", {
          symbol: sym,
          levels,
        });
        // 只收录有问题或低分的
        if (result.overall_score < 0.9 || result.levels.some(l => l.issues.length > 0)) {
          fullValidateResults.value.push(result);
        }
      } catch (_e) {
        // 忽略单只股票验证错误
      }
      fullValidateCompleted.value++;
    }

    // 每 20 只放一小段间隔
    if (i + batchSize < codes.length) {
      await new Promise(r => setTimeout(r, 50));
    }
  }

  const issueCount = fullValidateResults.value.length;
  fullValidateProgress.value = issueCount > 0
    ? `⚠️ 全量验证完成：${issueCount} 只股票存在问题（共 ${codes.length} 只）`
    : `✅ 全量验证完成：${codes.length} 只股票数据质量全部达标`;
  fullValidating.value = false;
}

// ===== 辅助 =====
function statusColor(status: string) {
  switch (status) {
    case "ok": return "text-[#26a69a]";
    case "skip": return "text-[#9e9e9e]";
    case "fail": return "text-[#ff9800]";
    case "error": return "text-[#ff5722]";
    default: return "text-[#9e9e9e]";
  }
}

function statusLabel(status: string) {
  switch (status) {
    case "ok": return "✓";
    case "skip": return "−";
    case "fail": return "✗";
    case "error": return "!";
    default: return "?";
  }
}

function severityColor(severity: string) {
  switch (severity) {
    case "error": return "text-[#ff5722]";
    case "warning": return "text-[#ff9800]";
    case "info": return "text-[#2196f3]";
    default: return "text-[#9e9e9e]";
  }
}

function severityIcon(severity: string) {
  switch (severity) {
    case "error": return "✗";
    case "warning": return "⚠";
    case "info": return "ℹ";
    default: return "?";
  }
}

function scoreColor(score: number) {
  if (score >= 0.9) return "text-[#26a69a]";
  if (score >= 0.7) return "text-[#ff9800]";
  return "text-[#ff5722]";
}

function sourceLabel(source: string) {
  if (!source || source === "none") return "无";
  if (source === "sina") return "新浪";
  if (source === "tencent") return "腾讯";
  if (source === "netease") return "网易";
  if (source === "eastmoney") return "东方财富";
  if (source === "tushare") return "Tushare";
  if (source.startsWith("sina_")) return `新浪(${source})`;
  if (source.startsWith("tencent_")) return `腾讯(${source})`;
  if (source.startsWith("netease_")) return `网易(${source})`;
  if (source === "resample_from_daily") return "日线重采样";
  if (source === "resample") return "重采样";
  return source;
}

const successCount = computed(() => syncResults.value.filter(r => r.levels.every(l => l.status === "ok")).length);
const failCount = computed(() => syncResults.value.filter(r => r.levels.some(l => l.status !== "ok")).length);

// 进度百分比
const syncPercent = computed(() => {
  if (syncTotal.value === 0) return 0;
  return Math.round((syncCompleted.value / syncTotal.value) * 100);
});

const fullValidatePercent = computed(() => {
  if (fullValidateTotal.value === 0) return 0;
  return Math.round((fullValidateCompleted.value / fullValidateTotal.value) * 100);
});

// 校验统计
const totalIssues = computed(() => {
  let count = 0;
  for (const r of validateResults.value) {
    for (const lv of r.levels) {
      count += lv.issues.length;
    }
  }
  return count;
});
const errorIssueCount = computed(() => {
  let count = 0;
  for (const r of validateResults.value) {
    for (const lv of r.levels) {
      count += lv.issues.filter(i => i.severity === "error").length;
    }
  }
  return count;
});
const warningIssueCount = computed(() => {
  let count = 0;
  for (const r of validateResults.value) {
    for (const lv of r.levels) {
      count += lv.issues.filter(i => i.severity === "warning").length;
    }
  }
  return count;
});

// 全量验证统计
const fullTotalErrors = computed(() => {
  let c = 0;
  for (const r of fullValidateResults.value) {
    for (const lv of r.levels) {
      c += lv.issues.filter(i => i.severity === "error").length;
    }
  }
  return c;
});
const fullTotalWarnings = computed(() => {
  let c = 0;
  for (const r of fullValidateResults.value) {
    for (const lv of r.levels) {
      c += lv.issues.filter(i => i.severity === "warning").length;
    }
  }
  return c;
});

// ===== 初始化 =====
onMounted(() => {
  refreshStatus();
  loadStockCodes();
});
</script>

<template>
  <div class="h-full overflow-y-auto p-4 space-y-4">
    <!-- 标题 -->
    <div class="flex items-center justify-between">
      <h2 class="text-lg font-bold text-[#e94560]">数据同步与校验</h2>
      <button
        @click="refreshStatus"
        :disabled="loading"
        class="px-3 py-1 text-xs bg-[#0f3460] text-[#9e9e9e] hover:text-white rounded transition-colors"
      >
        ↻ 刷新
      </button>
    </div>

    <!-- ═══════ 数据目录管理 ═══════ -->
    <div class="bg-[#16213e] rounded-lg p-3 space-y-2">
      <div class="text-xs text-[#e94560] font-bold">📁 数据目录</div>
      <div class="flex items-center gap-2">
        <span class="text-[10px] text-[#9e9e9e] shrink-0">当前目录</span>
        <span class="text-xs text-white font-mono flex-1 truncate" :title="currentDataDir">{{ currentDataDir }}</span>
      </div>
      <div class="flex items-center gap-2">
        <button
          @click="selectFolder"
          class="px-3 py-1 text-xs bg-[#0f3460] text-[#26a69a] rounded hover:bg-[#1a4a7a] transition-colors border border-[#26a69a]/30"
        >
          📂 切换目录
        </button>
        <button
          @click="moveDataToFolder"
          :disabled="movingData"
          class="px-3 py-1 text-xs bg-[#0f3460] text-[#ff9800] rounded hover:bg-[#1a4a7a] transition-colors border border-[#ff9800]/30 disabled:opacity-50"
        >
          🚚 迁移数据
        </button>
        <span v-if="movingData" class="text-[10px] text-[#ff9800] animate-pulse">迁移中...</span>
      </div>

      <!-- 迁移结果 -->
      <div v-if="moveResult" class="text-[10px] space-y-0.5">
        <span class="text-[#26a69a]">✓ 已移动 {{ moveResult.moved }} 个文件</span>
        <span v-if="moveResult.failed > 0" class="text-[#ff5722]">，{{ moveResult.failed }} 个失败</span>
        <div v-for="(err, idx) in moveResult.errors.slice(0, 5)" :key="idx" class="text-[#ff5722]/80 truncate">
          {{ err }}
        </div>
      </div>

      <div class="text-[10px] text-[#9e9e9e] leading-relaxed">
        默认数据目录：应用数据目录/com.moyan.yifang/data<br/>
        切换目录：仅改变后续读写路径，不移动已有数据<br/>
        迁移数据：将已有数据移动到新目录并切换
      </div>
    </div>

    <!-- 数据目录概况 -->
    <div v-if="dataStatus" class="bg-[#16213e] rounded-lg p-3 space-y-2">
      <div class="flex items-center justify-between text-xs">
        <span class="text-[#9e9e9e]">股票总数</span>
        <span class="text-white font-bold">{{ dataStatus.total_stocks }}</span>
      </div>

      <!-- 板块概况 -->
      <div class="mt-2 space-y-1">
        <div class="text-xs text-[#9e9e9e] font-bold mb-1">板块概况</div>
        <div class="grid grid-cols-2 gap-1.5">
          <div
            v-for="b in dataStatus.boards.filter(b => b.id !== 'all_a')"
            :key="b.id"
            class="flex items-center justify-between bg-[#0f3460]/60 rounded px-2 py-1"
          >
            <span class="text-xs" :style="{ color: boardDefs.find(d => d.id === b.id)?.color || '#9e9e9e' }">
              {{ boardDefs.find(d => d.id === b.id)?.icon || '' }} {{ b.name }}
            </span>
            <span class="text-xs font-mono text-white font-bold">{{ b.count }}</span>
          </div>
        </div>
      </div>

      <!-- 级别详情 -->
      <div class="mt-2 space-y-1">
        <div class="text-xs text-[#9e9e9e] font-bold mb-1">各级别数据概况</div>
        <table class="w-full text-xs">
          <thead>
            <tr class="text-[#9e9e9e]">
              <th class="text-left py-1">级别</th>
              <th class="text-right py-1">文件数</th>
              <th class="text-right py-1">样本时间范围</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="lv in dataStatus.levels"
              :key="lv.dir_name"
              class="border-t border-[#2a2a4a]/50"
            >
              <td class="py-1 text-white">{{ lv.level }}</td>
              <td class="py-1 text-right" :class="lv.file_count > 0 ? 'text-[#26a69a]' : 'text-[#9e9e9e]'">
                {{ lv.file_count }}
              </td>
              <td class="py-1 text-right text-[#9e9e9e] font-mono text-[10px]">
                <template v-if="lv.sample_start">
                  {{ lv.sample_start?.slice(0, 10) }} ~ {{ lv.sample_end?.slice(0, 10) }}
                </template>
                <template v-else>—</template>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- 加载状态 -->
    <div v-else-if="loading" class="text-center text-[#9e9e9e] py-8 animate-pulse">
      加载数据状态...
    </div>

    <!-- Tab 切换 -->
    <div class="flex items-center gap-1 bg-[#16213e] rounded-lg p-1">
      <button
        @click="activeTab = 'sync'"
        class="flex-1 py-1.5 text-xs rounded transition-all text-center"
        :class="activeTab === 'sync'
          ? 'bg-[#e94560] text-white'
          : 'text-[#9e9e9e] hover:text-white'"
      >
        📥 数据同步
      </button>
      <button
        @click="activeTab = 'validate'"
        class="flex-1 px-3 py-1.5 text-xs rounded transition-all"
        :class="activeTab === 'validate' ? 'bg-[#e94560] text-white' : 'text-[#9e9e9e] hover:text-white'"
      >
        🔍 数据校验
      </button>
    </div>

    <!-- ═══════ 同步面板 ═══════ -->
    <template v-if="activeTab === 'sync'">
      <div class="bg-[#16213e] rounded-lg p-3 space-y-3">
        <div class="text-xs text-[#9e9e9e] font-bold">同步设置</div>

        <!-- 同步级别选择 -->
        <div class="flex flex-wrap gap-1.5">
          <button
            v-for="opt in levelOptions"
            :key="opt.key"
            @click="toggleLevel(opt.key)"
            class="px-2 py-1 text-xs rounded transition-all"
            :class="selectedLevels.includes(opt.key)
              ? 'bg-[#e94560] text-white'
              : 'bg-[#0f3460] text-[#9e9e9e] hover:text-white'"
          >
            {{ opt.label }}
          </button>
        </div>

        <!-- 起始日期 & 强制覆盖 -->
        <div class="flex items-center gap-4 text-xs">
          <label class="flex items-center gap-2 text-[#9e9e9e]">
            起始日期
            <input
              v-model="startDate"
              type="date"
              class="bg-[#0f3460] text-white px-2 py-1 rounded text-xs outline-none"
            />
          </label>
          <label class="flex items-center gap-2 text-[#9e9e9e] cursor-pointer">
            <input v-model="forceSync" type="checkbox" class="accent-[#e94560]" />
            强制覆盖
          </label>
        </div>

        <!-- 数据源提示 -->
        <div class="text-[10px] text-[#9e9e9e] leading-relaxed bg-[#0f3460]/50 rounded p-2">
          多数据源协同：东方财富(分钟级最优) → 新浪 → 腾讯 → Tushare → 网易<br/>
          月线/周线：从日线重采样确保完整性
        </div>

        <!-- 单只股票同步 -->
        <div class="flex items-center gap-2">
          <input
            v-model="syncSymbol"
            type="text"
            placeholder="输入股票代码，如 000001"
            class="flex-1 bg-[#0f3460] text-white px-3 py-1.5 rounded text-sm outline-none placeholder-[#666]"
            @keyup.enter="syncSingleStock"
          />
          <button
            @click="syncSingleStock"
            :disabled="syncing || !syncSymbol.trim()"
            class="px-4 py-1.5 bg-[#e94560] text-white text-sm rounded hover:bg-[#d63851] transition-colors disabled:opacity-50"
          >
            同步
          </button>
        </div>

        <!-- 批量同步 -->
        <div class="flex items-center gap-2 pt-1 border-t border-[#2a2a4a]">
          <span class="text-xs text-[#9e9e9e]">
            批量同步全部 {{ stockCodes.length }} 只股票
          </span>
          <button
            @click="syncBatch"
            :disabled="syncing"
            class="px-4 py-1.5 bg-[#0f3460] text-[#e94560] text-sm rounded hover:bg-[#1a4a7a] transition-colors disabled:opacity-50 border border-[#e94560]/30"
          >
            批量同步
          </button>
        </div>

        <!-- 按板块同步 -->
        <div class="pt-1 border-t border-[#2a2a4a] space-y-2">
          <div class="text-xs text-[#9e9e9e] font-bold">按板块同步</div>
          <div class="grid grid-cols-2 gap-1.5">
            <button
              v-for="bd in boardDefs"
              :key="bd.id"
              @click="syncByBoard(bd.id)"
              :disabled="syncing"
              class="flex items-center justify-between px-2.5 py-1.5 text-xs rounded transition-colors disabled:opacity-50 border"
              :style="{ borderColor: bd.color + '40', color: bd.color }"
              :class="syncingBoard === bd.id ? 'bg-[#0f3460]' : 'bg-[#0f3460]/60 hover:bg-[#0f3460]'"
            >
              <span>{{ bd.icon }} {{ bd.name }}</span>
              <span v-if="syncingBoard === bd.id" class="animate-pulse text-[10px]">同步中...</span>
              <span v-else class="font-mono text-[10px] opacity-70">
                {{ dataStatus?.boards.find(b => b.id === bd.id)?.count ?? 0 }}
              </span>
            </button>
          </div>
          <!-- 全 A 股按钮 -->
          <button
            @click="syncByBoard('all_a')"
            :disabled="syncing"
            class="w-full py-2 text-sm font-bold rounded transition-colors disabled:opacity-50
              bg-gradient-to-r from-[#e94560] via-[#9c27b0] to-[#2196f3]
              text-white hover:brightness-110"
          >
            <span v-if="syncingBoard === 'all_a'" class="animate-pulse">🌐 全 A 股同步中...</span>
            <span v-else>🌐 同步全 A 股 ({{ dataStatus?.boards.find(b => b.id === 'all_a')?.count ?? stockCodes.length }} 只)</span>
          </button>
        </div>
      </div>

      <!-- 同步进度 -->
      <div v-if="syncProgress || syncing" class="bg-[#16213e] rounded-lg p-3 text-xs space-y-2">
        <div class="text-white">{{ syncProgress }}</div>

        <div v-if="syncing || syncCompleted > 0" class="space-y-1">
          <div class="w-full bg-[#0f3460] rounded-full h-2">
            <div
              class="h-2 rounded-full transition-all duration-300"
              :style="{ width: `${syncPercent}%` }"
              :class="syncPercent >= 100 ? 'bg-[#26a69a]' : 'bg-[#e94560]'"
            ></div>
          </div>
          <div class="flex justify-between text-[10px] text-[#9e9e9e]">
            <span>{{ syncCompleted }} / {{ syncTotal }}</span>
            <span>{{ syncPercent }}%</span>
          </div>
        </div>

        <div v-if="!syncing && syncResults.length > 0" class="flex gap-4 text-[10px]">
          <span class="text-[#26a69a]">✓ 成功: {{ successCount }}</span>
          <span v-if="failCount > 0" class="text-[#ff9800]">✗ 失败: {{ failCount }}</span>
          <span class="text-[#9e9e9e]">总计: {{ syncResults.length }}</span>
        </div>
      </div>

      <!-- 失败详情 -->
      <div v-if="syncFailedDetails.length > 0 && !syncing" class="bg-[#ff5722]/10 border border-[#ff5722]/30 rounded-lg p-3 space-y-1 max-h-48 overflow-y-auto">
        <div class="text-xs text-[#ff5722] font-bold mb-1">失败详情 ({{ syncFailedDetails.length }})</div>
        <div
          v-for="(f, idx) in syncFailedDetails"
          :key="idx"
          class="flex items-start gap-2 text-[10px] py-0.5"
        >
          <span class="text-[#ff5722]">✗</span>
          <span class="text-white font-mono shrink-0">{{ f.symbol }}</span>
          <span class="text-[#9e9e9e] shrink-0">{{ f.level }}</span>
          <span class="text-[#ff9800]/80">{{ f.msg }}</span>
        </div>
      </div>

      <!-- 同步结果列表 -->
      <div v-if="syncResults.length > 0" class="bg-[#16213e] rounded-lg p-3 space-y-1 max-h-60 overflow-y-auto">
        <div class="text-xs text-[#9e9e9e] font-bold mb-1">最近同步结果</div>
        <div
          v-for="r in syncResults.slice().reverse().slice(0, 50)"
          :key="r.symbol"
          class="flex items-center gap-2 text-xs py-0.5"
        >
          <span class="text-white font-mono w-16 shrink-0">{{ r.symbol }}</span>
          <span
            v-for="lv in r.levels"
            :key="lv.level"
            :class="statusColor(lv.status)"
            class="font-mono"
            :title="`${lv.msg || lv.status} (源: ${sourceLabel(lv.source)})`"
          >
            {{ lv.level }}:{{ statusLabel(lv.status) }}({{ lv.count }})
          </span>
        </div>
        <div v-if="syncResults.length > 50" class="text-[10px] text-[#9e9e9e]">
          ... 仅显示最近 50 条
        </div>
      </div>
    </template>

    <!-- ═══════ 校验面板 ═══════ -->
    <template v-if="activeTab === 'validate'">
      <!-- 数据校验 -->
      <div class="bg-[#16213e] rounded-lg p-3 space-y-3">
        <div class="text-xs text-[#9e9e9e] font-bold">数据完整性校验</div>
        <div class="text-[10px] text-[#9e9e9e] leading-relaxed bg-[#0f3460]/50 rounded p-2">
          校验项目：<b>OHLC 逻辑</b>(High≥max, Low≤min) · <b>零值异常</b>(停牌/零成交) · <b>连续性</b>(日期间隙/逆序/重复) · <b>点数合理性</b>(日K约250/年)
        </div>

        <!-- 级别选择 -->
        <div class="flex flex-wrap gap-1.5">
          <button
            v-for="opt in validateLevelOptions"
            :key="opt.key"
            @click="toggleValidateLevel(opt.key)"
            class="px-2 py-1 text-xs rounded transition-all"
            :class="validateLevels.includes(opt.key)
              ? 'bg-[#e94560] text-white'
              : 'bg-[#0f3460] text-[#9e9e9e] hover:text-white'"
          >
            {{ opt.label }}
          </button>
        </div>

        <!-- 单只股票校验 -->
        <div class="flex items-center gap-2">
          <input
            v-model="validateSymbol"
            type="text"
            placeholder="输入股票代码，如 000001"
            class="flex-1 bg-[#0f3460] text-white px-3 py-1.5 rounded text-sm outline-none placeholder-[#666]"
            @keyup.enter="validateSingleStock"
          />
          <button
            @click="validateSingleStock"
            :disabled="validating || !validateSymbol.trim()"
            class="px-4 py-1.5 bg-[#2196f3] text-white text-sm rounded hover:bg-[#1976d2] transition-colors disabled:opacity-50"
          >
            校验
          </button>
        </div>
      </div>

      <!-- 全量验证 -->
      <div class="bg-[#16213e] rounded-lg p-3 space-y-2 border border-[#9c27b0]/30">
        <div class="text-xs text-[#9c27b0] font-bold">🔬 全量数据验证</div>
        <div class="text-[10px] text-[#9e9e9e] leading-relaxed bg-[#0f3460]/50 rounded p-2">
          对当前数据目录中 <b>所有股票</b> 的 日线/周线/月线 进行完整性、准确性全面校验。<br/>
          验证时间取决于股票数量，约 5-10 分钟（5000+只）。
        </div>
        <div class="flex items-center gap-2">
          <button
            @click="fullValidate"
            :disabled="fullValidating || stockCodes.length === 0"
            class="px-4 py-1.5 bg-[#9c27b0] text-white text-sm rounded hover:bg-[#7b1fa2] transition-colors disabled:opacity-50"
          >
            {{ fullValidating ? '验证中...' : '开始全量验证' }}
          </button>
          <span class="text-[10px] text-[#9e9e9e]">共 {{ stockCodes.length }} 只</span>
        </div>

        <!-- 全量验证进度 -->
        <div v-if="fullValidating || fullValidateCompleted > 0" class="space-y-1">
          <div class="text-xs text-white">{{ fullValidateProgress }}</div>
          <div class="w-full bg-[#0f3460] rounded-full h-2">
            <div
              class="h-2 rounded-full transition-all duration-300"
              :style="{ width: `${fullValidatePercent}%` }"
              :class="fullValidatePercent >= 100 ? 'bg-[#26a69a]' : 'bg-[#9c27b0]'"
            ></div>
          </div>
          <div class="flex justify-between text-[10px] text-[#9e9e9e]">
            <span>{{ fullValidateCompleted }} / {{ fullValidateTotal }}</span>
            <span>{{ fullValidatePercent }}%</span>
          </div>
        </div>

        <!-- 全量验证统计 -->
        <div v-if="fullValidateResults.length > 0 && !fullValidating" class="space-y-1">
          <div class="flex gap-4 text-[10px]">
            <span class="text-[#ff5722]">✗ 错误: {{ fullTotalErrors }}</span>
            <span class="text-[#ff9800]">⚠ 警告: {{ fullTotalWarnings }}</span>
            <span class="text-[#9e9e9e]">问题股票: {{ fullValidateResults.length }}</span>
          </div>

          <!-- 问题股票列表 -->
          <div class="max-h-60 overflow-y-auto space-y-1">
            <div
              v-for="vr in fullValidateResults.slice(0, 100)"
              :key="vr.symbol"
              class="flex items-center gap-2 text-[10px] py-0.5 border-t border-[#2a2a4a]/30"
            >
              <span class="text-white font-mono w-16 shrink-0">{{ vr.symbol }}</span>
              <span class="font-mono" :class="scoreColor(vr.overall_score)">
                {{ (vr.overall_score * 100).toFixed(0) }}%
              </span>
              <span
                v-for="lv in vr.levels.filter(l => l.issues.length > 0)"
                :key="lv.level"
                class="text-[#9e9e9e]"
              >
                {{ lv.level }}:{{ lv.issues.length }}问题
              </span>
            </div>
            <div v-if="fullValidateResults.length > 100" class="text-[9px] text-[#9e9e9e]">
              ... 还有 {{ fullValidateResults.length - 100 }} 只
            </div>
          </div>
        </div>
      </div>

      <!-- 校验结果 -->
      <div v-if="validateResults.length > 0" class="space-y-3">
        <div
          v-for="vr in validateResults"
          :key="vr.symbol"
          class="bg-[#16213e] rounded-lg p-3 space-y-2"
        >
          <div class="flex items-center justify-between">
            <span class="text-sm font-bold text-white">{{ vr.symbol }}</span>
            <span class="text-xs font-mono" :class="scoreColor(vr.overall_score)">
              综合评分: {{ (vr.overall_score * 100).toFixed(0) }}%
            </span>
          </div>

          <!-- 各级别校验 -->
          <div v-for="lv in vr.levels" :key="lv.level" class="border-t border-[#2a2a4a]/50 pt-2">
            <div class="flex items-center justify-between text-xs mb-1">
              <span class="text-white font-bold">{{ lv.level }}</span>
              <div class="flex items-center gap-3">
                <span class="text-[#9e9e9e]">行数: {{ lv.total_rows.toLocaleString() }}</span>
                <span class="text-[#9e9e9e]">问题: {{ lv.issues.length }}</span>
                <span class="font-mono" :class="scoreColor(lv.score)">
                  {{ (lv.score * 100).toFixed(0) }}%
                </span>
              </div>
            </div>

            <!-- 评分条 -->
            <div class="w-full bg-[#0f3460] rounded-full h-1.5 mb-2">
              <div
                class="h-1.5 rounded-full transition-all"
                :style="{ width: `${lv.score * 100}%` }"
                :class="lv.score >= 0.9 ? 'bg-[#26a69a]' : lv.score >= 0.7 ? 'bg-[#ff9800]' : 'bg-[#ff5722]'"
              ></div>
            </div>

            <!-- Issue 列表 -->
            <div v-if="lv.issues.length > 0" class="space-y-0.5 max-h-32 overflow-y-auto">
              <div
                v-for="(issue, idx) in lv.issues.slice(0, 20)"
                :key="idx"
                class="flex items-start gap-2 text-[10px] py-0.5"
              >
                <span :class="severityColor(issue.severity)">{{ severityIcon(issue.severity) }}</span>
                <span class="text-[#9e9e9e] shrink-0">{{ issue.category }}</span>
                <span v-if="issue.datetime" class="text-[#666] font-mono shrink-0">{{ issue.datetime?.slice(0, 10) }}</span>
                <span class="text-white/80">{{ issue.message }}</span>
              </div>
              <div v-if="lv.issues.length > 20" class="text-[9px] text-[#9e9e9e]">
                ... 还有 {{ lv.issues.length - 20 }} 个问题
              </div>
            </div>
            <div v-else class="text-[10px] text-[#26a69a]">✓ 无问题</div>
          </div>

          <!-- 校验汇总 -->
          <div class="flex gap-4 text-[10px] border-t border-[#2a2a4a]/50 pt-1">
            <span :class="severityColor('error')">{{ errorIssueCount }} 个错误</span>
            <span :class="severityColor('warning')">{{ warningIssueCount }} 个警告</span>
            <span class="text-[#9e9e9e]">{{ totalIssues }} 个问题</span>
          </div>
        </div>
      </div>

      <!-- ═══════ 跨源交叉校验 ═══════ -->
      <div class="bg-[#16213e] rounded-lg p-3 space-y-3 border border-[#2196f3]/20">
        <div class="text-xs text-[#2196f3] font-bold">跨数据源交叉校验</div>
        <div class="text-[10px] text-[#9e9e9e] leading-relaxed bg-[#0f3460]/50 rounded p-2">
          同时从<span class="text-white">新浪</span>和<span class="text-white">腾讯</span>获取数据，对比相同日期的 OHLCV 值是否一致。允许 0.5% 以内的前复权差异。
        </div>

        <div class="flex items-center gap-2">
          <input
            v-model="crossValidateSymbol"
            type="text"
            placeholder="股票代码，如 000001"
            class="flex-1 bg-[#0f3460] text-white px-3 py-1.5 rounded text-sm outline-none placeholder-[#666]"
            @keyup.enter="doCrossValidate"
          />
          <select
            v-model="crossValidateLevel"
            class="bg-[#0f3460] text-white px-2 py-1.5 rounded text-sm outline-none"
          >
            <option v-for="opt in validateLevelOptions.filter(o => !['f1','f5','f15'].includes(o.key))"
              :key="opt.key" :value="opt.key">{{ opt.label }}</option>
          </select>
          <button
            @click="doCrossValidate"
            :disabled="crossValidating || !crossValidateSymbol.trim()"
            class="px-4 py-1.5 bg-[#2196f3] text-white text-sm rounded hover:bg-[#1976d2] transition-colors disabled:opacity-50"
          >
            交叉校验
          </button>
        </div>

        <!-- 交叉校验结果 -->
        <div v-if="crossValidateResult" class="space-y-2">
          <div class="flex items-center justify-between text-xs">
            <span class="text-white">{{ crossValidateResult.level }}</span>
            <div class="flex items-center gap-3">
              <span class="text-[#9e9e9e]">行数: {{ crossValidateResult.total_rows }}</span>
              <span class="font-mono" :class="scoreColor(crossValidateResult.score)">
                匹配度: {{ (crossValidateResult.score * 100).toFixed(0) }}%
              </span>
            </div>
          </div>

          <div class="w-full bg-[#0f3460] rounded-full h-1.5">
            <div
              class="h-1.5 rounded-full transition-all"
              :style="{ width: `${crossValidateResult.score * 100}%` }"
              :class="crossValidateResult.score >= 0.9 ? 'bg-[#26a69a]' : crossValidateResult.score >= 0.7 ? 'bg-[#ff9800]' : 'bg-[#ff5722]'"
            ></div>
          </div>

          <div v-if="crossValidateResult.issues.length > 0" class="space-y-0.5 max-h-40 overflow-y-auto">
            <div
              v-for="(issue, idx) in crossValidateResult.issues"
              :key="idx"
              class="flex items-start gap-2 text-[10px] py-0.5"
            >
              <span :class="severityColor(issue.severity)">{{ severityIcon(issue.severity) }}</span>
              <span v-if="issue.datetime" class="text-[#666] font-mono shrink-0">{{ issue.datetime?.slice(0, 10) }}</span>
              <span class="text-white/80">{{ issue.message }}</span>
            </div>
          </div>
          <div v-else class="text-[10px] text-[#26a69a]">✓ 两个数据源数据完全一致</div>
        </div>
      </div>
    </template>

    <!-- 错误 -->
    <div v-if="error" class="bg-[#ff5722]/10 border border-[#ff5722]/30 rounded-lg p-3 text-xs text-[#ff5722]">
      {{ error }}
    </div>
  </div>
</template>
