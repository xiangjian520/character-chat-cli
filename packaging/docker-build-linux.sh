#!/bin/bash
# ============================================================
# 在 Windows 上使用 Docker 交叉编译 Linux 二进制文件
# 前提: 安装 Docker Desktop for Windows
# ============================================================
set -e

APP_NAME="character-chat-cli"
VERSION="0.1.1"
PROJECT_DIR=$(pwd)

echo "=== Cross-compiling ${APP_NAME} for Linux using Docker ==="

# 使用 Rust 官方 Docker 镜像编译
docker run --rm -v "${PROJECT_DIR}:/build" -w /build \
    rust:1.75-slim-bookworm bash -c "
        apt-get update && apt-get install -y libssl-dev pkg-config libasound2-dev
        cargo build --release
        echo 'Build complete'
    "

# 打包
mkdir -p "linux-package/${APP_NAME}-${VERSION}"
cp "target/release/${APP_NAME}" "linux-package/${APP_NAME}-${VERSION}/"
cp README.md LICENSE "linux-package/${APP_NAME}-${VERSION}/" 2>/dev/null || true
cp -r personas "linux-package/${APP_NAME}-${VERSION}/" 2>/dev/null || true

cat > "linux-package/${APP_NAME}-${VERSION}/start.sh" << 'EOF'
#!/bin/bash
cd "$(dirname "$0")"
exec ./character-chat-cli
EOF
chmod +x "linux-package/${APP_NAME}-${VERSION}/start.sh"

cd linux-package
tar -czf "../${APP_NAME}-${VERSION}-linux-x86_64.tar.gz" "${APP_NAME}-${VERSION}"
cd ..

echo "Done: ${APP_NAME}-${VERSION}-linux-x86_64.tar.gz"
