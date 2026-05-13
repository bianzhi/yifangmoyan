<script setup lang="ts">
import type { AnalysisSettings } from "../types";

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

function toggleFusion(key: keyof AnalysisSettings["fusion"]) {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  newSettings.fusion[key] = !newSettings.fusion[key];
  emit("change", newSettings);
}

function toggleChart(key: keyof AnalysisSettings["chart"]) {
  const newSettings = JSON.parse(JSON.stringify(props.settings));
  newSettings.chart[key] = !newSettings.chart[key];
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
          v-for="(label, key) in ({ showFenxing: '分型', showBi: '笔', showXd: '线段', showBiZs: '笔中枢', showXdZs: '段中枢', showBuySell: '买卖点', showBeichi: '背驰' } as const)"
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

    <!-- 威科夫设置 -->
    <div>
      <h3 class="text-[10px] font-bold text-[#00bcd4] uppercase tracking-wider mb-1.5">威科夫分析</h3>
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
            :checked="settings.chart.showMacd"
            @change="toggleChart('showMacd')"
            class="w-3 h-3 rounded accent-[#2196f3]"
          />
          <span class="text-xs text-gray-300 group-hover:text-white transition-colors">MACD 副图</span>
        </label>
      </div>
    </div>

    <div class="h-px bg-[#2a2a4a]"></div>

    <!-- 图例说明 -->
    <div>
      <h3 class="text-[10px] font-bold text-gray-400 uppercase tracking-wider mb-1.5">图例</h3>
      <div class="space-y-0.5 text-xs text-gray-400">
        <div class="flex items-center gap-2">
          <span class="w-3 h-0.5 bg-[#4a90d9]"></span> 笔
        </div>
        <div class="flex items-center gap-2">
          <span class="w-3 h-0.5 bg-[#b388ff]"></span> 线段
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
        <div><kbd class="bg-[#0f3460] px-1 rounded">F</kbd> 融合模式</div>
        <div><kbd class="bg-[#0f3460] px-1 rounded">1-8</kbd> 切换周期</div>
        <div><kbd class="bg-[#0f3460] px-1 rounded">/</kbd> 搜索</div>
      </div>
    </div>
  </aside>
</template>
