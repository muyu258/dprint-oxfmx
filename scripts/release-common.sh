#!/usr/bin/env bash

release_require_command() {
  local command_name=$1
  local purpose=${2:-release processing}

  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'The %s command is required for %s.\n' "$command_name" "$purpose" >&2
    return 1
  fi
}

release_host_target() {
  local line

  while IFS= read -r line; do
    if [[ "$line" == host:* ]]; then
      printf '%s\n' "${line#host: }"
      return 0
    fi
  done < <(rustc -vV)

  printf 'Could not determine the Rust host target.\n' >&2
  return 1
}

release_dprint_platform_for_target() {
  case "$1" in
    aarch64-apple-darwin)
      printf 'darwin-aarch64\n'
      ;;
    x86_64-apple-darwin)
      printf 'darwin-x86_64\n'
      ;;
    aarch64-unknown-linux-gnu)
      printf 'linux-aarch64\n'
      ;;
    x86_64-unknown-linux-gnu)
      printf 'linux-x86_64\n'
      ;;
    aarch64-unknown-linux-musl)
      printf 'linux-aarch64-musl\n'
      ;;
    x86_64-unknown-linux-musl)
      printf 'linux-x86_64-musl\n'
      ;;
    aarch64-pc-windows-msvc)
      printf 'windows-aarch64\n'
      ;;
    x86_64-pc-windows-msvc)
      printf 'windows-x86_64\n'
      ;;
    *)
      printf 'Unsupported dprint process-plugin host target: %s.\n' "$1" >&2
      return 1
      ;;
  esac
}

release_executable_suffix_for_target() {
  if [[ "$1" == *-pc-windows-msvc ]]; then
    printf '.exe\n'
  else
    printf '\n'
  fi
}

release_runtime_version() {
  node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).version)' "$1"
}

release_archive_path_is_safe() {
  local path=$1

  case "$path" in
    /* | [A-Za-z]:* | *\\* | .. | ../* | */../* | */..)
      return 1
      ;;
  esac
}

release_validate_archive_paths() {
  local path

  while IFS= read -r path; do
    if ! release_archive_path_is_safe "$path"; then
      printf 'Archive contains an unsafe path: %s\n' "$path" >&2
      return 1
    fi
  done < "$1"
}

release_sha256_file() {
  node -e '
const { createHash } = require("node:crypto");
const { createReadStream } = require("node:fs");
const stream = createReadStream(process.argv[1]);
const hash = createHash("sha256");
stream.on("data", chunk => hash.update(chunk));
stream.on("end", () => console.log(hash.digest("hex")));
' "$1"
}
