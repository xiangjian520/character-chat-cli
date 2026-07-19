#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; NC='\033[0m'

info()  { echo -e "${GREEN}[*]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }

DEFAULT_DIR="${HOME}/character-chat-cli"
UNINSTALL_DIR="${1:-$DEFAULT_DIR}"

echo -e "${CYAN}"
echo "  ┌────────────────────────────┐"
echo "  │  Character-Chat CLI  卸载   │"
echo "  └────────────────────────────┘"
echo -e "${NC}"

if [ ! -d "$UNINSTALL_DIR" ]; then
    warn "目录不存在: ${UNINSTALL_DIR}"
    echo "  用法: $0 [安装目录]"
    echo "  默认: ${DEFAULT_DIR}"
    exit 0
fi

# ─── 确认 ───
echo ""
echo -e "  将删除: ${RED}${UNINSTALL_DIR}${NC}"
read -r -p "  确认? [y/N] " confirm
if [ "${confirm,,}" != "y" ] && [ "${confirm,,}" != "yes" ]; then
    echo "  已取消"
    exit 0
fi

# ─── 删除快捷方式 ───
for link in /usr/local/bin/character-chat /usr/bin/character-chat; do
    if [ -L "$link" ]; then
        sudo rm -f "$link"
        info "已删除快捷方式: ${link}"
    fi
done

# ─── 删除目录 ───
rm -rf "$UNINSTALL_DIR"
info "已删除: ${UNINSTALL_DIR}"

echo ""
echo -e "${GREEN}  卸载完成${NC}"
