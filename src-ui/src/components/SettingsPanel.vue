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
</script>

<template>
  <aside class="w-52 bg-[#16213e] border-l border-[#2a2a4a] overflow-y-auto shrink-0 p-3 space-y-4">
    <!-- 缠论设置 -->
    <div>
      <h3 class="text-xs font-bold text-[#b388ff] uppercase tracking-wider mb-2">缠论</h3>
      <div class="space-y-1.5">
        <label
          v-for="(label, key) in ({ showBi: '笔', showXd: '线段', showBiZs: '笔中枢', showXdZs: '段中枢', showBuySell: '买卖点', showBeichi: '背驰' } as const)"
          :key="key"
          class="flex items-center gap-2 cursor-pointer group"
        >
          <input
            type="checkbox"
            :checked="settings.czsc[key]"
            @change="toggleCzsc(key)"
            class="w-3.5 h-3.5 rounded accent-[#b388ff]"
          />
          <span class="text-xs text-gray-300 group-hover:text-white transition-colors">{{ label }}</span>
        </label>
      </div>
    </div>

    <div class="h-px bg-[#2a2a4a]"></div>

    <!-- 威科夫设置 -->
    <div>
      <h3 class="text-xs font-bold text-[#00bcd4] uppercase tracking-wider mb-2">威科夫</h3>
      <div class="space-y-1.5">
        <label
          v-for="(label, key) in ({ showTrendLines: '趋势线', showTR: '交易区间', showIceLine: '冰线', showLPS: 'LPS', showJOC: 'JOC', showSpring: 'Spring', showUTAD: 'UTAD' } as const)"
          :key="key"
          class="flex items-center gap-2 cursor-pointer group"
        >
          <input
            type="checkbox"
            :checked="settings.wyckoff[key]"
            @change="toggleWyckoff(key)"
            class="w-3.5 h-3.5 rounded accent-[#00bcd4]"
          />
          <span class="text-xs text-gray-300 group-hover:text-white transition-colors">{{ label }}</span>
        </label>
      </div>
    </div>

    <div class="h-px bg-[#2a2a4a]"></div>

    <!-- 图例说明 -->
    <div>
      <h3 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">图例</h3>
      <div class="space-y-1 text-xs text-gray-400">
        <div class="flex items-center gap-2">
          <span class="w-3 h-0.5 bg-[#ef5350]"></span> 阳线
        </div>
        <div class="flex items-center gap-2">
          <span class="w-3 h-0.5 bg-[#26a69a]"></span> 阴线
        </div>
        <div class="flex items-center gap-2">
          <span class="w-3 h-0.5 bg-[#4a90d9]"></span> 笔
        </div>
        <div class="flex items-center gap-2">
          <span class="w-3 h-0.5 bg-[#b388ff]"></span> 线段
        </div>
        <div class="flex items-center gap-2">
          <span class="text-[#00e676] text-base">↑</span> 买点
        </div>
        <div class="flex items-center gap-2">
          <span class="text-[#ff1744] text-base">↓</span> 卖点
        </div>
        <div class="flex items-center gap-2">
          <span class="text-[#ff9800]">⚠</span> 背驰
        </div>
        <div class="flex items-center gap-2">
          <span class="w-3 h-0.5 bg-[#00bcd4]"></span> 威科夫
        </div>
      </div>
    </div>
  </aside>
</template>
