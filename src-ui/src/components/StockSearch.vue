<script setup lang="ts">
import { ref, watch } from "vue";
import { searchStocks } from "../composables/useApi";
import type { StockInfo } from "../types";

const props = defineProps<{
  modelValue: string;
  results: StockInfo[];
  show: boolean;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", val: string): void;
  (e: "search"): void;
  (e: "select", symbol: string, name: string): void;
  (e: "close"): void;
}>();

const localResults = ref<StockInfo[]>([]);
const localShow = ref(false);
let debounceTimer: ReturnType<typeof setTimeout> | null = null;

// 实时拼音/代码搜索，200ms防抖
watch(() => props.modelValue, (val) => {
  if (debounceTimer) clearTimeout(debounceTimer);
  if (!val.trim()) {
    localResults.value = [];
    localShow.value = false;
    return;
  }
  debounceTimer = setTimeout(async () => {
    try {
      const results = await searchStocks(val.trim());
      localResults.value = results.slice(0, 20);
      localShow.value = results.length > 0;
    } catch {
      localResults.value = [];
      localShow.value = false;
    }
  }, 200);
});

function onInput(event: Event) {
  emit("update:modelValue", (event.target as HTMLInputElement).value);
}

function onKeyup(event: KeyboardEvent) {
  if (event.key === "Enter") {
    // 如果有下拉结果，自动选择第一个
    if (localResults.value.length > 0) {
      const first = localResults.value[0];
      onSelect(first.symbol, first.name);
    } else {
      emit("search");
    }
  }
  if (event.key === "Escape") {
    localShow.value = false;
    emit("close");
  }
}

function onSelect(sym: string, name: string) {
  localShow.value = false;
  emit("select", sym, name);
}

function onBlur() {
  // 延迟关闭，让点击事件先触发
  setTimeout(() => {
    localShow.value = false;
  }, 200);
}
</script>

<template>
  <div class="relative">
    <div class="flex items-center gap-2">
      <div class="flex items-center bg-[#0f3460] rounded px-2 py-1">
        <svg class="w-4 h-4 text-[#9e9e9e]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        <input
          id="stock-search-input"
          :value="modelValue"
          @input="onInput"
          @keyup="onKeyup"
          @blur="onBlur"
          placeholder="输入代码/名称/拼音"
          class="bg-transparent text-sm text-white ml-2 w-44 outline-none placeholder-[#666]"
        />
      </div>
    </div>

    <!-- 搜索结果下拉 -->
    <div
      v-if="localShow && localResults.length > 0"
      class="absolute top-full left-0 mt-1 w-72 max-h-60 overflow-y-auto bg-[#16213e] border border-[#2a2a4a] rounded shadow-lg z-50"
    >
      <div
        v-for="stock in localResults"
        :key="stock.symbol"
        @mousedown.prevent="onSelect(stock.symbol, stock.name)"
        class="flex items-center justify-between px-3 py-2 hover:bg-[#0f3460] cursor-pointer text-sm"
      >
        <div class="flex items-center gap-2">
          <span class="text-gray-300 font-mono">{{ stock.symbol }}</span>
          <span class="text-white">{{ stock.name }}</span>
          <span v-if="stock.pinyin" class="text-[#666] text-xs">{{ stock.pinyin }}</span>
        </div>
        <span class="text-[#9e9e9e] text-xs">{{ stock.market }}</span>
      </div>
    </div>
  </div>
</template>
