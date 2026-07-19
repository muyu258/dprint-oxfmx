#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
# shellcheck source=release-common.sh
source "$ROOT_DIR/scripts/release-common.sh"

if [[ $# -ne 1 ]]; then
  printf 'Usage: %s <release-tarball>\n' "$0" >&2
  exit 2
fi

for command_name in rustc node tar unzip mktemp cp cmp diff grep tr; do
  release_require_command "$command_name" "release smoke testing" || exit 4
done

HOST_TARGET=$(release_host_target) || exit 3
DPRINT_PLATFORM=$(release_dprint_platform_for_target "$HOST_TARGET") || exit 3
EXECUTABLE_SUFFIX=$(release_executable_suffix_for_target "$HOST_TARGET")
PLUGIN_NAME="dprint-plugin-oxfmt"
EXPECTED_VERSION=$(release_runtime_version "$ROOT_DIR/runtime/package.json")
BUNDLE_NAME="$PLUGIN_NAME-$EXPECTED_VERSION-$HOST_TARGET"
EXPECTED_ARCHIVE_BASENAME="$BUNDLE_NAME.tar.gz"
EXPECTED_PLATFORM_ZIP_NAME="$BUNDLE_NAME.zip"
EXPECTED_EXECUTABLE="$PLUGIN_NAME$EXECUTABLE_SUFFIX"
REQUIRED_RUNTIME_ENTRIES=(
  runtime/package.json
  runtime/pnpm-lock.yaml
  runtime/dist/worker.js
  runtime/dist/protocol.js
  runtime/node_modules/oxfmt/package.json
)

ARCHIVE_DIR=$(cd "$(dirname "$1")" && pwd)
ARCHIVE="$ARCHIVE_DIR/$(basename "$1")"
if [[ ! -f "$ARCHIVE" ]]; then
  printf 'Release tarball not found: %s\n' "$ARCHIVE" >&2
  exit 3
fi
if [[ "$(basename "$ARCHIVE")" != "$EXPECTED_ARCHIVE_BASENAME" ]]; then
  printf 'Expected release tarball %s for host %s, found %s.\n' \
    "$EXPECTED_ARCHIVE_BASENAME" "$HOST_TARGET" "$(basename "$ARCHIVE")" >&2
  exit 3
fi

if [[ -z "${DPRINT_BIN:-}" ]]; then
  DPRINT_BIN=$(command -v dprint || true)
elif [[ "$DPRINT_BIN" != */* ]]; then
  DPRINT_BIN=$(command -v "$DPRINT_BIN" || true)
fi
if [[ -z "$DPRINT_BIN" || ! -f "$DPRINT_BIN" ]]; then
  printf 'dprint executable not found. Set DPRINT_BIN to dprint 0.55.1.\n' >&2
  exit 4
fi
if [[ "$DPRINT_BIN" == */* ]]; then
  DPRINT_BIN_DIR=$(cd "$(dirname "$DPRINT_BIN")" && pwd)
  DPRINT_BIN="$DPRINT_BIN_DIR/$(basename "$DPRINT_BIN")"
fi

run_dprint() {
  "$DPRINT_BIN" "$@"
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
EXPECTED_ARCHIVE_CHECKSUM=$(node --input-type=module - "$ARCHIVE.sha256" "$EXPECTED_ARCHIVE_BASENAME" <<'NODE'
import { readFileSync } from "node:fs";

const sidecarPath = process.argv[2];
const expectedBasename = process.argv[3];
const contents = readFileSync(sidecarPath, "utf8");
const match = /^([a-f0-9]{64})  ([^\r\n]+)\r?\n?$/.exec(contents);
if (!match) throw new Error("invalid release checksum sidecar");
if (match[2] !== expectedBasename) {
  throw new Error(`checksum sidecar references ${match[2]}, expected ${expectedBasename}`);
}
console.log(match[1]);
NODE
)
ACTUAL_ARCHIVE_CHECKSUM=$(release_sha256_file "$ARCHIVE")
if [[ "$ACTUAL_ARCHIVE_CHECKSUM" != "$EXPECTED_ARCHIVE_CHECKSUM" ]]; then
  printf 'Release tarball checksum mismatch.\n' >&2
  exit 7
fi

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/dprint-plugin-oxfmt-smoke.XXXXXX")
trap 'rm -rf "$WORK_DIR"' EXIT

tar -tzf "$ARCHIVE" > "$WORK_DIR/archive.contents"
tar -tvzf "$ARCHIVE" > "$WORK_DIR/archive.details"
release_validate_archive_paths "$WORK_DIR/archive.contents"
node --input-type=module - \
  "$WORK_DIR/archive.contents" \
  "$WORK_DIR/archive.details" \
  "$EXPECTED_PLATFORM_ZIP_NAME" <<'NODE'
import { readFileSync } from "node:fs";

const names = readFileSync(process.argv[2], "utf8").split(/\r?\n/).filter(Boolean);
const details = readFileSync(process.argv[3], "utf8").split(/\r?\n/).filter(Boolean);
const expected = new Set(["plugin.json", process.argv[4]]);
if (names.length !== 2 || new Set(names).size !== names.length) {
  throw new Error(`release tarball must contain exactly two unique entries, found: ${names.join(", ")}`);
}
if (names.some(name => !expected.has(name))) {
  throw new Error(`release tarball contains an unexpected entry: ${names.join(", ")}`);
}
if (details.length !== 2 || details.some(line => !line.startsWith("-"))) {
  throw new Error("release tarball entries must both be regular files");
}
NODE

tar -xzf "$ARCHIVE" -C "$WORK_DIR"
MANIFEST_PATH="$WORK_DIR/plugin.json"
if [[ ! -f "$MANIFEST_PATH" ]]; then
  printf 'plugin.json not found in release tarball.\n' >&2
  exit 8
fi

PLATFORM_DETAILS=$(node --input-type=module - \
  "$MANIFEST_PATH" \
  "$EXPECTED_VERSION" \
  "$DPRINT_PLATFORM" \
  "$EXPECTED_PLATFORM_ZIP_NAME" <<'NODE'
import { readFileSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";

const manifestPath = process.argv[2];
const expectedVersion = process.argv[3];
const expectedPlatform = process.argv[4];
const expectedReference = process.argv[5];
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
if (manifest.schemaVersion !== 2) throw new Error("schemaVersion must be 2");
if (manifest.kind !== "process") throw new Error("kind must be process");
if (manifest.name !== "dprint-plugin-oxfmt") throw new Error(`unexpected name: ${manifest.name}`);
if (manifest.version !== expectedVersion) throw new Error(`unexpected version: ${manifest.version}`);
const metadataKeys = new Set(["schemaVersion", "kind", "name", "version"]);
const platformKeys = Object.keys(manifest).filter(key => !metadataKeys.has(key));
if (platformKeys.length !== 1 || platformKeys[0] !== expectedPlatform) {
  throw new Error(`expected platform ${expectedPlatform}, found: ${platformKeys.join(", ")}`);
}
const entry = manifest[expectedPlatform];
if (!entry || typeof entry.reference !== "string" || typeof entry.checksum !== "string") {
  throw new Error(`invalid platform entry: ${expectedPlatform}`);
}
if (entry.reference !== expectedReference) {
  throw new Error(`expected platform reference ${expectedReference}, found ${entry.reference}`);
}
if (!/^[a-f0-9]{64}$/.test(entry.checksum)) throw new Error("invalid platform checksum");
const artifactPath = resolve(dirname(manifestPath), entry.reference);
if (!statSync(artifactPath).isFile()) throw new Error(`artifact not found: ${artifactPath}`);
console.log(`${artifactPath}\t${entry.checksum}`);
NODE
)
IFS=$'\t' read -r PLATFORM_ZIP EXPECTED_ZIP_CHECKSUM <<< "$PLATFORM_DETAILS"
ACTUAL_ZIP_CHECKSUM=$(release_sha256_file "$PLATFORM_ZIP")
if [[ "$ACTUAL_ZIP_CHECKSUM" != "$EXPECTED_ZIP_CHECKSUM" ]]; then
  printf 'Platform ZIP checksum mismatch.\n' >&2
  exit 8
fi

unzip -Z1 "$PLATFORM_ZIP" > "$WORK_DIR/platform-zip.contents"
release_validate_archive_paths "$WORK_DIR/platform-zip.contents"
for required in "${REQUIRED_RUNTIME_ENTRIES[@]}"; do
  grep -Fx "$required" "$WORK_DIR/platform-zip.contents" >/dev/null || {
    printf 'platform ZIP is missing %s.\n' "$required" >&2
    exit 8
  }
done
node --input-type=module - \
  "$WORK_DIR/platform-zip.contents" \
  "$EXPECTED_EXECUTABLE" <<'NODE'
import { readFileSync } from "node:fs";

const entries = readFileSync(process.argv[2], "utf8").split(/\r?\n/).filter(Boolean);
const executable = process.argv[3];
if (entries.length === 0 || new Set(entries).size !== entries.length) {
  throw new Error("platform ZIP must contain unique entries");
}
for (const name of entries) {
  if (name !== executable && !name.startsWith("runtime/")) {
    throw new Error(`platform ZIP contains an unexpected top-level entry: ${name}`);
  }
  if (name.startsWith("runtime/src/") || /^runtime\/dist\/.*\.test\./.test(name)) {
    throw new Error(`platform ZIP contains development-only content: ${name}`);
  }
}
NODE

PAYLOAD_DIR="$WORK_DIR/platform-payload"
mkdir -p "$PAYLOAD_DIR"
unzip -q "$PLATFORM_ZIP" -d "$PAYLOAD_DIR"
(
  cd "$PAYLOAD_DIR/runtime"
  node --input-type=module -e '
const oxfmt = await import("oxfmt");
if (typeof oxfmt.format !== "function") throw new Error("oxfmt.format is not available");
'
)

MANIFEST_CHECKSUM=$(release_sha256_file "$MANIFEST_PATH")
PLUGIN_REFERENCE="$MANIFEST_PATH@$MANIFEST_CHECKSUM"
unset DPRINT_OXFMT_NODE DPRINT_OXFMT_WORKER

assert_checksum_rejected() {
  local name=$1
  local reference=$2
  local cache_dir=$3
  local stdout_file="$WORK_DIR/$name.stdout"
  local stderr_file="$WORK_DIR/$name.stderr"

  set +e
  DPRINT_CACHE_DIR="$cache_dir" run_dprint fmt \
    --config-discovery=false \
    --plugins "$reference" \
    --stdin ts \
    < "$ROOT_DIR/tests/fixtures/basic/typescript.input.ts" \
    > "$stdout_file" \
    2> "$stderr_file"
  local status=$?
  set -e

  if [[ $status -ne 12 ]]; then
    printf '%s checksum validation exited with %s instead of 12.\n' "$name" "$status" >&2
    cat "$stdout_file" "$stderr_file" >&2
    exit 9
  fi
  cat "$stdout_file" "$stderr_file" > "$WORK_DIR/$name.output"
  if ! grep -Eiq 'checksum|hash' "$WORK_DIR/$name.output"; then
    printf '%s checksum validation did not report a checksum diagnostic.\n' "$name" >&2
    cat "$WORK_DIR/$name.output" >&2
    exit 9
  fi
}

BAD_MANIFEST_CHECKSUM=$(printf '%064d' 0)
assert_checksum_rejected \
  manifest-checksum \
  "$MANIFEST_PATH@$BAD_MANIFEST_CHECKSUM" \
  "$WORK_DIR/dprint-cache-bad-manifest"

TAMPERED_DIR="$WORK_DIR/tampered-platform"
mkdir -p "$TAMPERED_DIR"
cp "$MANIFEST_PATH" "$TAMPERED_DIR/plugin.json"
cp "$PLATFORM_ZIP" "$TAMPERED_DIR/$EXPECTED_PLATFORM_ZIP_NAME"
printf 'tampered\n' >> "$TAMPERED_DIR/$EXPECTED_PLATFORM_ZIP_NAME"
assert_checksum_rejected \
  platform-checksum \
  "$TAMPERED_DIR/plugin.json@$MANIFEST_CHECKSUM" \
  "$WORK_DIR/dprint-cache-bad-platform"

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
    exit 10
  fi
  if ! cmp -s "$stdout_file" "$expected"; then
    printf '%s output did not match the expected fixture.\n' "$name" >&2
    diff -u "$expected" "$stdout_file" >&2 || true
    exit 11
  fi
}

assert_cached_runtime_layout() {
  node --input-type=module - \
    "$DPRINT_CACHE_DIR" \
    "$EXPECTED_EXECUTABLE" \
    "${REQUIRED_RUNTIME_ENTRIES[@]}" <<'NODE'
import { existsSync, readdirSync } from "node:fs";
import { basename, dirname, join } from "node:path";

const cacheDir = process.argv[2];
const expectedExecutable = process.argv[3];
const requiredSiblings = process.argv.slice(4);
const candidates = [];
const pending = [cacheDir];
while (pending.length > 0) {
  const directory = pending.pop();
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) pending.push(path);
    else if (entry.isFile() && basename(path) === expectedExecutable) candidates.push(path);
  }
}
const valid = candidates.filter(candidate => {
  const parent = dirname(candidate);
  return requiredSiblings.every(relative => existsSync(join(parent, relative)));
});
if (valid.length === 0) {
  throw new Error(`no cached ${expectedExecutable} has the required sibling runtime layout`);
}
NODE
}

assert_formatted \
  typescript \
  "$ROOT_DIR/tests/fixtures/basic/typescript.input.ts" \
  "$ROOT_DIR/tests/fixtures/basic/typescript.expected.ts" \
  --plugins "$PLUGIN_REFERENCE" --stdin ts
assert_cached_runtime_layout
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
  exit 12
fi
cat "$WORK_DIR/syntax-error.stdout" "$WORK_DIR/syntax-error.stderr" > "$WORK_DIR/syntax-error.output"
grep -F 'syntax-error.input.ts' "$WORK_DIR/syntax-error.output" >/dev/null
grep -F 'Unexpected token' "$WORK_DIR/syntax-error.output" >/dev/null

printf 'dprint CLI release smoke passed for %s.\n' "$(basename "$ARCHIVE")"
