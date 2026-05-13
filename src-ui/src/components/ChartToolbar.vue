<script setup lang="ts">
import type { TimeFrame, AnalysisSettings, ViewMode } from "../types";
import { TIME_FRAMES } from "../types";

defineProps<{
  timeframe: TimeFrame;
  settings: AnalysisSettings;
  viewMode: ViewMode;
}>();

const emit = defineEmits<{
  (e: "timeframe-change", tf: TimeFrame): void;
  (e: "settings-change", settings: AnalysisSettings): void;
  (e: "view-mode-change", mode: ViewMode): void;
}>();

function selectTf(tf: TimeFrame) {
  emit("timeframe-change", tf);
}
</script>

<template>
  <div class="flex items-center gap-1 px-4 h-9 bg-[#0f3460] border-b border-[#2a2a4a] shrink-0">
    <!-- 时间周期切换 -->
    <div class="flex items-center gap-0.5">
      <button
        v-for="tf in TIME_FRAMES"
        :key="tf.key"
        @click="selectTf(tf.key)"
        class="px-2.5 py-1 text-xs rounded transition-all duration-150"
        :class="timeframe === tf.key
          ? 'bg-[#e94560] text-white font-semibold'
          : 'text-[#9e9e9e] hover:bg-[#1a4a7a] hover:text-white'"
      >
        {{ tf.label }}
      </button>
    </div>

    <div class="w-px h-5 bg-[#2a2a4a] mx-2"></div>

    <!-- 快捷指标显示 -->
    <div class="flex items-center gap-2 text-xs text-[#9e9e9e]">
      <span v-if="settings.czsc.showBi" class="text-[#4a90d9]">笔</span>
      <span v-if="settings.czsc.showXd" class="text-[#b388ff]">线段</span>
      <span v-if="settings.czsc.showBiZs" class="text-[#b388ff]/70">笔中枢</span>
      <span v-if="settings.czsc.showXdZs" class="text-[#b388ff]/50">段中枢</span>
      <span v-if="settings.czsc.showBuySell" class="text-[#00e676]">买卖</span>
      <span v-if="settings.czsc.showBeichi" class="text-[#ff9800]">背驰</span>
    </div>

    <div class="w-px h-5 bg-[#2a2a4a] mx-2" v-if="Object.values(settings.wyckoff).some(Boolean)"></div>

    <div class="flex items-center gap-2 text-xs text-[#9e9e9e]" v-if="Object.values(settings.wyckoff).some(Boolean)">
      <span class="text-[#00bcd4]">WY</span>
    </div>

    <div class="w-px h-5 bg-[#2a2a4a] mx-2" v-if="settings.fusion.showFusion"></div>

    <div class="flex items-center gap-2 text-xs text-[#9e9e9e]" v-if="settings.fusion.showFusion">
      <span class="text-[#ffd700]">融合</span>
    </div>
  </div>
</template>
