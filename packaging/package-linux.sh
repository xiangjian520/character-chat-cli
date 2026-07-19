#!/bin/bash
# ============================================================
# Linux 打包脚本 for character-chat-cli
# 使用方法: chmod +x package-linux.sh && ./package-linux.sh
# ============================================================
set -e

APP_NAME="character-chat-cli"
VERSION="0.1.1"
RELEASE_DIR="release-linux"
INSTALL_DIR="${RELEASE_DIR}/${APP_NAME}-${VERSION}"

echo "=== Building ${APP_NAME} v${VERSION} for Linux ==="

# 0. 安装系统依赖 (如未安装)
if [ -f /etc/debian_version ]; then
    echo "[Debian/Ubuntu] Checking dependencies..."
    sudo apt-get install -y build-essential libssl-dev pkg-config libasound2-dev
elif [ -f /etc/redhat-release ]; then
    echo "[RHEL/Fedora] Checking dependencies..."
    sudo dnf install -y openssl-devel pkg-config alsa-lib-devel
elif [ -f /etc/arch-release ]; then
    echo "[Arch] Checking dependencies..."
    sudo pacman -S --needed base-devel openssl pkg-config alsa-lib
fi

# 1. 编译 Release 版本
echo "=== Building release binary ==="
cargo build --release

# 2. 准备打包目录
echo "=== Preparing package directory ==="
rm -rf "${INSTALL_DIR}"
mkdir -p "${INSTALL_DIR}/personas"
mkdir -p "${INSTALL_DIR}/plugins"
mkdir -p "${INSTALL_DIR}/data"

cp target/release/${APP_NAME} "${INSTALL_DIR}/"
cp README.md LICENSE "${INSTALL_DIR}/" 2>/dev/null || true
cp -r personas/* "${INSTALL_DIR}/personas/" 2>/dev/null || true

# 3. 创建启动脚本
cat > "${INSTALL_DIR}/start.sh" << 'EOF'
#!/bin/bash
cd "$(dirname "$0")"
exec ./character-chat-cli
EOF
chmod +x "${INSTALL_DIR}/start.sh"

# ============================================================
# 方法 A: 打包为 .tar.gz (通用分发)
# ============================================================
echo "=== Creating .tar.gz archive ==="
cd "${RELEASE_DIR}"
tar -czf "../${APP_NAME}-${VERSION}-linux-x86_64.tar.gz" "${APP_NAME}-${VERSION}"
cd ..
echo "  -> ${APP_NAME}-${VERSION}-linux-x86_64.tar.gz"

# ============================================================
# 方法 B: 打包为 .deb (Debian/Ubuntu) -- 需要 cargo-deb
# ============================================================
echo ""
echo "=== Creating .deb package ==="
if cargo deb --version >/dev/null 2>&1; then
    cargo deb --target x86_64-unknown-linux-gnu
    echo "  -> target/debian/${APP_NAME}_${VERSION}_amd64.deb"
else
    echo "  [SKIP] cargo-deb not installed. Install with: cargo install cargo-deb"
    echo "  Then add this to Cargo.toml:"
    echo ""
    echo "  [package.metadata.deb]"
    echo '  maintainer = "xiangjian520"'
    echo '  copyright = "2026, xiangjian520"'
    echo '  license-file = ["LICENSE", "0"]'
    echo '  extended-description = "AI-powered character-roleplay chat CLI"'
    echo '  depends = "libssl3, libasound2"'
    echo '  section = "utils"'
    echo '  priority = "optional"'
    echo '  assets = ['
    echo '      ["personas/*", "usr/share/character-chat-cli/personas/", "644"],'
    echo '      ["README.md", "usr/share/doc/character-chat-cli/README", "644"],'
    echo '  ]'
fi

# ============================================================
# 方法 C: 打包为 .rpm (RHEL/Fedora) -- 需要 cargo-rpm
# ============================================================
echo ""
echo "=== Creating .rpm package ==="
if cargo rpm --version >/dev/null 2>&1; then
    cargo rpm build
    echo "  -> target/release/rpmbuild/RPMS/x86_64/${APP_NAME}-${VERSION}-1.x86_64.rpm"
else
    echo "  [SKIP] cargo-rpm not installed. Install with: cargo install cargo-rpm"
fi

# ============================================================
# 方法 D: 打包为 AppImage (独立可执行)
# ============================================================
echo ""
echo "=== Creating AppImage ==="
if command -v linuxdeploy >/dev/null 2>&1; then
    # 使用 linuxdeploy 打包
    linuxdeploy --appdir AppDir --executable target/release/${APP_NAME} \
        --desktop-file ${APP_NAME}.desktop --icon-file assets/icon.png \
        --output appimage
    mv *.AppImage "${APP_NAME}-${VERSION}-x86_64.AppImage"
    echo "  -> ${APP_NAME}-${VERSION}-x86_64.AppImage"
elif command -v appimagetool >/dev/null 2>&1; then
    mkdir -p AppDir/usr/bin AppDir/usr/share/applications AppDir/usr/share/icons
    cp target/release/${APP_NAME} AppDir/usr/bin/
    
    cat > AppDir/usr/share/applications/${APP_NAME}.desktop << DESKTOPEOF
[Desktop Entry]
Name=Character Chat CLI
Exec=${APP_NAME}
Type=Application
Terminal=true
Categories=Utility;
DESKTOPEOF

    cat > AppDir/AppRun << 'APPROOT'
#!/bin/bash
HERE="$(dirname "$(readlink -f "${0}")")"
exec "${HERE}/usr/bin/character-chat-cli" "$@"
APPROOT
    chmod +x AppDir/AppRun

    ARCH=x86_64 appimagetool AppDir "${APP_NAME}-${VERSION}-x86_64.AppImage"
    rm -rf AppDir
    echo "  -> ${APP_NAME}-${VERSION}-x86_64.AppImage"
else
    echo "  [SKIP] appimagetool not found."
    echo "  Install from: https://github.com/AppImage/AppImageKit/releases"
fi

# ============================================================
# 方法 E: 使用 musl 静态编译 (完全独立，无 glibc 依赖)
# ============================================================
echo ""
echo "=== Static musl build ==="
if rustup target list --installed | grep -q "x86_64-unknown-linux-musl"; then
    cargo build --release --target x86_64-unknown-linux-musl
    cp target/x86_64-unknown-linux-musl/release/${APP_NAME} "${APP_NAME}-${VERSION}-linux-x86_64-static"
    chmod +x "${APP_NAME}-${VERSION}-linux-x86_64-static"
    echo "  -> ${APP_NAME}-${VERSION}-linux-x86_64-static (fully static binary)"
else
    echo "  [SKIP] musl target not installed."
    echo "  Install with: rustup target add x86_64-unknown-linux-musl"
    echo "  Also install: musl-tools (apt) or musl-gcc (dnf)"
fi

echo ""
echo "=== Done! ==="
ls -lh ${APP_NAME}-${VERSION}-*.tar.gz ${APP_NAME}-${VERSION}-*.AppImage ${APP_NAME}-${VERSION}-*-static 2>/dev/null || true
