#!/usr/bin/env bash
# Builds vyu in release mode and packages it for Linux (x86_64):
#   dist/vyu_<version>_amd64.deb   (install: sudo apt install ./dist/vyu_*.deb)
#   dist/vyu-linux-x64.tar.gz      (binary + .desktop + icon)
#
# FFmpeg is linked from the system (libavcodec etc.), so the .deb depends on
# the distro's FFmpeg packages. Build on the oldest Ubuntu you want to support.
#
# Usage: scripts/package-linux.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
ARCH="$(dpkg --print-architecture)"

cargo build --release

DIST="$ROOT/dist"
rm -rf "$DIST/deb" "$DIST/vyu-linux"
mkdir -p "$DIST"

# --- .deb -------------------------------------------------------------------
PKG="$DIST/deb"
install -Dm755 -s target/release/vyu "$PKG/usr/bin/vyu"
install -Dm644 packaging/linux/vyu.desktop "$PKG/usr/share/applications/vyu.desktop"
install -Dm644 packaging/linux/vyu.svg "$PKG/usr/share/icons/hicolor/scalable/apps/vyu.svg"
install -Dm644 LICENSE "$PKG/usr/share/doc/vyu/copyright"
install -Dm644 THIRD_PARTY.md "$PKG/usr/share/doc/vyu/THIRD_PARTY.md"
install -Dm644 README.md "$PKG/usr/share/doc/vyu/README.md"

# Runtime dependencies: the packages owning the shared libraries vyu links to
# directly (DT_NEEDED); apt pulls in their own dependencies.
DEPENDS="$(
    readelf -d target/release/vyu | sed -n 's/.*Shared library: \[\(.*\)\]/\1/p' | while read -r soname; do
        lib="$(ldconfig -p | awk -v s="$soname" '$1 == s && /x86-64/ {print $NF; exit}')"
        [ -n "$lib" ] && { dpkg -S "$lib" 2>/dev/null || dpkg -S "$(readlink -f "$lib")" 2>/dev/null || true; }
    done | cut -d: -f1 | sort -u | grep -v -E '^(libc6|libgcc-s1)$' | paste -sd, | sed 's/,/, /g'
)"
DEPENDS="libc6, libgcc-s1${DEPENDS:+, $DEPENDS}"
SIZE="$(du -sk "$PKG/usr" | cut -f1)"

mkdir -p "$PKG/DEBIAN"
cat > "$PKG/DEBIAN/control" <<CONTROL
Package: vyu
Version: $VERSION
Architecture: $ARCH
Maintainer: vyu <https://github.com/engviniciusbs/vyu>
Installed-Size: $SIZE
Depends: $DEPENDS
Recommends: gnome-keyring | kwalletmanager
Section: video
Priority: optional
Homepage: https://github.com/engviniciusbs/vyu
Description: Visualizador leve para câmeras Tapo/RTSP
 Mostra várias câmeras RTSP numa tela livre, com áudio por câmera,
 layouts salvos e reconexão automática.
CONTROL

DEB="$DIST/vyu_${VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group --build "$PKG" "$DEB" >/dev/null
rm -rf "$PKG"

# --- tar.gz -----------------------------------------------------------------
OUT="$DIST/vyu-linux"
mkdir -p "$OUT"
install -m755 -s target/release/vyu "$OUT/vyu"
cp packaging/linux/vyu.desktop packaging/linux/vyu.svg LICENSE THIRD_PARTY.md README.md "$OUT/"
tar -C "$DIST" -czf "$DIST/vyu-linux-x64.tar.gz" vyu-linux
rm -rf "$OUT"

echo "Pronto: $DEB"
echo "        $DIST/vyu-linux-x64.tar.gz"
