#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TARGET=${TARGET:-$(rustc -vV | awk '/^host:/ { print $2 }')}
VERSION=$(node -e 'console.log(JSON.parse(require("fs").readFileSync("runtime/package.json", "utf8")).version)')
RELEASE_DIR="$ROOT_DIR/dist/releases"
STAGE_DIR="$RELEASE_DIR/dprint-plugin-oxfmt-$VERSION-$TARGET"
ARCHIVE="$RELEASE_DIR/dprint-plugin-oxfmt-$VERSION-$TARGET.tar.gz"

rm -rf "$STAGE_DIR" "$ARCHIVE" "$ARCHIVE.sha256"
mkdir -p "$STAGE_DIR/runtime" "$RELEASE_DIR"

cargo build --release -p dprint-plugin-oxfmt
pnpm --dir runtime install --frozen-lockfile
pnpm --dir runtime run build
cp "$ROOT_DIR/runtime/package.json" "$STAGE_DIR/runtime/package.json"
cp "$ROOT_DIR/runtime/pnpm-lock.yaml" "$STAGE_DIR/runtime/pnpm-lock.yaml"
pnpm --dir "$STAGE_DIR/runtime" install --prod --frozen-lockfile

cp "$ROOT_DIR/target/release/dprint-plugin-oxfmt" "$STAGE_DIR/dprint-plugin-oxfmt"
cp "$ROOT_DIR/runtime/dist/worker.js" "$STAGE_DIR/runtime/worker.js"
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
  worker: "runtime/worker.js",
};
writeFileSync(`${stageDir}/plugin-manifest.json`, `${JSON.stringify(manifest, null, 2)}\n`);
NODE

tar -czf "$ARCHIVE" -C "$STAGE_DIR" .
shasum -a 256 "$ARCHIVE" > "$ARCHIVE.sha256"
printf 'Created %s\n' "$ARCHIVE"
printf 'Checksum %s\n' "$ARCHIVE.sha256"
