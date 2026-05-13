<script setup lang="ts">
import { useWatchlist } from "../composables/useStorage";
import { ref } from "vue";

defineProps<{
  currentSymbol: string;
}>();

const emit = defineEmits<{
  (e: "select", symbol: string): void;
  (e: "remove", symbol: string): void;
}>();

const { watchlist, removeFromWatchlist } = useWatchlist();
const showRemoveConfirm = ref<string | null>(null);

function selectStock(sym: string) {
  emit("select", sym);
}

function onRemove(sym: string) {
  if (showRemoveConfirm.value === sym) {
    removeFromWatchlist(sym);
    emit("remove", sym);
    showRemoveConfirm.value = null;
  } else {
    showRemoveConfirm.value = sym;
    // 2秒后自动取消确认
    setTimeout(() => {
      if (showRemoveConfirm.value === sym) {
        showRemoveConfirm.value = null;
      }
    }, 2000);
  }
}
</script>

<template>
  <div class="h-full flex flex-col">
    <div class="flex items-center justify-between px-3 py-2 border-b border-[#2a2a4a]">
      <h3 class="text-xs font-bold text-gray-300 uppercase tracking-wider">自选股</h3>
      <span class="text-[10px] text-[#9e9e9e]">{{ watchlist.length }}只</span>
    </div>

    <div v-if="watchlist.length === 0" class="flex-1 flex items-center justify-center p-4">
      <div class="text-center text-[#666] text-xs">
        <div class="mb-2">暂无自选股</div>
        <div class="text-[10px]">搜索股票后点击 ☆ 添加</div>
      </div>
    </div>

    <div v-else class="flex-1 overflow-y-auto">
      <div
        v-for="sym in watchlist"
        :key="sym"
        class="flex items-center justify-between px-3 py-1.5 hover:bg-[#0f3460] cursor-pointer group transition-colors"
        :class="{ 'bg-[#0f3460]': sym === currentSymbol }"
        @click="selectStock(sym)"
      >
        <span
          class="text-sm font-mono"
          :class="sym === currentSymbol ? 'text-[#e94560]' : 'text-gray-300'"
        >
          {{ sym }}
        </span>
        <button
          @click.stop="onRemove(sym)"
          class="text-[10px] px-1 rounded transition-all"
          :class="showRemoveConfirm === sym ? 'text-[#ff1744] bg-[#ff1744]/10' : 'text-[#666] hover:text-[#ff5722] opacity-0 group-hover:opacity-100'"
        >
          {{ showRemoveConfirm === sym ? "确认？" : "✕" }}
        </button>
      </div>
    </div>
  </div>
</template>
