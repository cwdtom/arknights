#!/usr/bin/env bash
set -euo pipefail

APP_NAME="arknights"
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${VERSION:-$(grep '^version =' "$ROOT_DIR/Cargo.toml" | head -1 | sed -E 's/version = "(.*)"/\1/')}"
DIST_DIR="$ROOT_DIR/dist"
TARGET_DIR="$ROOT_DIR/target"

MAC_TARGET="aarch64-apple-darwin"
LINUX_TARGET="x86_64-unknown-linux-gnu"

ORT_DIR="$ROOT_DIR/third_party/onnxruntime"
ORT_LIB_DIR="$ORT_DIR/lib"

echo "==> app:      $APP_NAME"
echo "==> version:  $VERSION"
echo "==> root:     $ROOT_DIR"

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

require_file() {
  if [[ ! -f "$1" ]]; then
    echo "ERROR: missing file: $1"
    exit 1
  fi
}

resolve_single_file() {
  local pattern="$1"
  local matches=()

  while IFS= read -r file; do
    matches+=("$file")
  done < <(compgen -G "$pattern" | sort)

  if [[ ${#matches[@]} -ne 1 ]]; then
    echo "ERROR: expected exactly one file matching: $pattern"
    printf ' - %s\n' "${matches[@]}"
    exit 1
  fi

  printf '%s\n' "${matches[0]}"
}

copy_common_files() {
  local out_dir="$1"
  [[ -f "$ROOT_DIR/README.md" ]] && cp "$ROOT_DIR/README.md" "$out_dir/"
  [[ -f "$ROOT_DIR/LICENSE" ]] && cp "$ROOT_DIR/LICENSE" "$out_dir/"
  [[ -f "$ROOT_DIR/LICENSE-MIT" ]] && cp "$ROOT_DIR/LICENSE-MIT" "$out_dir/"
  [[ -f "$ROOT_DIR/LICENSE-APACHE" ]] && cp "$ROOT_DIR/LICENSE-APACHE" "$out_dir/"
  return 0
}

package_tar_gz() {
  local src_dir="$1"
  local out_file="$2"
  tar -C "$(dirname "$src_dir")" -czf "$out_file" "$(basename "$src_dir")"
}

build_macos() {
  echo "==> building macOS arm64"
  cargo build --release --target "$MAC_TARGET"

  local bin="$TARGET_DIR/$MAC_TARGET/release/$APP_NAME"
  local lib
  lib="$(resolve_single_file "$ORT_LIB_DIR/osx-arm64/libonnxruntime*.dylib")"
  local out="$DIST_DIR/${APP_NAME}-v${VERSION}-macos-arm64"

  require_file "$bin"
  require_file "$lib"

  mkdir -p "$out/lib"
  cp "$bin" "$out/$APP_NAME"
  cp "$lib" "$out/lib/"
  copy_common_files "$out"

  cat > "$out/run.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
export DYLD_LIBRARY_PATH="$DIR/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
exec "$DIR/arknights" "$@"
EOF
  chmod +x "$out/run.sh"

  package_tar_gz "$out" "$DIST_DIR/${APP_NAME}-v${VERSION}-macos-arm64.tar.gz"
}

build_linux() {
  echo "==> building Linux x86_64"
  CROSS_CONTAINER_OPTS="--platform linux/amd64" \
    cross build --release --target "$LINUX_TARGET"

  local bin="$TARGET_DIR/$LINUX_TARGET/release/$APP_NAME"
  local lib
  lib="$(resolve_single_file "$ORT_LIB_DIR/linux-x64/libonnxruntime.so*")"
  local out="$DIST_DIR/${APP_NAME}-v${VERSION}-linux-x64"

  require_file "$bin"
  require_file "$lib"

  mkdir -p "$out/lib"
  cp "$bin" "$out/$APP_NAME"
  cp "$lib" "$out/lib/"
  copy_common_files "$out"

  cat > "$out/run.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
export LD_LIBRARY_PATH="$DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$DIR/arknights" "$@"
EOF
  chmod +x "$out/run.sh"

  package_tar_gz "$out" "$DIST_DIR/${APP_NAME}-v${VERSION}-linux-x64.tar.gz"
}

main() {
  build_macos
  build_linux

  echo
  echo "==> done"
  ls -lh "$DIST_DIR"
}

main "$@"
