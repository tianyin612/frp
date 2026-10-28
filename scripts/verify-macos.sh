#!/usr/bin/env bash
#
# 发布前自查：把"用户装上去会不会被拦"这件事在本地先验一遍。
# 用法：./scripts/verify-macos.sh /path/to/frpc-desktop.app
#
set -uo pipefail

APP="${1:?用法: verify-macos.sh <path/to/xxx.app>}"
FAILED=0

log()  { printf '\033[36m▸ %s\033[0m\n' "$*"; }
ok()   { printf '\033[32m✓ %s\033[0m\n' "$*"; }
bad()  { printf '\033[31m✗ %s\033[0m\n' "$*"; FAILED=1; }

[[ -d "$APP" ]] || { echo "找不到 $APP"; exit 1; }

log "1. 主程序代码签名"
SIG="$(codesign -dv --verbose=4 "$APP" 2>&1)"
if echo "$SIG" | grep -q "Authority=Developer ID Application"; then
  ok "$(echo "$SIG" | grep 'Authority=' | head -1)"
else
  bad "主程序不是 Developer ID 签名"
fi
echo "$SIG" | grep -q "flags=.*runtime" && ok "hardened runtime 已启用" || bad "未启用 hardened runtime"

log "2. 内置 frpc sidecar 签名"
FRPC="$APP/Contents/MacOS/frpc"
if [[ -f "$FRPC" ]]; then
  FSIG="$(codesign -dv --verbose=4 "$FRPC" 2>&1)"
  echo "$FSIG" | grep -q "Authority=Developer ID Application" && ok "sidecar 已签名" || bad "sidecar 未签名（公证必挂）"
  echo "$FSIG" | grep -q "flags=.*runtime" && ok "sidecar hardened runtime 已启用" || bad "sidecar 未启用 hardened runtime"
  ok "架构：$(lipo -archs "$FRPC" 2>/dev/null || echo unknown)"
else
  log "未内置 frpc，跳过"
fi

log "3. 深度校验"
codesign --verify --deep --strict "$APP" 2>/dev/null && ok "codesign --verify --deep --strict" || bad "签名链不完整"

log "4. Gatekeeper（spctl）"
if spctl -a -vvv -t exec "$APP" 2>&1 | grep -q "accepted"; then
  ok "spctl accepted"
else
  bad "spctl 未通过：$(spctl -a -vvv -t exec "$APP" 2>&1 | head -2)"
fi

log "5. 公证票据（staple）"
xcrun stapler validate "$APP" >/dev/null 2>&1 && ok "票据已 staple" || bad "没有 staple 票据，离线首启会被拦"

log "6. quarantine 残留"
if xattr -lr "$APP" 2>/dev/null | grep -qi quarantine; then
  bad "仍带 com.apple.quarantine 标记"
else
  ok "无隔离标记"
fi

echo
if [[ "$FAILED" == "0" ]]; then
  printf '\033[32m全部通过，可以发布\033[0m\n'
else
  printf '\033[31m存在未通过项，先修再发\033[0m\n'
  exit 1
fi
