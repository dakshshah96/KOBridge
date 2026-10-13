#!/bin/sh
# Builds the aria2c that ships inside KOBridge, as a Tauri sidecar:
# src-tauri/binaries/aria2c-<target triple>. macOS uses AppleTLS and only system libraries,
# for macOS 11 and later. Linux links glibc, OpenSSL 3 and zlib dynamically, the same libraries
# the app itself needs, because a static glibc can't resolve names on other distributions.
set -eu
VER=1.37.0
SHA=60a420ad7085eb616cb6e2bdf0a7206d68ff3d37fb5a956dc44242eb2f79b66b
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
OUT="$ROOT/src-tauri/binaries/aria2c-$TRIPLE"
[ -x "$OUT" ] && { echo "$OUT already built"; exit 0; }

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"
curl -fsSL -o aria2.tar.xz "https://github.com/aria2/aria2/releases/download/release-$VER/aria2-$VER.tar.xz"
echo "$SHA  aria2.tar.xz" | shasum -a 256 -c -
tar xf aria2.tar.xz
cd "aria2-$VER"

COMMON="--disable-nls --disable-dependency-tracking --without-libxml2 --without-libexpat --without-sqlite3 \
  --without-libssh2 --without-libcares --without-gnutls --without-libnettle --without-libgmp --without-libgcrypt"
case "$(uname)" in
  Darwin)
    # no Homebrew: an empty pkg-config path keeps configure on the system SDK
    export MACOSX_DEPLOYMENT_TARGET=11.0
    PKG_CONFIG_LIBDIR=/nonexistent PKG_CONFIG_PATH= ./configure $COMMON --with-appletls --without-openssl \
      CC=clang CXX=clang++ CPPFLAGS= LDFLAGS= ;;
  *)
    ./configure $COMMON --with-openssl --with-ca-bundle=/etc/ssl/certs/ca-certificates.crt ;;
esac
make -j"$(getconf _NPROCESSORS_ONLN)"
strip src/aria2c
mkdir -p "$(dirname "$OUT")"
cp src/aria2c "$OUT"
cp COPYING "$ROOT/src-tauri/binaries/aria2-COPYING"
if [ "$(uname)" = Darwin ]; then
  if otool -L "$OUT" | grep -q /opt/homebrew; then echo "aria2c links Homebrew libraries" >&2; rm "$OUT"; exit 1; fi
fi
"$OUT" --version | head -n1
