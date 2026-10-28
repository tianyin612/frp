#!/usr/bin/env bash
#
# macOS 签名 + 公证 + 打包 DMG，一键跑完并自查。
#
# 用法：
#   export SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
#   export APPLE_ID="you@example.com"
#   export APPLE_TEAM_ID="TEAMID"
#   export APPLE_APP_PASSWORD="abcd-efgh-ijkl-mnop"   # app 专用密码
#   ./scripts/sign-macos.sh
#
# 可选：
#   TARGET=aarch64-apple-darwin | x86_64-apple-darwin | universal-apple-darwin
#   SKIP_BUILD=1      跳过 pnpm tauri build（复用已有产物）
#   SKIP_NOTARIZE=1   只签名不公证（本地调试用）
#
set -euo pipefail

APP_NAME="frpc-desktop"
TARGET="${TARGET:-aarch64-apple-darwin}"
IDENTITY="${SIGN_IDENTITY:-}"
APPLE_ID="${APPLE_ID:-}"
TEAM_ID="${APPLE_TEAM_ID:-}"
APP_PASSWORD="${APPLE_APP_PASSWORD:-}"
SKIP_BUILD="${SKIP_BUILD:-0}"
SKIP_NOTARIZE="${SKIP_NOTARIZE:-0}"

log()  { printf '\033[36m▸ %s\033[0m\n' "$*"; }
ok()   { printf '\033[32m✓ %s\033[0m\n' "$*"; }
fail() { printf '\033[31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="$ROOT/src-tauri/target/$TARGET/release"
APP="$OUT/bundle/macos/$APP_NAME.app"
VERSION="$(node -p "require('./package.json').version")"
ENTITLEMENTS="$ROOT/src-tauri/entitlements.plist"

[[ "$(uname)" == "Darwin" ]] || fail "签名只能跑在 macOS 上"
[[ -n "$IDENTITY" ]] || fail "缺少 SIGN_IDENTITY"
security find-identity -v -p codesigning | grep -qF "$IDENTITY" \
  || fail "钥匙串里找不到证书：$IDENTITY（先导入 .p12）"
[[ -f "$ENTITLEMENTS" ]] || fail "缺少 entitlements.plist"

# ---------------------------------------------------------------- 1. 构建
if [[ "$SKIP_BUILD" != "1" ]]; then
  log "构建 $TARGET —— 刻意屏蔽 Tauri 的自动签名/公证，统一交给本脚本，避免顺序错乱"
  env -u APPLE_SIGNING_IDENTITY -u APPLE_CERTIFICATE \
      -u APPLE_CERTIFICATE_PASSWORD -u APPLE_ID \
      -u APPLE_PASSWORD -u APPLE_TEAM_ID \
      pnpm tauri build --target "$TARGET"
fi

[[ -d "$APP" ]] || fail "找不到产物：$APP"

# ------------------------------------------------- 2. 清扩展属性（否则签名带脏数据）
log "清理扩展属性"
xattr -cr "$APP"

# ------------------------------------- 3. 由内到外签名（含内置 frpc sidecar）
log "签名嵌套 Mach-O（最深的先签）"
count=0
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if file -b "$f" | grep -q "Mach-O"; then
    codesign --force --timestamp --options runtime --sign "$IDENTITY" "$f"
    count=$((count + 1))
  fi
done < <(
  find "$APP/Contents" -type f -print \
    | awk -F/ '{print NF"\t"$0}' | sort -rn | cut -f2-
)
ok "已签名 $count 个 Mach-O（主程序 + frpc sidecar + 其他）"

# 兜底：确认 sidecar 版本/架构对得上，签错架构公证也会过、但用户机器跑不了
FRPC_IN_APP="$APP/Contents/MacOS/frpc"
if [[ -f "$FRPC_IN_APP" ]]; then
  log "内置 frpc 架构：$(lipo -archs "$FRPC_IN_APP" 2>/dev/null || echo unknown)"
  case "$TARGET" in
    aarch64-apple-darwin) [[ "$(lipo -archs "$FRPC_IN_APP")" == *arm64* ]] || fail "sidecar 不是 arm64" ;;
    x86_64-apple-darwin)  [[ "$(lipo -archs "$FRPC_IN_APP")" == *x86_64* ]] || fail "sidecar 不是 x86_64" ;;
    universal-apple-darwin) [[ "$(lipo -archs "$FRPC_IN_APP")" == *arm64*x86_64* || "$(lipo -archs "$FRPC_IN_APP")" == *x86_64*arm64* ]] || fail "sidecar 不是通用二进制" ;;
  esac
else
  log "未内置 frpc（用户需在设置里指定路径），跳过架构校验"
fi

# --------------------------------------------------------- 4. 签外层 .app
log "签名 App Bundle（带 entitlements + hardened runtime）"
codesign --force --timestamp --options runtime \
  --entitlements "$ENTITLEMENTS" \
  --sign "$IDENTITY" "$APP"

codesign --verify --deep --strict --verbose=2 "$APP" 2>&1 | tail -3
ok "签名校验通过"

# ------------------------------------------------------------ 5. 公证
mkdir -p "$OUT/bundle/dmg"
DMG="$OUT/bundle/dmg/${APP_NAME}_${VERSION}_${TARGET}.dmg"
ZIP="$OUT/${APP_NAME}.zip"

if [[ "$SKIP_NOTARIZE" != "1" ]]; then
  [[ -n "$APPLE_ID" && -n "$TEAM_ID" && -n "$APP_PASSWORD" ]] \
    || fail "公证需要 APPLE_ID / APPLE_TEAM_ID / APPLE_APP_PASSWORD"

  log "打包 zip 并公证 .app"
  rm -f "$ZIP"
  ditto -c -k --keepParent "$APP" "$ZIP"

  xcrun notarytool submit "$ZIP" \
    --apple-id "$APPLE_ID" --team-id "$TEAM_ID" --password "$APP_PASSWORD" \
    --wait --output-format json > "$OUT/notarize-app.json" \
    || { cat "$OUT/notarize-app.json"; fail "公证提交失败"; }
  STATUS="$(node -p "require('$OUT/notarize-app.json').status" 2>/dev/null || echo '')"
  [[ "$STATUS" == "Accepted" ]] || {
    SUB_ID="$(node -p "require('$OUT/notarize-app.json').id" 2>/dev/null || echo '')"
    echo "查看原因：xcrun notarytool log $SUB_ID --apple-id \"$APPLE_ID\" --team-id \"$TEAM_ID\" --password \"\$APPLE_APP_PASSWORD\""
    fail "App 公证未通过：$STATUS"
  }
  ok "App 公证通过"

  log "staple 到 .app（离线也能通过 Gatekeeper）"
  xcrun stapler staple "$APP"
  xcrun stapler validate "$APP"

  log "制作 DMG"
  hdiutil create -volname "$APP_NAME" -srcfolder "$APP" -ov -format UDZO "$DMG" >/dev/null

  log "公证 DMG"
  xcrun notarytool submit "$DMG" \
    --apple-id "$APPLE_ID" --team-id "$TEAM_ID" --password "$APP_PASSWORD" \
    --wait --output-format json > "$OUT/notarize-dmg.json" \
    || { cat "$OUT/notarize-dmg.json"; fail "DMG 公证提交失败"; }
  STATUS="$(node -p "require('$OUT/notarize-dmg.json').status" 2>/dev/null || echo '')"
  [[ "$STATUS" == "Accepted" ]] || fail "DMG 公证未通过：$STATUS"

  xcrun stapler staple "$DMG"
  xcrun stapler validate "$DMG"
  ok "DMG 公证 + staple 完成"
else
  log "跳过公证（SKIP_NOTARIZE=1），仅打包 DMG"
  hdiutil create -volname "$APP_NAME" -srcfolder "$APP" -ov -format UDZO "$DMG" >/dev/null
fi

# --------------------------------------------------------- 6. 发布前自查
log "Gatekeeper 检查"
spctl -a -vvv -t exec "$APP" 2>&1 | sed 's/^/    /' || true
if [[ -f "$DMG" ]]; then
  spctl -a -vvv -t open --context context:primary-signature "$DMG" 2>&1 | sed 's/^/    /' || true
fi

log "隔离属性检查（应为空）"
xattr -lr "$APP" 2>/dev/null | grep -i quarantine && fail "产物仍带 quarantine 标记" || true

echo
ok "完成"
echo "  产物：$DMG"
echo "  自查：./scripts/verify-macos.sh \"$APP\""
