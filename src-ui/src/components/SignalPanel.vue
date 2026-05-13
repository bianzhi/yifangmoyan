<script setup lang="ts">
import { computed } from "vue";
import type { ChartData, AnalysisSettings } from "../types";
import {
  CZSC_BS_COLORS,
  WYCKOFF_EVENT_COLORS,
  WYCKOFF_PHASE_COLORS,
  WYCKOFF_BULLISH_EVENTS,
} from "../types";

const props = defineProps<{
  chartData: ChartData;
  settings: AnalysisSettings;
}>();

// 缠论信号
const czscSignals = computed(() => {
  if (!props.chartData.czsc) return [];
  const czsc = props.chartData.czsc;
  const signals: { type: string; label: string; index: number; dt: string; price: number; color: string }[] = [];

  // 买卖点
  for (const bs of czsc.buy_sell) {
    const conf = CZSC_BS_COLORS[bs.bs_type] || { color: "#fff", text: bs.bs_type };
    signals.push({
      type: bs.bs_type.includes("buy") ? "buy" : "sell",
      label: conf.text,
      index: bs.index,
      dt: bs.dt,
      price: bs.price,
      color: conf.color,
    });
  }

  // 背驰
  for (const bc of czsc.beichi) {
    let label = "⚡笔背驰";
    if (bc.bc_type === "xd_beichi") label = "⚡线段背驰";
    if (bc.bc_sub_type === "panzheng") label += "(盘整)";
    signals.push({
      type: "beichi",
      label,
      index: bc.index,
      dt: bc.dt,
      price: 0,
      color: "#ff9800",
    });
  }

  return signals.sort((a, b) => a.index - b.index);
});

// 威科夫信号
const wyckoffSignals = computed(() => {
  if (!props.chartData.wyckoff) return [];
  return props.chartData.wyckoff.events.map((e) => ({
    type: WYCKOFF_BULLISH_EVENTS.includes(e.event_type) ? "bullish" : "bearish",
    label: e.event_type,
    index: e.index,
    dt: e.dt,
    price: e.price,
    color: WYCKOFF_EVENT_COLORS[e.event_type] || "#9e9e9e",
    description: e.description,
  })).sort((a, b) => a.index - b.index);
});

// 融合信号
const fusionSignals = computed(() => {
  if (!props.chartData.fusion) return [];
  return props.chartData.fusion.signals.map((s) => ({
    ...s,
    stars: "★".repeat(s.strength),
    color: s.direction === "bullish" ? "#ffd700" : "#ff6d00",
  }));
});

// 当前阶段
const currentPhase = computed(() => {
  if (!props.chartData.wyckoff || props.chartData.wyckoff.phase_labels.length === 0) return null;
  const labels = props.chartData.wyckoff.phase_labels;
  return labels[labels.length - 1];
});
</script>

<template>
  <div class="bg-[#16213e] p-3 space-y-3 text-xs">
    <!-- 当前阶段 -->
    <div v-if="currentPhase" class="pb-2 border-b border-[#2a2a4a]">
      <div class="text-[#9e9e9e] text-[10px] uppercase mb-1">当前阶段</div>
      <div class="flex items-center gap-2">
        <span
          class="inline-block w-2 h-2 rounded-full"
          :style="{ backgroundColor: WYCKOFF_PHASE_COLORS[currentPhase.phase] || '#666' }"
        ></span>
        <span class="font-bold" :style="{ color: WYCKOFF_PHASE_COLORS[currentPhase.phase] }">
          {{ currentPhase.phase }}
        </span>
        <span v-if="currentPhase.sub_phase" class="text-[#9e9e9e]">
          {{ currentPhase.sub_phase }}
        </span>
      </div>
    </div>

    <!-- 缠论信号 -->
    <div v-if="czscSignals.length > 0">
      <div class="text-[#b388ff] text-[10px] uppercase font-bold mb-1.5">缠论信号</div>
      <div class="space-y-1 max-h-40 overflow-y-auto">
        <div
          v-for="(sig, i) in czscSignals.slice(-20)"
          :key="'czsc-' + i"
          class="flex items-center justify-between py-0.5"
        >
          <div class="flex items-center gap-1.5">
            <span class="w-1.5 h-1.5 rounded-full" :style="{ backgroundColor: sig.color }"></span>
            <span :style="{ color: sig.color }" class="font-mono">{{ sig.label }}</span>
          </div>
          <span class="text-[#9e9e9e]">{{ sig.dt.slice(0, 10) }}</span>
        </div>
      </div>
    </div>

    <!-- 威科夫信号 -->
    <div v-if="wyckoffSignals.length > 0">
      <div class="text-[#00bcd4] text-[10px] uppercase font-bold mb-1.5">威科夫信号</div>
      <div class="space-y-1 max-h-40 overflow-y-auto">
        <div
          v-for="(sig, i) in wyckoffSignals.slice(-20)"
          :key="'wy-' + i"
          class="flex items-center justify-between py-0.5"
        >
          <div class="flex items-center gap-1.5">
            <span class="w-1.5 h-1.5 rounded-full" :style="{ backgroundColor: sig.color }"></span>
            <span :style="{ color: sig.color }" class="font-mono">{{ sig.label }}</span>
          </div>
          <span class="text-[#9e9e9e]">{{ sig.dt.slice(0, 10) }}</span>
        </div>
      </div>
    </div>

    <!-- 融合解读 -->
    <div v-if="fusionSignals.length > 0 && settings.fusion.showFusion">
      <div class="h-px bg-[#2a2a4a] mb-2"></div>
      <div class="text-[#ffd700] text-[10px] uppercase font-bold mb-1.5">⚡ 融合解读</div>
      <div class="space-y-2">
        <div
          v-for="(sig, i) in fusionSignals.slice(-10)"
          :key="'fusion-' + i"
          class="bg-[#0f3460] rounded p-2"
        >
          <div class="flex items-center justify-between mb-0.5">
            <span class="font-bold" :style="{ color: sig.color }">
              {{ sig.stars }}
            </span>
            <span class="text-[#9e9e9e]">{{ sig.dt.slice(0, 10) }}</span>
          </div>
          <div class="text-gray-300 leading-snug">{{ sig.interpretation }}</div>
          <div class="text-[#9e9e9e] mt-0.5">
            缠论{{ sig.czsc_type }} + {{ sig.wyckoff_events.join("/") }}
          </div>
        </div>
      </div>
    </div>

    <!-- 无信号 -->
    <div
      v-if="czscSignals.length === 0 && wyckoffSignals.length === 0 && fusionSignals.length === 0"
      class="text-center text-[#666] py-4"
    >
      暂无信号
    </div>
  </div>
</template>
