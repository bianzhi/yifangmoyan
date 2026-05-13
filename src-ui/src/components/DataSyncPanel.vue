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

interface BoardOnlineInfo {
  id: string;
  name: string;
  total_count: number;
  local_count: number;
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

// ===== 响应式数据 =====
const dataStatus = ref<DataStatus | null>(null);
const boardOnlineInfo = ref<BoardOnlineInfo[]>([]);
const loading = ref(false);
const syncing = ref(false);
const validating = ref(false);
const syncProgress = ref("");
const syncResults = ref<SyncStockResult[]>([]);
const validateResults = ref<ValidateStockResult[]>([]);
const error = ref("");
const activeTab = ref<"sync" | "validate">("sync");

// 同步进度详情
const syncTotal = ref(0);
const syncCompleted = ref(0);
const syncFailedDetails = ref<{ symbol: string; level: string; msg: string }[]>([]);

// 同步设置
const selectedLevels = ref<string[]>(["m", "w", "d"]);
const startDate = ref("2023-01-01");
const forceSync = ref(false);

// 当前同步的板块
const syncingBoard = ref<string | null>(null);

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

// ===== 板块定义 =====
const boardDefs = [
  { id: "sh_main", name: "上证主板", color: "#e94560", icon: "🔴" },
  { id: "sz_main", name: "深证主板", color: "#2196f3", icon: "🔵" },
  { id: "gem", name: "创业板", color: "#ff9800", icon: "🟠" },
  { id: "star", name: "科创板", color: "#9c27b0", icon: "🟣" },
  { id: "bse", name: "北交所", color: "#4caf50", icon: "🟢" },
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

// ===== 加载板块在线信息 =====
const loadingOnlineInfo = ref(false);
async function loadBoardOnlineInfo() {
  loadingOnlineInfo.value = true;
  try {
    boardOnlineInfo.value = await invoke<BoardOnlineInfo[]>("get_board_online_info");
  } catch (e: any) {
    console.error("获取板块在线信息失败:", e);
  } finally {
    loadingOnlineInfo.value = false;
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

// ===== 按板块同步（前控分批调用，实时更新进度）=====
async function syncByBoard(boardId: string) {
  const boardDef = boardDefs.find(b => b.id === boardId);
  const boardName = boardDef ? boardDef.name : (boardId === "all_a" ? "全 A 股" : boardId);

  syncingBoard.value = boardId;
  boardSyncProgress.value = `正在从东方财富获取 ${boardName} 股票列表...`;
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

  const batchSize = 20;

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
      await new Promise((r) => setTimeout(r, 100));
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
  loadBoardOnlineInfo();
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
      title: "选择新数据目录",
    });
    if (!selected) return;
    const dir = typeof selected === "string" ? selected : selected;
    movingData.value = true;
    moveResult.value = null;
    error.value = "";

    const result = await invoke<MoveDataResult>("move_data_dir", { newPath: dir });
    moveResult.value = result;
    await refreshStatus();
  } catch (e: any) {
    error.value = `迁移数据失败: ${e}`;
  } finally {
    movingData.value = false;
  }
}

// ===== 全量验证 =====
async function fullValidate() {
  fullValidating.value = true;
  fullValidateResults.value = [];
  fullValidateCompleted.value = 0;
  fullValidateProgress.value = "加载股票列表...";
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
    fullValidateProgress.value = `校验 ${fullValidateCompleted.value + 1}-${Math.min(fullValidateCompleted.value + batchSize, codes.length)} / ${codes.length}`;

    try {
      const results = await invoke<ValidateStockResult[]>("validate_stocks_batch", {
        symbols: batch,
        levels: validateLevels.value,
      });
      for (const r of results) {
        if (r.overall_score < 0.8) {
          fullValidateResults.value.push(r);
        }
      }
    } catch (e: any) {
      error.value = e.toString();
    }

    fullValidateCompleted.value = Math.min(fullValidateCompleted.value + batchSize, codes.length);

    if (i + batchSize < codes.length) {
      await new Promise((r) => setTimeout(r, 100));
    }
  }

  fullValidateProgress.value = fullValidateResults.value.length > 0
    ? `⚠️ 校验完成，${fullValidateResults.value.length} 只股票评分 < 0.8`
    : `✅ 全部 ${codes.length} 只股票数据校验通过`;
  fullValidating.value = false;
}

// ===== 辅助函数 =====
function statusColor(status: string) {
  switch (status) {
    case "ok": return "#26a69a";
    case "partial": return "#ff9800";
    case "error": return "#ff5722";
    default: return "#9e9e9e";
  }
}

function statusLabel(status: string) {
  switch (status) {
    case "ok": return "✓";
    case "partial": return "⚠";
    case "error": return "✗";
    default: return status;
  }
}

function severityColor(severity: string) {
  switch (severity) {
    case "error": return "#ff5722";
    case "warning": return "#ff9800";
    case "info": return "#2196f3";
    default: return "#9e9e9e";
  }
}

function severityIcon(severity: string) {
  switch (severity) {
    case "error": return "❌";
    case "warning": return "⚠️";
    case "info": return "ℹ️";
    default: return "•";
  }
}

function scoreColor(score: number) {
  if (score >= 0.9) return "#26a69a";
  if (score >= 0.7) return "#ff9800";
  return "#ff5722";
}

function sourceLabel(source: string) {
  switch (source) {
    case "sina": return "新浪";
    case "tencent": return "腾讯";
    case "eastmoney": return "东方财富";
    case "tushare": return "Tushare";
    case "netease": return "网易";
    case "merged": return "合并";
    case "resampled": return "重采样";
    default: return source;
  }
}

const syncPercent = computed(() =>
  syncTotal.value > 0 ? Math.round((syncCompleted.value / syncTotal.value) * 100) : 0
);

const successCount = computed(() =>
  syncResults.value.filter(r => r.levels.every(l => l.status === "ok")).length
);

const failCount = computed(() =>
  syncResults.value.length - successCount.value
);

const boardSyncProgress = ref("");

// 获取板块在线股票总数（优先在线值，回退到本地统计）
function getBoardOnlineTotal(boardId: string): number | null {
  const info = boardOnlineInfo.value.find(b => b.id === boardId);
  if (info && info.total_count > 0) return info.total_count;
  // 回退：用本地 dataStatus.boards 的 count 做兜底
  if (dataStatus.value) {
    const board = dataStatus.value.boards.find(b => b.id === boardId);
    if (board) return board.count;
  }
  return null;
}

// 获取板块本地已有股票数（优先在线信息里的 local_count，回退到本地统计）
function getBoardLocalCount(boardId: string): number | null {
  const info = boardOnlineInfo.value.find(b => b.id === boardId);
  if (info && info.local_count > 0) return info.local_count;
  if (dataStatus.value) {
    const board = dataStatus.value.boards.find(b => b.id === boardId);
    if (board) return board.count;
  }
  return null;
}

// ===== 初始化 =====
onMounted(async () => {
  // 先加载本地统计（瞬间完成），确保板块数量有兜底值
  await refreshStatus();
  // 再异步获取在线信息（需要网络请求，轻量级 pz=1）
  loadBoardOnlineInfo();
});
</script>

<template>
  <div class="h-full overflow-y-auto p-4 space-y-4">
    <!-- 标题 -->
    <div class="flex items-center justify-between">
      <h2 class="text-lg font-bold text-[#e94560]">数据同步与校验</h2>
      <button
        @click="refreshStatus(); loadBoardOnlineInfo()"
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
        <span class="text-[#9e9e9e]">本地股票总数</span>
        <span class="text-white font-bold">{{ dataStatus.total_stocks }}</span>
      </div>

      <!-- 板块概况（含在线信息） -->
      <div class="mt-2 space-y-1">
        <div class="text-xs text-[#9e9e9e] font-bold mb-1">
          板块概况（在线 / 本地）
          <span v-if="loadingOnlineInfo" class="animate-pulse text-[#e94560] ml-2">⏳ 更新中...</span>
        </div>
        <div class="grid grid-cols-2 gap-1.5">
          <div
            v-for="b in boardDefs"
            :key="b.id"
            class="flex items-center justify-between bg-[#0f3460]/60 rounded px-2 py-1"
          >
            <span class="text-xs" :style="{ color: b.color }">
              {{ b.icon }} {{ b.name }}
            </span>
            <span class="text-xs text-white font-mono">
              <span :class="getBoardOnlineTotal(b.id) !== null ? 'text-[#9e9e9e]' : 'text-[#9e9e9e]/50'">
                {{ getBoardOnlineTotal(b.id) ?? '—' }}
              </span>
              <span class="text-[#666]"> / </span>
              <span :class="getBoardLocalCount(b.id) ? 'text-[#26a69a]' : 'text-[#9e9e9e]'">
                {{ getBoardLocalCount(b.id) ?? '—' }}
              </span>
            </span>
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
          点击板块按钮自动从东方财富获取该板块全部股票并同步
        </div>

        <!-- 按板块同步 -->
        <div class="space-y-2">
          <div class="text-xs text-[#9e9e9e] font-bold">按板块同步</div>
          <div class="grid grid-cols-2 gap-1.5">
            <button
              v-for="bd in boardDefs"
              :key="bd.id"
              @click="syncByBoard(bd.id)"
              :disabled="syncing"
              class="flex items-center justify-between px-2.5 py-2 text-xs rounded transition-colors disabled:opacity-50 border"
              :style="{ borderColor: bd.color + '40', color: bd.color }"
              :class="syncingBoard === bd.id ? 'bg-[#0f3460]' : 'bg-[#0f3460]/60 hover:bg-[#0f3460]'"
            >
              <span>{{ bd.icon }} {{ bd.name }}</span>
              <span v-if="syncingBoard === bd.id" class="animate-pulse text-[10px]">同步中...</span>
              <span v-else class="font-mono text-[10px] opacity-70">
                {{ getBoardOnlineTotal(bd.id) ?? '—' }} 只
              </span>
            </button>
          </div>
          <!-- 全 A 股按钮 -->
          <button
            @click="syncByBoard('all_a')"
            :disabled="syncing"
            class="w-full py-2.5 text-sm font-bold rounded transition-colors disabled:opacity-50
              bg-gradient-to-r from-[#e94560] via-[#9c27b0] to-[#2196f3]
              text-white hover:brightness-110"
          >
            <span v-if="syncingBoard === 'all_a'" class="animate-pulse">🌐 全 A 股同步中...</span>
            <span v-else>🌐 同步全 A 股 ({{ getBoardOnlineTotal('all_a') ?? '—' }} 只)</span>
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

        <!-- 同步失败详情 -->
        <div v-if="syncFailedDetails.length > 0" class="mt-2 space-y-1 max-h-32 overflow-y-auto">
          <div v-for="(f, idx) in syncFailedDetails.slice(0, 50)" :key="idx" class="text-[10px] text-[#ff9800]/80">
            {{ f.symbol }} [{{ f.level }}]: {{ f.msg }}
          </div>
          <div v-if="syncFailedDetails.length > 50" class="text-[10px] text-[#9e9e9e]">
            ...还有 {{ syncFailedDetails.length - 50 }} 条失败记录
          </div>
        </div>
      </div>

      <!-- 最近同步结果详情 -->
      <div v-if="!syncing && syncResults.length > 0 && syncResults.length <= 20" class="bg-[#16213e] rounded-lg p-3 space-y-2">
        <div class="text-xs text-[#9e9e9e] font-bold">同步结果详情</div>
        <div
          v-for="r in syncResults"
          :key="r.symbol"
          class="border-t border-[#2a2a4a]/30 first:border-t-0 pt-1.5"
        >
          <div class="flex items-center gap-2 text-xs">
            <span class="text-white font-mono font-bold">{{ r.symbol }}</span>
            <span
              v-for="lv in r.levels"
              :key="lv.level"
              class="text-[10px] px-1 py-0.5 rounded"
              :style="{ backgroundColor: statusColor(lv.status) + '20', color: statusColor(lv.status) }"
            >
              {{ lv.level }} {{ statusLabel(lv.status) }} {{ lv.count }}条({{ sourceLabel(lv.source) }})
            </span>
          </div>
          <div v-for="lv in r.levels.filter(l => l.msg)" :key="lv.level + lv.msg" class="text-[10px] text-[#ff9800]/70 pl-4">
            {{ lv.level }}: {{ lv.msg }}
          </div>
        </div>
      </div>
    </template>

    <!-- ═══════ 校验面板 ═══════ -->
    <template v-if="activeTab === 'validate'">
      <!-- 单只股票校验 -->
      <div class="bg-[#16213e] rounded-lg p-3 space-y-3">
        <div class="text-xs text-[#9e9e9e] font-bold">校验设置</div>

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

        <!-- 单只校验 -->
        <div class="pt-2 border-t border-[#2a2a4a] space-y-2">
          <div class="text-xs text-[#9e9e9e]">单只股票 OHLC 校验</div>
          <div class="flex items-center gap-2">
            <input
              v-model="validateSymbol"
              type="text"
              placeholder="股票代码，如 000001"
              class="flex-1 bg-[#0f3460] text-white px-3 py-1.5 rounded text-sm outline-none placeholder-[#666]"
              @keyup.enter="validateSingleStock"
            />
            <button
              @click="validateSingleStock"
              :disabled="validating || !validateSymbol.trim()"
              class="px-4 py-1.5 bg-[#e94560] text-white text-sm rounded hover:bg-[#d63851] transition-colors disabled:opacity-50"
            >
              校验
            </button>
          </div>

          <!-- 校验结果 -->
          <div v-if="validateResults.length > 0" class="space-y-2 pt-2">
            <div v-for="vr in validateResults" :key="vr.symbol" class="space-y-1.5">
              <div class="flex items-center justify-between text-xs">
                <span class="text-white font-mono">{{ vr.symbol }}</span>
                <span :style="{ color: scoreColor(vr.overall_score) }" class="font-bold">
                  综合: {{ (vr.overall_score * 100).toFixed(1) }}%
                </span>
              </div>
              <div v-for="lv in vr.levels" :key="lv.level" class="bg-[#0f3460]/60 rounded p-2 space-y-1">
                <div class="flex items-center justify-between text-xs">
                  <span class="text-white">{{ lv.level }}</span>
                  <span class="text-[#9e9e9e]">{{ lv.total_rows }} 行</span>
                  <span :style="{ color: scoreColor(lv.score) }" class="font-bold">
                    {{ (lv.score * 100).toFixed(1) }}%
                  </span>
                </div>
                <div v-for="(issue, idx) in lv.issues.slice(0, 10)" :key="idx" class="text-[10px] flex gap-1 items-start">
                  <span>{{ severityIcon(issue.severity) }}</span>
                  <span :style="{ color: severityColor(issue.severity) }" class="flex-1">{{ issue.message }}</span>
                </div>
                <div v-if="lv.issues.length > 10" class="text-[10px] text-[#9e9e9e]">
                  ...还有 {{ lv.issues.length - 10 }} 条
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 跨源交叉校验 -->
        <div class="pt-2 border-t border-[#2a2a4a] space-y-2">
          <div class="text-xs text-[#9e9e9e]">跨数据源交叉校验（新浪 vs 腾讯）</div>
          <div class="flex items-center gap-2">
            <input
              v-model="crossValidateSymbol"
              type="text"
              placeholder="股票代码"
              class="flex-1 bg-[#0f3460] text-white px-3 py-1.5 rounded text-sm outline-none placeholder-[#666]"
              @keyup.enter="doCrossValidate"
            />
            <select
              v-model="crossValidateLevel"
              class="bg-[#0f3460] text-white px-2 py-1.5 rounded text-sm outline-none"
            >
              <option value="d">日线</option>
              <option value="w">周线</option>
              <option value="m">月线</option>
            </select>
            <button
              @click="doCrossValidate"
              :disabled="crossValidating"
              class="px-4 py-1.5 bg-[#0f3460] text-[#26a69a] text-sm rounded hover:bg-[#1a4a7a] transition-colors disabled:opacity-50 border border-[#26a69a]/30"
            >
              交叉校验
            </button>
          </div>

          <div v-if="crossValidateResult" class="bg-[#0f3460]/60 rounded p-2 space-y-1">
            <div class="flex items-center justify-between text-xs">
              <span class="text-white">{{ crossValidateResult.level }}</span>
              <span class="text-[#9e9e9e]">{{ crossValidateResult.total_rows }} 行</span>
              <span :style="{ color: scoreColor(crossValidateResult.score) }" class="font-bold">
                {{ (crossValidateResult.score * 100).toFixed(1) }}%
              </span>
            </div>
            <div v-for="(issue, idx) in crossValidateResult.issues" :key="idx" class="text-[10px] flex gap-1 items-start">
              <span>{{ severityIcon(issue.severity) }}</span>
              <span :style="{ color: severityColor(issue.severity) }" class="flex-1">{{ issue.message }}</span>
            </div>
          </div>
        </div>

        <!-- 全量校验 -->
        <div class="pt-2 border-t border-[#2a2a4a] space-y-2">
          <div class="text-xs text-[#9e9e9e]">全量校验（所有本地数据）</div>
          <button
            @click="fullValidate"
            :disabled="fullValidating"
            class="px-4 py-1.5 bg-[#0f3460] text-[#ff9800] text-sm rounded hover:bg-[#1a4a7a] transition-colors disabled:opacity-50 border border-[#ff9800]/30"
          >
            🔍 开始全量校验
          </button>

          <div v-if="fullValidateProgress" class="text-xs text-white">{{ fullValidateProgress }}</div>

          <div v-if="fullValidating" class="space-y-1">
            <div class="w-full bg-[#0f3460] rounded-full h-2">
              <div
                class="h-2 rounded-full bg-[#ff9800] transition-all duration-300"
                :style="{ width: `${fullValidateTotal > 0 ? (fullValidateCompleted / fullValidateTotal * 100) : 0}%` }"
              ></div>
            </div>
            <div class="flex justify-between text-[10px] text-[#9e9e9e]">
              <span>{{ fullValidateCompleted }} / {{ fullValidateTotal }}</span>
            </div>
          </div>

          <div v-if="fullValidateResults.length > 0" class="space-y-1 max-h-48 overflow-y-auto">
            <div
              v-for="r in fullValidateResults.slice(0, 50)"
              :key="r.symbol"
              class="flex items-center justify-between text-xs bg-[#0f3460]/60 rounded px-2 py-1"
            >
              <span class="text-white font-mono">{{ r.symbol }}</span>
              <span :style="{ color: scoreColor(r.overall_score) }">{{ (r.overall_score * 100).toFixed(1) }}%</span>
            </div>
            <div v-if="fullValidateResults.length > 50" class="text-[10px] text-[#9e9e9e] text-center">
              ...还有 {{ fullValidateResults.length - 50 }} 只
            </div>
          </div>
        </div>
      </div>
    </template>

    <!-- 错误提示 -->
    <div v-if="error" class="bg-[#ff5722]/10 border border-[#ff5722]/30 rounded-lg p-3 text-xs text-[#ff5722]">
      {{ error }}
    </div>
  </div>
</template>
