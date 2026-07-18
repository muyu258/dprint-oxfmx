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
TARGET="$HOST_TARGET"
VERSION=$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).version)' "$ROOT_DIR/runtime/package.json")
RELEASE_DIR="$ROOT_DIR/dist/releases"
STAGE_DIR="$RELEASE_DIR/dprint-plugin-oxfmt-$VERSION-$TARGET"
ARCHIVE="$RELEASE_DIR/dprint-plugin-oxfmt-$VERSION-$TARGET.tar.gz"

rm -rf "$STAGE_DIR" "$ARCHIVE" "$ARCHIVE.sha256"
mkdir -p "$STAGE_DIR/runtime/dist" "$RELEASE_DIR"

cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --release -p dprint-plugin-oxfmt
pnpm --dir "$ROOT_DIR/runtime" install --frozen-lockfile
pnpm --dir "$ROOT_DIR/runtime" run build
cp "$ROOT_DIR/runtime/package.json" "$STAGE_DIR/runtime/package.json"
cp "$ROOT_DIR/runtime/pnpm-lock.yaml" "$STAGE_DIR/runtime/pnpm-lock.yaml"
pnpm --dir "$STAGE_DIR/runtime" install --prod --frozen-lockfile

cp "$ROOT_DIR/target/release/dprint-plugin-oxfmt" "$STAGE_DIR/dprint-plugin-oxfmt"
cp "$ROOT_DIR/runtime/dist/worker.js" "$STAGE_DIR/runtime/dist/worker.js"
cp "$ROOT_DIR/runtime/dist/protocol.js" "$STAGE_DIR/runtime/dist/protocol.js"
chmod +x "$STAGE_DIR/dprint-plugin-oxfmt"

node --input-type=module - "$STAGE_DIR" "$VERSION" "$TARGET" <<'NODE'
import { writeFileSync } from "node:fs";

const [, , stageDir, version, target] = process.argv;
const manifest = {
  schemaVersion: 2,
  name: "dprint-plugin-oxfmt",
  version,
  platform: target,
  node: "system",
  nodeVersion: "24.16.0",
  oxfmtVersion: "0.59.0",
  executable: "dprint-plugin-oxfmt",
  worker: "runtime/dist/worker.js",
};
writeFileSync(`${stageDir}/plugin-manifest.json`, `${JSON.stringify(manifest, null, 2)}\n`);
NODE

tar -czf "$ARCHIVE" -C "$STAGE_DIR" .
shasum -a 256 "$ARCHIVE" > "$ARCHIVE.sha256"
printf 'Created %s\n' "$ARCHIVE"
printf 'Checksum %s\n' "$ARCHIVE.sha256"
