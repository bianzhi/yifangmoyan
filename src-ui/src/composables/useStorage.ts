import { ref, watch } from "vue";

/** 深合并：用 defaults 填充 target 中缺失的键（递归） */
function deepMerge<T>(defaults: T, target: any): T {
  if (typeof defaults !== "object" || defaults === null) return defaults;
  const result = { ...(defaults as any) };
  for (const key of Object.keys(defaults as any)) {
    if (key in target) {
      if (typeof (defaults as any)[key] === "object" && (defaults as any)[key] !== null && !Array.isArray((defaults as any)[key])) {
        result[key] = deepMerge((defaults as any)[key], target[key]);
      } else {
        result[key] = target[key];
      }
    }
  }
  // 保留 target 中存在但 defaults 中没有的键（向前兼容）
  for (const key of Object.keys(target)) {
    if (!(key in result)) {
      result[key] = target[key];
    }
  }
  return result as T;
}

// ===== 自选股管理 =====

const STORAGE_KEY = "moyan_watchlist";

function loadWatchlist(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveWatchlist(list: string[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(list));
}

const watchlist = ref<string[]>(loadWatchlist());

watch(watchlist, (val) => saveWatchlist(val), { deep: true });

export function useWatchlist() {
  function addToWatchlist(symbol: string) {
    if (!watchlist.value.includes(symbol)) {
      watchlist.value.push(symbol);
    }
  }

  function removeFromWatchlist(symbol: string) {
    watchlist.value = watchlist.value.filter((s) => s !== symbol);
  }

  function isInWatchlist(symbol: string): boolean {
    return watchlist.value.includes(symbol);
  }

  return {
    watchlist,
    addToWatchlist,
    removeFromWatchlist,
    isInWatchlist,
  };
}

// ===== 设置持久化 =====

const SETTINGS_KEY = "moyan_settings";

export function usePersistedSettings<T>(key: string, defaultValue: T) {
  const storageKey = `${SETTINGS_KEY}_${key}`;

  function load(): T {
    try {
      const raw = localStorage.getItem(storageKey);
      return raw ? deepMerge(defaultValue, JSON.parse(raw)) : defaultValue;
    } catch {
      return defaultValue;
    }
  }

  function save(val: T) {
    localStorage.setItem(storageKey, JSON.stringify(val));
  }

  const data = ref<T>(load()) as any;

  watch(data, (val) => save(val), { deep: true });

  return data;
}
