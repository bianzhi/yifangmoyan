<script setup lang="ts">
import type { AnalysisSettings, ChartStyles, LineStyle, ZhongShuStyle } from "../types";
import { DEFAULT_CHART_STYLES } from "../types";

const props = defineProps<{
  settings: AnalysisSettings;
}>();

const emit = defineEmits<{
  (e: "change", settings: AnalysisSettings): void;
}>();

function toggleCzsc(key: keyof AnalysisSettings["czsc"]) {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  newSettings.czsc[key] = !newSettings.czsc[key];
  emit("change", newSettings);
}

function toggleWyckoff(key: keyof AnalysisSettings["wyckoff"]) {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  newSettings.wyckoff[key] = !newSettings.wyckoff[key];
  emit("change", newSettings);
}

const ALL_WYCKOFF_EVENT_KEYS: (keyof AnalysisSettings["wyckoff"])[] = [
  "showSC", "showAR", "showST", "showSpring", "showSOS", "showLPS", "showJOC",
  "showPSY", "showBC", "showUTAD", "showSOW", "showLPSY",
];

function toggleAllWyckoffEvents() {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  const allOn = ALL_WYCKOFF_EVENT_KEYS.every((k) => newSettings.wyckoff[k]);
  const newVal = !allOn;
  for (const k of ALL_WYCKOFF_EVENT_KEYS) {
    newSettings.wyckoff[k] = newVal;
  }
  emit("change", newSettings);
}

function toggleFusion(key: keyof AnalysisSettings["fusion"]) {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  newSettings.fusion[key] = !newSettings.fusion[key];
  emit("change", newSettings);
}

function toggleChart(key: keyof AnalysisSettings["chart"]) {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  if (!newSettings.chart) newSettings.chart = { showMacd: true };
  newSettings.chart[key] = !newSettings.chart[key];
  emit("change", newSettings);
}

function updateLineStyle(key: keyof Pick<ChartStyles, "bi" | "xd">, field: keyof LineStyle, value: string | number) {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  if (!newSettings.styles) newSettings.styles = { ...DEFAULT_CHART_STYLES };
  (newSettings.styles[key] as any)[field] = value;
  emit("change", newSettings);
}

function updateZhongShuStyle(key: keyof Pick<ChartStyles, "biZs" | "xdZs">, field: keyof ZhongShuStyle, value: string | number) {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  if (!newSettings.styles) newSettings.styles = { ...DEFAULT_CHART_STYLES };
  (newSettings.styles[key] as any)[field] = value;
  emit("change", newSettings);
}

function resetStyles() {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  newSettings.styles = { ...DEFAULT_CHART_STYLES };
  emit("change", newSettings);
}
</script>

<template>
  <aside class="bg-[#16213e] border-t border-[#2a2a4a] overflow-y-auto shrink-0 p-3 space-y-3 max-h-80">
    <!-- 缠论设置 -->
    <div>
      <h3 class="text-[10px] font-bold text-[#b388ff] uppercase tracking-wider mb-1.5">缠论分析</h3>
      <div class="space-y-1">
        <label
          v-for="(label, key) in ({ showFenxing: '分型', showBi: '笔', showXd: '线段', showBiZs: '笔中枢', showXdZs: '段中枢', show1buy: '一买', show2buy: '二买', show3buy: '三买', show1sell: '一卖', show2sell: '二卖', show3sell: '三卖', showBeichi: '背驰' } as const)"
          :key="key"
          class="flex items-center gap-2 cursor-pointer group"
        >
          <input
            type="checkbox"
            :checked="settings.czsc[key]"
            @change="toggleCzsc(key)"
            class="w-3 h-3 rounded accent-[#b388ff]"
          />
          <span class="text-xs text-gray-300 group-hover:text-white transition-colors">{{ label }}</span>
        </label>
      </div>
    </div>

    <div class="h-px bg-[#2a2a4a]"></div>

    <!-- 样式配置 -->
    <div>
      <div class="flex items-center justify-between mb-1.5">
        <h3 class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">线条样式</h3>
        <button @click="resetStyles" class="text-[9px] text-[#9e9e9e] hover:text-white transition-colors">重置</button>
      </div>

      <!-- 笔样式 -->
      <div class="space-y-1 bg-[#0f3460] rounded p-2 mb-1.5">
        <div class="flex items-center justify-between">
          <span class="text-[10px] text-gray-400">笔</span>
          <span class="text-[10px] text-gray-500">{{ settings.styles?.bi?.lineWidth ?? 1 }}px</span>
        </div>
        <div class="flex items-center gap-2">
          <input
            type="color"
            :value="settings.styles?.bi?.color ?? '#4a90d9'"
            @input="updateLineStyle('bi', 'color', ($event.target as HTMLInputElement).value)"
            class="w-5 h-5 rounded cursor-pointer border-0 bg-transparent"
          />
          <input
            type="range" min="1" max="4" step="1"
            :value="settings.styles?.bi?.lineWidth ?? 1"
            @input="updateLineStyle('bi', 'lineWidth', Number(($event.target as HTMLInputElement).value))"
            class="flex-1 h-1 accent-[#4a90d9]"
          />
        </div>
      </div>

      <!-- 线段样式 -->
      <div class="space-y-1 bg-[#0f3460] rounded p-2 mb-1.5">
        <div class="flex items-center justify-between">
          <span class="text-[10px] text-gray-400">线段</span>
          <span class="text-[10px] text-gray-500">{{ settings.styles?.xd?.lineWidth ?? 3 }}px</span>
        </div>
        <div class="flex items-center gap-2">
          <input
            type="color"
            :value="settings.styles?.xd?.color ?? '#b388ff'"
            @input="updateLineStyle('xd', 'color', ($event.target as HTMLInputElement).value)"
            class="w-5 h-5 rounded cursor-pointer border-0 bg-transparent"
          />
          <input
            type="range" min="1" max="4" step="1"
            :value="settings.styles?.xd?.lineWidth ?? 3"
            @input="updateLineStyle('xd', 'lineWidth', Number(($event.target as HTMLInputElement).value))"
            class="flex-1 h-1 accent-[#b388ff]"
          />
        </div>
      </div>

      <!-- 笔中枢样式 -->
      <div class="space-y-1 bg-[#0f3460] rounded p-2 mb-1.5">
        <div class="flex items-center justify-between">
          <span class="text-[10px] text-gray-400">笔中枢</span>
          <span class="text-[10px] text-gray-500">{{ settings.styles?.biZs?.borderWidth ?? 2 }}px</span>
        </div>
        <div class="flex items-center gap-2">
          <div class="flex items-center gap-1">
            <span class="text-[9px] text-gray-500">边</span>
            <input
              type="color"
              :value="settings.styles?.biZs?.borderColor ?? '#b388ff'"
              @input="updateZhongShuStyle('biZs', 'borderColor', ($event.target as HTMLInputElement).value)"
              class="w-4 h-4 rounded cursor-pointer border-0 bg-transparent"
            />
          </div>
          <div class="flex items-center gap-1">
            <span class="text-[9px] text-gray-500">填</span>
            <input
              type="color"
              :value="settings.styles?.biZs?.fillColor ?? '#b388ff'"
              @input="updateZhongShuStyle('biZs', 'fillColor', ($event.target as HTMLInputElement).value)"
              class="w-4 h-4 rounded cursor-pointer border-0 bg-transparent"
            />
          </div>
          <input
            type="range" min="1" max="4" step="1"
            :value="settings.styles?.biZs?.borderWidth ?? 2"
            @input="updateZhongShuStyle('biZs', 'borderWidth', Number(($event.target as HTMLInputElement).value))"
            class="flex-1 h-1 accent-[#b388ff]"
          />
        </div>
      </div>

      <!-- 线段中枢样式 -->
      <div class="space-y-1 bg-[#0f3460] rounded p-2">
        <div class="flex items-center justify-between">
          <span class="text-[10px] text-gray-400">段中枢</span>
          <span class="text-[10px] text-gray-500">{{ settings.styles?.xdZs?.borderWidth ?? 2 }}px</span>
        </div>
        <div class="flex items-center gap-2">
          <div class="flex items-center gap-1">
            <span class="text-[9px] text-gray-500">边</span>
            <input
              type="color"
              :value="settings.styles?.xdZs?.borderColor ?? '#ff9800'"
              @input="updateZhongShuStyle('xdZs', 'borderColor', ($event.target as HTMLInputElement).value)"
              class="w-4 h-4 rounded cursor-pointer border-0 bg-transparent"
            />
          </div>
          <div class="flex items-center gap-1">
            <span class="text-[9px] text-gray-500">填</span>
            <input
              type="color"
              :value="settings.styles?.xdZs?.fillColor ?? '#ff9800'"
              @input="updateZhongShuStyle('xdZs', 'fillColor', ($event.target as HTMLInputElement).value)"
              class="w-4 h-4 rounded cursor-pointer border-0 bg-transparent"
            />
          </div>
          <input
            type="range" min="1" max="4" step="1"
            :value="settings.styles?.xdZs?.borderWidth ?? 2"
            @input="updateZhongShuStyle('xdZs', 'borderWidth', Number(($event.target as HTMLInputElement).value))"
            class="flex-1 h-1 accent-[#ff9800]"
          />
        </div>
      </div>
    </div>

    <div class="h-px bg-[#2a2a4a]"></div>

    <!-- 威科夫设置 -->
    <div>
      <div class="flex items-center justify-between mb-1.5">
        <h3 class="text-[10px] font-bold text-[#00bcd4] uppercase tracking-wider">威科夫分析</h3>
        <button
          @click="toggleAllWyckoffEvents"
          class="text-[9px] px-1.5 py-0.5 rounded bg-[#0f3460] text-[#00bcd4] hover:bg-[#1a4a7a] hover:text-white transition-colors"
          title="一键开关全部事件信号"
        >全部开/关</button>
      </div>
      <div class="space-y-1">
        <label
          v-for="(label, key) in ({ showPhase: '阶段色带', showTR: '交易区间', showIceLine: '冰线', showSupplyDemand: '供需线' } as const)"
          :key="key"
          class="flex items-center gap-2 cursor-pointer group"
        >
          <input
            type="checkbox"
            :checked="settings.wyckoff[key]"
            @change="toggleWyckoff(key)"
            class="w-3 h-3 rounded accent-[#00bcd4]"
          />
          <span class="text-xs text-gray-300 group-hover:text-white transition-colors">{{ label }}</span>
        </label>
      </div>

      <!-- 关键信号 -->
      <div class="mt-1.5 text-[10px] text-[#9e9e9e] uppercase mb-1">关键信号</div>
      <div class="space-y-1">
        <label
          v-for="(label, key) in ({ showSC: 'SC 卖出高潮', showAR: 'AR 自动反弹', showST: 'ST 二次测试', showSpring: 'Spring 弹簧', showSOS: 'SOS 强势信号', showLPS: 'LPS 最后支撑', showJOC: 'JOC 跳过小溪' } as const)"
          :key="key"
          class="flex items-center gap-2 cursor-pointer group"
        >
          <input
            type="checkbox"
            :checked="settings.wyckoff[key]"
            @change="toggleWyckoff(key)"
            class="w-3 h-3 rounded accent-[#00bcd4]"
          />
          <span class="text-xs text-gray-300 group-hover:text-white transition-colors">{{ label }}</span>
        </label>
      </div>

      <div class="mt-1.5 text-[10px] text-[#9e9e9e] uppercase mb-1">派发信号</div>
      <div class="space-y-1">
        <label
          v-for="(label, key) in ({ showPSY: 'PSY 初步供给', showBC: 'BC 买入高潮', showUTAD: 'UTAD 派发冲高', showSOW: 'SOW 弱势信号', showLPSY: 'LPSY 最后供给' } as const)"
          :key="key"
          class="flex items-center gap-2 cursor-pointer group"
        >
          <input
            type="checkbox"
            :checked="settings.wyckoff[key]"
            @change="toggleWyckoff(key)"
            class="w-3 h-3 rounded accent-[#00bcd4]"
          />
          <span class="text-xs text-gray-300 group-hover:text-white transition-colors">{{ label }}</span>
        </label>
      </div>
    </div>

    <div class="h-px bg-[#2a2a4a]"></div>

    <!-- 融合设置 -->
    <div>
      <h3 class="text-[10px] font-bold text-[#ffd700] uppercase tracking-wider mb-1.5">融合解读</h3>
      <div class="space-y-1">
        <label class="flex items-center gap-2 cursor-pointer group">
          <input
            type="checkbox"
            :checked="settings.fusion.showFusion"
            @change="toggleFusion('showFusion')"
            class="w-3 h-3 rounded accent-[#ffd700]"
          />
          <span class="text-xs text-gray-300 group-hover:text-white transition-colors">显示融合信号</span>
        </label>
      </div>
    </div>

    <div class="h-px bg-[#2a2a4a]"></div>

    <!-- 图表设置 -->
    <div>
      <h3 class="text-[10px] font-bold text-[#2196f3] uppercase tracking-wider mb-1.5">图表</h3>
      <div class="space-y-1">
        <label class="flex items-center gap-2 cursor-pointer group">
          <input
            type="checkbox"
            :checked="settings.chart?.showMacd"
            @change="toggleChart('showMacd')"
            class="w-3 h-3 rounded accent-[#2196f3]"
          />
          <span class="text-xs text-gray-300 group-hover:text-white transition-colors">MACD 副图</span>
        </label>
      </div>
    </div>

    <div class="h-px bg-[#2a2a4a]"></div>

    <!-- 图例说明（动态反映当前样式） -->
    <div>
      <h3 class="text-[10px] font-bold text-gray-400 uppercase tracking-wider mb-1.5">图例</h3>
      <div class="space-y-0.5 text-xs text-gray-400">
        <div class="flex items-center gap-2">
          <span class="w-3" :style="{ height: settings.styles?.bi?.lineWidth + 'px', backgroundColor: settings.styles?.bi?.color || '#4a90d9' }"></span> 笔
        </div>
        <div class="flex items-center gap-2">
          <span class="w-3" :style="{ height: settings.styles?.xd?.lineWidth + 'px', backgroundColor: settings.styles?.xd?.color || '#b388ff' }"></span> 线段
        </div>
        <div class="flex items-center gap-2">
          <span
            class="w-3 h-2.5 rounded-sm border"
            :style="{ borderColor: settings.styles?.biZs?.borderColor || '#b388ff', backgroundColor: settings.styles?.biZs?.fillColor || 'rgba(179,136,255,0.08)' }"
          ></span> 笔中枢
        </div>
        <div class="flex items-center gap-2">
          <span
            class="w-3 h-2.5 rounded-sm border"
            :style="{ borderColor: settings.styles?.xdZs?.borderColor || '#ff9800', backgroundColor: settings.styles?.xdZs?.fillColor || 'rgba(255,152,0,0.08)' }"
          ></span> 段中枢
        </div>
        <div class="flex items-center gap-2">
          <span class="text-[#00e676]">●</span> 买点
        </div>
        <div class="flex items-center gap-2">
          <span class="text-[#ff1744]">●</span> 卖点
        </div>
        <div class="flex items-center gap-2">
          <span class="text-[#ff9800]">⚡</span> 背驰
        </div>
        <div class="flex items-center gap-2">
          <span class="text-[#00bcd4]">■</span> 威科夫
        </div>
        <div class="flex items-center gap-2">
          <span class="text-[#ffd700]">★</span> 融合
        </div>
      </div>
    </div>

    <!-- 快捷键提示 -->
    <div>
      <h3 class="text-[10px] font-bold text-gray-400 uppercase tracking-wider mb-1.5">快捷键</h3>
      <div class="space-y-0.5 text-[10px] text-gray-500">
        <div><kbd class="bg-[#0f3460] px-1 rounded">0</kbd> 纯K线</div>
        <div><kbd class="bg-[#0f3460] px-1 rounded">B</kbd> 缠论模式</div>
        <div><kbd class="bg-[#0f3460] px-1 rounded">W</kbd> 威科夫模式</div>
        <div><kbd class="bg-[#0f3460] px-1 rounded">E</kbd> 开关威科夫事件</div>
        <div><kbd class="bg-[#0f3460] px-1 rounded">F</kbd> 融合模式</div>
        <div><kbd class="bg-[#0f3460] px-1 rounded">1-8</kbd> 切换周期</div>
        <div><kbd class="bg-[#0f3460] px-1 rounded">/</kbd> 搜索</div>
      </div>
    </div>
  </aside>
</template>
