#!/usr/bin/env bash
# 墨岩K线 — 发布管理脚本
# 用法：./scripts/release.sh <version> [notes]
# 示例：./scripts/release.sh 0.1.0 "首个测试版本"

set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${1:?用法: $0 <version> [notes]}"
NOTES="${2:-版本 $VERSION}"

echo "============================================"
echo "  墨岩K线 发布 v${VERSION}"
echo "============================================"

# 1. 构建 DMG
echo ""
echo "[1/5] 正在编译 macOS 版本..."
cargo tauri build --bundles dmg

# Tauri 构建产物通常在 src-tauri/target/release/bundle/
BUNDLE_DIR="target/release/bundle"
DMG_DIR="${BUNDLE_DIR}/dmg"
DMG_FILE=$(ls -t "${DMG_DIR}"/*.dmg 2>/dev/null | head -1)

if [ ! -f "${DMG_FILE}" ]; then
    echo "错误: 找不到 DMG 文件，检查编译是否成功"
    exit 1
fi

# 目标文件名：Yifang_{version}_{arch}.dmg (纯 ASCII，避免 URL 编码问题)
DMG_NAME=$(basename "${DMG_FILE}")
ARCH=""

# 检测架构
if [[ "$DMG_NAME" == *"aarch64"* ]]; then
    ARCH="aarch64"
elif [[ "$DMG_NAME" == *"x64"* || "$DMG_NAME" == *"x86"* ]]; then
    ARCH="x86_64"
elif [[ "$DMG_NAME" == *"universal"* ]]; then
    ARCH="universal"
else
    # 默认：检测当前机器架构
    if [ "$(uname -m)" = "arm64" ]; then
        ARCH="aarch64"
    else
        ARCH="x86_64"
    fi
fi

DMG_ASCII="Yifang_${VERSION}_${ARCH}.dmg"

echo ""
echo "[2/5] 构建完成: ${DMG_NAME}"
echo "       架构: ${ARCH}"

# 2. 创建版本目录并复制 DMG（重命名为 ASCII）
RELEASE_DIR="releases/v${VERSION}"
mkdir -p "${RELEASE_DIR}"
cp "${DMG_FILE}" "${RELEASE_DIR}/${DMG_ASCII}"

echo "[3/5] DMG 已复制到 ${RELEASE_DIR}/${DMG_ASCII}"

# 3. 生成签名（如果启用了签名）
# Tauri v2 使用 ZIP + 签名，如果签名不存在则跳过签名验证
SIGNATURE=""
if [ -f "${BUNDLE_DIR}/dmg/${DMG_NAME}.sig" ]; then
    SIGNATURE=$(cat "${BUNDLE_DIR}/dmg/${DMG_NAME}.sig")
    echo "       签名文件已找到"
else
    echo "       未找到签名，将跳过签名验证"
fi

# 4. 更新 latest.json 清单
echo "[4/5] 更新 latest.json..."

DMG_URL="https://gitcode.com/Ai3/yifangmoyan/raw/main/releases/v${VERSION}/${DMG_ASCII}"

# 平台键名
OS_ARCH="darwin-${ARCH}"

# 检查是否已有 releases/latest.json
LATEST_JSON="releases/latest.json"
PUB_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

PLATFORM_ENTRY=$(cat <<EOF
{
    "signature": "${SIGNATURE}",
    "url": "${DMG_URL}"
}
EOF
)

# 创建或更新 latest.json
cat > "${LATEST_JSON}" <<EOF
{
  "version": "${VERSION}",
  "notes": "${NOTES}",
  "pub_date": "${PUB_DATE}",
  "platforms": {
    "${OS_ARCH}": ${PLATFORM_ENTRY}
  }
}
EOF

# 5. 清理旧版本（保留最近 3 个）
echo "[5/5] 清理旧版本（保留最近 3 个）..."

# 列出所有版本目录，按版本排序，删除旧的
RELEASE_DIRS=$(ls -1d releases/v*/ 2>/dev/null | sort -V)
COUNT=$(echo "${RELEASE_DIRS}" | grep -c "v" || true)

if [ "${COUNT}" -gt 3 ]; then
    TO_DELETE=$(echo "${RELEASE_DIRS}" | head -n -3)
    for dir in ${TO_DELETE}; do
        echo "       删除旧版本: ${dir}"
        rm -rf "${dir}"
    done
else
    echo "       当前 ${COUNT} 个版本，无需清理"
fi

echo ""
echo "============================================"
echo "  发布完成!"
echo "  DMG:   ${RELEASE_DIR}/${DMG_NAME}"
echo "  清单:  ${LATEST_JSON}"
echo ""
echo "  下一步:"
echo "    git tag v${VERSION}"
echo "    git add releases/ && git commit -m 'release: v${VERSION}'"
echo "    git push --tags"
echo "============================================"
