#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if [[ $# -ne 1 ]]; then
  printf 'Usage: %s <release-tarball>\n' "$0" >&2
  exit 2
fi

ARCHIVE_DIR=$(cd "$(dirname "$1")" && pwd)
ARCHIVE="$ARCHIVE_DIR/$(basename "$1")"
if [[ ! -f "$ARCHIVE" ]]; then
  printf 'Release tarball not found: %s\n' "$ARCHIVE" >&2
  exit 3
fi
command -v unzip >/dev/null 2>&1 || {
  printf 'The unzip command is required to inspect the process-plugin artifact.\n' >&2
  exit 4
}

if [[ -z "${DPRINT_BIN:-}" ]]; then
  DPRINT_BIN=$(command -v dprint || true)
fi
if [[ -z "$DPRINT_BIN" || ! -x "$DPRINT_BIN" ]]; then
  printf 'dprint executable not found. Set DPRINT_BIN to dprint 0.55.1.\n' >&2
  exit 4
fi

run_dprint() {
  "$DPRINT_BIN" "$@"
}

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

EXPECTED_NODE_VERSION=$(tr -d '[:space:]' < "$ROOT_DIR/.node-version")
ACTUAL_NODE_VERSION=$(node --version)
if [[ "$ACTUAL_NODE_VERSION" != "v$EXPECTED_NODE_VERSION" ]]; then
  printf 'Expected system Node v%s, found %s.\n' "$EXPECTED_NODE_VERSION" "$ACTUAL_NODE_VERSION" >&2
  exit 5
fi
EXPECTED_DPRINT_VERSION=$(tr -d '[:space:]' < "$ROOT_DIR/.dprint-version")
ACTUAL_DPRINT_VERSION=$(run_dprint --version)
if [[ "$ACTUAL_DPRINT_VERSION" != "dprint $EXPECTED_DPRINT_VERSION" ]]; then
  printf 'Expected dprint %s, found %s.\n' "$EXPECTED_DPRINT_VERSION" "$ACTUAL_DPRINT_VERSION" >&2
  exit 6
fi

if [[ ! -f "$ARCHIVE.sha256" ]]; then
  printf 'Release tarball checksum not found: %s.\n' "$ARCHIVE.sha256" >&2
  exit 7
fi
read -r EXPECTED_ARCHIVE_CHECKSUM _ < "$ARCHIVE.sha256"
ACTUAL_ARCHIVE_CHECKSUM=$(sha256_file "$ARCHIVE")
if [[ "$ACTUAL_ARCHIVE_CHECKSUM" != "$EXPECTED_ARCHIVE_CHECKSUM" ]]; then
  printf 'Release tarball checksum mismatch.\n' >&2
  exit 7
fi

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/dprint-plugin-oxfmt-smoke.XXXXXX")
trap 'rm -rf "$WORK_DIR"' EXIT
tar -xzf "$ARCHIVE" -C "$WORK_DIR"
MANIFEST_PATH="$WORK_DIR/plugin.json"
if [[ ! -f "$MANIFEST_PATH" ]]; then
  printf 'plugin.json not found in release tarball.\n' >&2
  exit 8
fi

EXPECTED_VERSION=$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).version)' "$ROOT_DIR/runtime/package.json")
PLATFORM_DETAILS=$(node --input-type=module - "$MANIFEST_PATH" "$EXPECTED_VERSION" <<'NODE'
import { readFileSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";

const manifestPath = process.argv[2];
const expectedVersion = process.argv[3];
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
if (manifest.schemaVersion !== 2) throw new Error("schemaVersion must be 2");
if (manifest.kind !== "process") throw new Error("kind must be process");
if (manifest.name !== "dprint-plugin-oxfmt") throw new Error(`unexpected name: ${manifest.name}`);
if (manifest.version !== expectedVersion) throw new Error(`unexpected version: ${manifest.version}`);
const metadataKeys = new Set(["schemaVersion", "kind", "name", "version"]);
const platformKeys = Object.keys(manifest).filter(key => !metadataKeys.has(key));
if (platformKeys.length !== 1) {
  throw new Error(`expected one platform entry, found: ${platformKeys.join(", ")}`);
}
const platform = platformKeys[0];
const entry = manifest[platform];
if (!entry || typeof entry.reference !== "string" || typeof entry.checksum !== "string") {
  throw new Error(`invalid platform entry: ${platform}`);
}
if (!/^[a-f0-9]{64}$/.test(entry.checksum)) throw new Error("invalid platform checksum");
const artifactPath = resolve(dirname(manifestPath), entry.reference);
if (!statSync(artifactPath).isFile()) throw new Error(`artifact not found: ${artifactPath}`);
console.log(`${artifactPath}\t${entry.checksum}`);
NODE
)
IFS=$'\t' read -r PLATFORM_ZIP EXPECTED_ZIP_CHECKSUM <<< "$PLATFORM_DETAILS"
ACTUAL_ZIP_CHECKSUM=$(sha256_file "$PLATFORM_ZIP")
if [[ "$ACTUAL_ZIP_CHECKSUM" != "$EXPECTED_ZIP_CHECKSUM" ]]; then
  printf 'Platform ZIP checksum mismatch.\n' >&2
  exit 8
fi

EXPECTED_EXECUTABLE=$(node -e 'process.stdout.write("dprint-plugin-oxfmt" + (process.platform === "win32" ? ".exe" : ""))')
unzip -Z1 "$PLATFORM_ZIP" > "$WORK_DIR/platform-zip.contents"
grep -Fx "$EXPECTED_EXECUTABLE" "$WORK_DIR/platform-zip.contents" >/dev/null
grep -Fx 'runtime/dist/worker.js' "$WORK_DIR/platform-zip.contents" >/dev/null
grep -Fx 'runtime/dist/protocol.js' "$WORK_DIR/platform-zip.contents" >/dev/null
grep -Fx 'runtime/package.json' "$WORK_DIR/platform-zip.contents" >/dev/null

MANIFEST_CHECKSUM=$(sha256_file "$MANIFEST_PATH")
PLUGIN_REFERENCE="$MANIFEST_PATH@$MANIFEST_CHECKSUM"
SINGLE_QUOTE_CONFIG="$WORK_DIR/dprint.single-quote.json"
node --input-type=module - "$SINGLE_QUOTE_CONFIG" "$PLUGIN_REFERENCE" <<'NODE'
import { writeFileSync } from "node:fs";

const config = {
  plugins: [process.argv[3]],
  oxfmt: { singleQuote: true },
};
writeFileSync(process.argv[2], `${JSON.stringify(config, null, 2)}\n`);
NODE

export DPRINT_CACHE_DIR="$WORK_DIR/dprint-cache"
unset DPRINT_OXFMT_NODE DPRINT_OXFMT_WORKER
cd "$WORK_DIR"

assert_formatted() {
  local name=$1
  local input=$2
  local expected=$3
  shift 3
  local stdout_file="$WORK_DIR/$name.stdout"
  local stderr_file="$WORK_DIR/$name.stderr"

  if ! run_dprint fmt --config-discovery=false "$@" < "$input" > "$stdout_file" 2> "$stderr_file"; then
    printf '%s formatting failed.\n' "$name" >&2
    cat "$stderr_file" >&2
    exit 9
  fi
  if ! cmp -s "$stdout_file" "$expected"; then
    printf '%s output did not match the expected fixture.\n' "$name" >&2
    diff -u "$expected" "$stdout_file" >&2 || true
    exit 10
  fi
}

assert_formatted \
  typescript \
  "$ROOT_DIR/tests/fixtures/basic/typescript.input.ts" \
  "$ROOT_DIR/tests/fixtures/basic/typescript.expected.ts" \
  --plugins "$PLUGIN_REFERENCE" --stdin ts
assert_formatted \
  javascript \
  "$ROOT_DIR/tests/fixtures/basic/javascript.input.js" \
  "$ROOT_DIR/tests/fixtures/basic/javascript.expected.js" \
  --plugins "$PLUGIN_REFERENCE" --stdin js
assert_formatted \
  single-quote \
  "$ROOT_DIR/tests/fixtures/basic/single-quote.input.ts" \
  "$ROOT_DIR/tests/fixtures/basic/single-quote.expected.ts" \
  --config "$SINGLE_QUOTE_CONFIG" --stdin ts
assert_formatted \
  already-formatted \
  "$ROOT_DIR/tests/fixtures/basic/already-formatted.input.ts" \
  "$ROOT_DIR/tests/fixtures/basic/already-formatted.input.ts" \
  --plugins "$PLUGIN_REFERENCE" --stdin ts

set +e
run_dprint fmt \
  --config-discovery=false \
  --plugins "$PLUGIN_REFERENCE" \
  --stdin syntax-error.input.ts \
  < "$ROOT_DIR/tests/fixtures/errors/syntax-error.input.ts" \
  > "$WORK_DIR/syntax-error.stdout" \
  2> "$WORK_DIR/syntax-error.stderr"
SYNTAX_STATUS=$?
set -e
if [[ $SYNTAX_STATUS -eq 0 ]]; then
  printf 'syntax-error formatting unexpectedly succeeded.\n' >&2
  exit 11
fi
cat "$WORK_DIR/syntax-error.stdout" "$WORK_DIR/syntax-error.stderr" > "$WORK_DIR/syntax-error.output"
grep -F 'syntax-error.input.ts' "$WORK_DIR/syntax-error.output" >/dev/null
grep -F 'Unexpected token' "$WORK_DIR/syntax-error.output" >/dev/null

printf 'dprint CLI release smoke passed for %s.\n' "$(basename "$ARCHIVE")"
