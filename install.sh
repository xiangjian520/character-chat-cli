#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────
#  Character-Chat CLI  Linux 一键安装脚本
#  GitHub: https://github.com/xiangjian520/character-chat-cli
# ─────────────────────────────────────────────

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

INSTALL_DIR="${HOME}/character-chat-cli"
BRANCH="master"
RELEASE_BUILD=true
REPO_URL=""
valid_urls=()

# ─── 镜像源（中国大陆优先） ───
MIRRORS=(
    "https://gitclone.com/github.com/xiangjian520/character-chat-cli.git"
    "https://wget.la/https://github.com/xiangjian520/character-chat-cli.git"
    "https://hk.gh-proxy.org/https://github.com/xiangjian520/character-chat-cli.git"
    "https://ghfast.top/https://github.com/xiangjian520/character-chat-cli.git"
    "https://githubfast.com/xiangjian520/character-chat-cli.git"
    "https://github.com/xiangjian520/character-chat-cli.git"
)

# ─────────────────────────────────────────────
#  工具函数
# ─────────────────────────────────────────────

info()    { echo -e "${GREEN}[*]${NC} $*"; }
warn()    { echo -e "${YELLOW}[!]${NC} $*"; }
error()   { echo -e "${RED}[x]${NC} $*"; exit 1; }
header()  { echo -e "\n${CYAN}${BOLD}── $* ──${NC}\n"; }

# 检测发行版
detect_distro() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        DISTRO_ID="${ID}"
        DISTRO_LIKE="${ID_LIKE:-}"
    elif [ -f /etc/debian_version ]; then
        DISTRO_ID="debian"
    elif [ -f /etc/redhat-release ]; then
        DISTRO_ID="rhel"
    elif [ -f /etc/arch-release ]; then
        DISTRO_ID="arch"
    else
        error "无法识别当前 Linux 发行版"
    fi
    info "检测到系统: ${DISTRO_ID}"
}

# 检测包管理器并安装依赖
install_deps() {
    header "安装系统依赖"

    case "$DISTRO_ID" in
        ubuntu|debian|deepin|uos|kali|mint|raspbian|pop)
            info "使用 apt 安装依赖..."
            sudo apt update -y
            sudo apt install -y build-essential libssl-dev pkg-config libasound2-dev \
                                curl git redis-server
            ;;
        rhel|centos|fedora|rocky|alma|anolis|openEuler)
            if command -v dnf &>/dev/null; then
                info "使用 dnf 安装依赖..."
                sudo dnf groupinstall -y "Development Tools"
                sudo dnf install -y openssl-devel pkg-config alsa-lib-devel \
                                    curl git redis
            else
                info "使用 yum 安装依赖..."
                sudo yum groupinstall -y "Development Tools"
                sudo yum install -y openssl-devel pkg-config alsa-lib-devel \
                                    curl git redis
            fi
            ;;
        arch|manjaro|endeavouros)
            info "使用 pacman 安装依赖..."
            sudo pacman -Sy --noconfirm base-devel openssl pkg-config alsa-lib \
                                      curl git redis
            ;;
        opensuse*|sles)
            info "使用 zypper 安装依赖..."
            sudo zypper install -y -t pattern devel_basis
            sudo zypper install -y openssl-devel pkg-config alsa-devel \
                                    curl git redis
            ;;
        alpine)
            info "使用 apk 安装依赖..."
            sudo apk add build-base openssl-dev pkgconfig alsa-lib-dev \
                            curl git redis
            ;;
        *)
            warn "未识别的发行版，尝试继续..."
            ;;
    esac

    # 启动 Redis
    if command -v redis-server &>/dev/null; then
        if ! redis-cli ping &>/dev/null 2>&1; then
            info "启动 Redis..."
            if command -v systemctl &>/dev/null; then
                sudo systemctl enable --now redis 2>/dev/null || redis-server --daemonize yes
            else
                redis-server --daemonize yes
            fi
        else
            info "Redis 已在运行"
        fi
    fi
}

# 安装 Rust 工具链
install_rust() {
    header "检查 Rust 工具链"
    if command -v rustc &>/dev/null; then
        info "Rust 已安装: $(rustc --version)"
    else
        info "安装 Rust..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
        info "Rust 安装完成"
    fi
}

# 测速并选择最快镜像
pick_mirror() {
    header "测速选择最优镜像"

    local fastest_url=""
    local fastest_time=999
    local valid_urls=()

    for url in "${MIRRORS[@]}"; do
        printf "  ${CYAN}▶${NC} %s ... " "$url"
        local code elapsed
        read -r code elapsed <<<"$(
            curl -sS --connect-timeout 5 --max-time 10 \
                 -w '%{http_code} %{time_total}' -o /dev/null \
                 "$url/info/refs?service=git-upload-pack" 2>/dev/null || echo "999 999"
        )"
        if [ "$code" != "200" ] || [ "$elapsed" = "999" ]; then
            echo -e "${RED}不可用 (HTTP ${code})${NC}"
            continue
        fi
        local t_ms
        t_ms=$(awk "BEGIN {printf \"%.0f\", $elapsed * 1000}")
        echo -e "${GREEN}${t_ms}ms${NC}"
        valid_urls+=("$url")
        if awk "BEGIN {exit !($elapsed < $fastest_time)}"; then
            fastest_time="$elapsed"
            fastest_url="$url"
        fi
    done

    if [ -z "$fastest_url" ]; then
        warn "所有镜像不可达，使用 GitHub 官方源"
        REPO_URL="${MIRRORS[-1]}"
    else
        REPO_URL="$fastest_url"
    fi
    info "选择: ${REPO_URL}"
}

# 克隆仓库（失败自动回退到下一个镜像）
clone_repo() {
    header "克隆仓库"

    if [ -d "$INSTALL_DIR/.git" ]; then
        info "仓库已存在, 更新..."
        git -C "$INSTALL_DIR" fetch origin "$BRANCH"
        git -C "$INSTALL_DIR" checkout "$BRANCH"
        git -C "$INSTALL_DIR" pull origin "$BRANCH" 2>/dev/null || true
        cd "$INSTALL_DIR"
        return
    fi

    rm -rf "$INSTALL_DIR"

    # 用最快的 URL 优先，失败则依次回退
    local try_urls=()
    if [ -n "$REPO_URL" ]; then
        try_urls+=("$REPO_URL")
    fi
    if [ ${#valid_urls[@]} -gt 0 ]; then
        for u in "${valid_urls[@]}"; do
            [[ "$u" != "$REPO_URL" ]] && try_urls+=("$u")
        done
    fi
    # 如果没有任何可用 URL，直接把所有镜像塞进去
    if [ ${#try_urls[@]} -eq 0 ]; then
        try_urls=("${MIRRORS[@]}")
    fi

    for url in "${try_urls[@]}"; do
        info "尝试: ${url}"
        if git clone --depth 1 --branch "$BRANCH" "$url" "$INSTALL_DIR" 2>/dev/null; then
            cd "$INSTALL_DIR"
            return
        fi
        warn "克隆失败, 尝试下一个镜像..."
        rm -rf "$INSTALL_DIR"
    done

    error "所有镜像均克隆失败，请检查网络"
}

# 编译
build() {
    header "编译项目"
    if $RELEASE_BUILD; then
        cargo build --release
        info "编译完成: target/release/character-chat-cli"
    else
        cargo build
        info "编译完成: target/debug/character-chat-cli"
    fi
}

# 清理多余文件
cleanup() {
    header "清理编译缓存"
    local kept=0 del=0

    if $RELEASE_BUILD; then
        # 删除 debug 构建目录
        if [ -d target/debug ]; then
            local size
            size=$(du -sh target/debug 2>/dev/null | cut -f1)
            rm -rf target/debug
            info "已删除 target/debug/ (${size})"
            ((del++))
        fi
        # 保留 release 二进制，删除中间产物
        mkdir -p target/keep
        cp target/release/character-chat-cli target/keep/ 2>/dev/null || true
        rm -rf target/release
        mkdir -p target/release
        mv target/keep/character-chat-cli target/release/ 2>/dev/null || true
        rmdir target/keep 2>/dev/null || true
        info "已精简 target/release/ (仅保留二进制)"
        ((del++))
    fi

    # 删除 cargo 增量编译缓存
    if [ -d target/.fingerprint ]; then
        rm -rf target/.fingerprint
        ((del++))
    fi

    info "清理完成"
    info "安装目录: ${INSTALL_DIR}"
    echo ""
}

# ─────────────────────────────────────────────
#  入口
# ─────────────────────────────────────────────

clear 2>/dev/null || true
echo -e "${CYAN}${BOLD}"
echo "  ┌─────────────────────────────────────┐"
echo "  │   Character-Chat CLI  Linux 安装     │"
echo "  │         一键安装脚本 v1.0            │"
echo "  └─────────────────────────────────────┘"
echo -e "${NC}"

# 参数解析
while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug)    RELEASE_BUILD=false; shift ;;
        --dir)      INSTALL_DIR="$2"; shift 2 ;;
        --branch)   BRANCH="$2"; shift 2 ;;
        --skip-deps) SKIP_DEPS=true; shift ;;
        --skip-rust) SKIP_RUST=true; shift ;;
        --skip-build) SKIP_BUILD=true; shift ;;
        --url)       REPO_URL="$2"; SKIP_MIRROR=true; shift 2 ;;
        -h|--help)
            echo "用法: $0 [选项]"
            echo "  --debug        Debug 编译"
            echo "  --dir PATH     安装目录 (默认: ~/character-chat-cli)"
            echo "  --branch NAME  Git 分支 (默认: master)"
            echo "  --skip-deps    跳过系统依赖安装"
            echo "  --skip-rust    跳过 Rust 安装"
            echo "  --skip-build   跳过编译"
            echo "  --url URL      指定仓库地址 (跳过测速)"
            exit 0 ;;
        *) shift ;;
    esac
done

detect_distro
[ "${SKIP_DEPS:-false}" = true ] || install_deps
[ "${SKIP_RUST:-false}" = true ] || install_rust
[ "${SKIP_MIRROR:-false}" = true ] || pick_mirror
clone_repo
[ "${SKIP_BUILD:-false}" = true ] || build
cleanup

echo -e "${GREEN}${BOLD}╔══════════════════════════════════════╗${NC}"
echo -e "${GREEN}${BOLD}║   Character-Chat CLI 安装完成!       ║${NC}"
echo -e "${GREEN}${BOLD}╚══════════════════════════════════════╝${NC}"
echo ""
echo -e "  目录: ${INSTALL_DIR}"
echo -e "  启动: ${INSTALL_DIR}/target/release/character-chat-cli"
echo ""
echo -e "  首次运行前请编辑配置:"
echo -e "    ${CYAN}cd ${INSTALL_DIR}${NC}"
echo -e "    ${CYAN}nano config.json${NC}   (设置 api_key)"
echo -e "  或设置环境变量:"
echo -e "    ${CYAN}export DEEPSEEK_API_KEY=\"sk-xxx\"${NC}"
echo ""
echo -e "  快捷方式 (可选):"
echo -e "    ${CYAN}sudo ln -s ${INSTALL_DIR}/target/release/character-chat-cli /usr/local/bin/character-chat${NC}"
echo ""
