#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="${DIR}/target/release/character-chat-cli"
CONFIG="${DIR}/config.json"

# ─── 颜色 ───
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'

# ─── 检查 Redis ───
if command -v redis-cli &>/dev/null; then
    if redis-cli ping &>/dev/null 2>&1; then
        echo -e "${GREEN}[Redis]${NC} 连接正常"
    else
        echo -e "${YELLOW}[Redis]${NC} 未运行，尝试启动..."
        if command -v systemctl &>/dev/null; then
            sudo systemctl start redis 2>/dev/null || redis-server --daemonize yes 2>/dev/null
        else
            redis-server --daemonize yes 2>/dev/null
        fi
        sleep 1
        if redis-cli ping &>/dev/null 2>&1; then
            echo -e "${GREEN}[Redis]${NC} 已启动"
        else
            echo -e "${RED}[Redis]${NC} 启动失败，请手动启动 Redis 后重试"
            exit 1
        fi
    fi
else
    echo -e "${YELLOW}[Redis]${NC} 未检测到 redis-cli，跳过检查"
fi

# ─── 编译（如需要） ───
if [ ! -f "$BIN" ]; then
    echo -e "${YELLOW}[编译]${NC} 未找到二进制文件，开始编译..."
    cd "$DIR"
    cargo build --release
    echo -e "${GREEN}[编译]${NC} 完成"
fi

# ─── 生成默认配置 ───
if [ ! -f "$CONFIG" ]; then
    echo -e "${YELLOW}[配置]${NC} config.json 不存在，将自动生成默认配置"
fi

# ─── 启动 ───
cd "$DIR"
exec "$BIN"
