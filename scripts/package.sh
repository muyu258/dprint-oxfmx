#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
HOST_TARGET=$(rustc -vV | awk '/^host:/ { print $2 }')
if [[ -z "$HOST_TARGET" ]]; then
  printf 'Could not determine the Rust host target.\n' >&2
  exit 1
fi
if [[ -n "${TARGET:-}" && "$TARGET" != "$HOST_TARGET" ]]; then
  printf 'Cross-compilation is not supported: TARGET=%s, host=%s.\n' "$TARGET" "$HOST_TARGET" >&2
  exit 2
fi
if [[ -n "${CARGO_BUILD_TARGET:-}" && "$CARGO_BUILD_TARGET" != "$HOST_TARGET" ]]; then
  printf 'Cross-compilation is not supported: CARGO_BUILD_TARGET=%s, host=%s.\n' "$CARGO_BUILD_TARGET" "$HOST_TARGET" >&2
  exit 2
fi

case "$HOST_TARGET" in
  aarch64-apple-darwin)
    DPRINT_PLATFORM="darwin-aarch64"
    EXECUTABLE_SUFFIX=""
    ;;
  x86_64-apple-darwin)
    DPRINT_PLATFORM="darwin-x86_64"
    EXECUTABLE_SUFFIX=""
    ;;
  aarch64-unknown-linux-gnu)
    DPRINT_PLATFORM="linux-aarch64"
    EXECUTABLE_SUFFIX=""
    ;;
  x86_64-unknown-linux-gnu)
    DPRINT_PLATFORM="linux-x86_64"
    EXECUTABLE_SUFFIX=""
    ;;
  aarch64-unknown-linux-musl)
    DPRINT_PLATFORM="linux-aarch64-musl"
    EXECUTABLE_SUFFIX=""
    ;;
  x86_64-unknown-linux-musl)
    DPRINT_PLATFORM="linux-x86_64-musl"
    EXECUTABLE_SUFFIX=""
    ;;
  aarch64-pc-windows-msvc)
    DPRINT_PLATFORM="windows-aarch64"
    EXECUTABLE_SUFFIX=".exe"
    ;;
  x86_64-pc-windows-msvc)
    DPRINT_PLATFORM="windows-x86_64"
    EXECUTABLE_SUFFIX=".exe"
    ;;
  *)
    printf 'Unsupported dprint process-plugin host target: %s.\n' "$HOST_TARGET" >&2
    exit 3
    ;;
esac

command -v zip >/dev/null 2>&1 || {
  printf 'The zip command is required to package a dprint process plugin.\n' >&2
  exit 4
}

PLUGIN_NAME="dprint-plugin-oxfmt"
EXECUTABLE_NAME="$PLUGIN_NAME$EXECUTABLE_SUFFIX"
RUNTIME_VERSION=$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).version)' "$ROOT_DIR/runtime/package.json")
EXPECTED_NODE_VERSION=$(tr -d '[:space:]' < "$ROOT_DIR/.node-version")
ACTUAL_NODE_VERSION=$(node --version)
if [[ "$ACTUAL_NODE_VERSION" != "v$EXPECTED_NODE_VERSION" ]]; then
  printf 'Expected Node v%s, found %s.\n' "$EXPECTED_NODE_VERSION" "$ACTUAL_NODE_VERSION" >&2
  exit 5
fi
EXPECTED_PNPM_VERSION=$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).packageManager.split("@").at(-1))' "$ROOT_DIR/runtime/package.json")
ACTUAL_PNPM_VERSION=$(pnpm --version)
if [[ "$ACTUAL_PNPM_VERSION" != "$EXPECTED_PNPM_VERSION" ]]; then
  printf 'Expected pnpm %s, found %s.\n' "$EXPECTED_PNPM_VERSION" "$ACTUAL_PNPM_VERSION" >&2
  exit 5
fi
CARGO_METADATA=$(cargo metadata --manifest-path "$ROOT_DIR/Cargo.toml" --format-version 1 --no-deps)
CARGO_VERSION=$(printf '%s' "$CARGO_METADATA" | node -e '
let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", chunk => input += chunk);
process.stdin.on("end", () => {
  const metadata = JSON.parse(input);
  const plugin = metadata.packages.find(pkg => pkg.name === "dprint-plugin-oxfmt");
  if (!plugin) throw new Error("dprint-plugin-oxfmt package not found");
  console.log(plugin.version);
});
')
CARGO_TARGET_DIRECTORY=$(printf '%s' "$CARGO_METADATA" | node -e '
let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", chunk => input += chunk);
process.stdin.on("end", () => console.log(JSON.parse(input).target_directory));
')
if [[ "$RUNTIME_VERSION" != "$CARGO_VERSION" ]]; then
  printf 'Version mismatch: runtime=%s, Rust plugin=%s.\n' "$RUNTIME_VERSION" "$CARGO_VERSION" >&2
  exit 5
fi
VERSION="$CARGO_VERSION"

RELEASE_DIR="$ROOT_DIR/dist/releases"
BUNDLE_NAME="$PLUGIN_NAME-$VERSION-$HOST_TARGET"
ARCHIVE="$RELEASE_DIR/$BUNDLE_NAME.tar.gz"
PLATFORM_ZIP_NAME="$BUNDLE_NAME.zip"

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/dprint-plugin-oxfmt-package.XXXXXX")
trap 'rm -rf "$WORK_DIR"' EXIT
PAYLOAD_DIR="$WORK_DIR/payload"
BUNDLE_DIR="$WORK_DIR/bundle"
PLATFORM_ZIP="$BUNDLE_DIR/$PLATFORM_ZIP_NAME"
mkdir -p "$PAYLOAD_DIR/runtime/dist" "$BUNDLE_DIR" "$RELEASE_DIR"
rm -f "$ARCHIVE" "$ARCHIVE.sha256"

cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --release --target "$HOST_TARGET" -p dprint-plugin-oxfmt
pnpm --dir "$ROOT_DIR/runtime" install --frozen-lockfile
pnpm --dir "$ROOT_DIR/runtime" run build
cp "$ROOT_DIR/runtime/package.json" "$PAYLOAD_DIR/runtime/package.json"
cp "$ROOT_DIR/runtime/pnpm-lock.yaml" "$PAYLOAD_DIR/runtime/pnpm-lock.yaml"
pnpm --dir "$PAYLOAD_DIR/runtime" install --prod --frozen-lockfile --config.node-linker=hoisted

cp "$CARGO_TARGET_DIRECTORY/$HOST_TARGET/release/$EXECUTABLE_NAME" "$PAYLOAD_DIR/$EXECUTABLE_NAME"
cp "$ROOT_DIR/runtime/dist/worker.js" "$PAYLOAD_DIR/runtime/dist/worker.js"
cp "$ROOT_DIR/runtime/dist/protocol.js" "$PAYLOAD_DIR/runtime/dist/protocol.js"
if [[ -z "$EXECUTABLE_SUFFIX" ]]; then
  chmod +x "$PAYLOAD_DIR/$EXECUTABLE_NAME"
fi

(
  cd "$PAYLOAD_DIR"
  zip -q -r "$PLATFORM_ZIP" "$EXECUTABLE_NAME" runtime
)

sha256_file() {
  node -e '
const { createHash } = require("node:crypto");
const { createReadStream } = require("node:fs");
const stream = createReadStream(process.argv[1]);
const hash = createHash("sha256");
stream.on("data", chunk => hash.update(chunk));
stream.on("end", () => console.log(hash.digest("hex")));
' "$1"
}

ZIP_CHECKSUM=$(sha256_file "$PLATFORM_ZIP")
node --input-type=module - "$BUNDLE_DIR/plugin.json" "$PLUGIN_NAME" "$VERSION" "$DPRINT_PLATFORM" "$PLATFORM_ZIP_NAME" "$ZIP_CHECKSUM" <<'NODE'
import { writeFileSync } from "node:fs";

const [, , manifestPath, name, version, platform, reference, checksum] = process.argv;
const manifest = {
  schemaVersion: 2,
  kind: "process",
  name,
  version,
  [platform]: { reference, checksum },
};
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
NODE

tar -czf "$ARCHIVE" -C "$BUNDLE_DIR" plugin.json "$PLATFORM_ZIP_NAME"
ARCHIVE_CHECKSUM=$(sha256_file "$ARCHIVE")
printf '%s  %s\n' "$ARCHIVE_CHECKSUM" "$(basename "$ARCHIVE")" > "$ARCHIVE.sha256"
printf 'Created %s\n' "$ARCHIVE"
printf 'Checksum %s\n' "$ARCHIVE.sha256"
