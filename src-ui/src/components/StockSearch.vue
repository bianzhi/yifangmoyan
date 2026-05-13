<script setup lang="ts">
import type { StockInfo } from "../types";

defineProps<{
  modelValue: string;
  results: StockInfo[];
  show: boolean;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", val: string): void;
  (e: "search"): void;
  (e: "select", symbol: string): void;
  (e: "close"): void;
}>();

function onInput(event: Event) {
  emit("update:modelValue", (event.target as HTMLInputElement).value);
}

function onKeyup(event: KeyboardEvent) {
  if (event.key === "Enter") {
    emit("search");
  }
  if (event.key === "Escape") {
    emit("close");
  }
}

function onSelect(sym: string) {
  emit("select", sym);
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
          @focus="show === false"
          placeholder="输入股票代码/拼音"
          class="bg-transparent text-sm text-white ml-2 w-36 outline-none placeholder-[#666]"
        />
      </div>
    </div>

    <!-- 搜索结果下拉 -->
    <div
      v-if="show && results.length > 0"
      class="absolute top-full left-0 mt-1 w-64 max-h-60 overflow-y-auto bg-[#16213e] border border-[#2a2a4a] rounded shadow-lg z-50"
    >
      <div
        v-for="stock in results.slice(0, 20)"
        :key="stock.symbol"
        @click="onSelect(stock.symbol)"
        class="flex items-center justify-between px-3 py-2 hover:bg-[#0f3460] cursor-pointer text-sm"
      >
        <span class="text-gray-300">{{ stock.symbol }}</span>
        <span class="text-[#9e9e9e]">{{ stock.market }}</span>
      </div>
    </div>
  </div>
</template>
