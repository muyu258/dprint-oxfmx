# dprint-plugin-oxfmt

`dprint-plugin-oxfmt` is a process-plugin bridge from dprint to the official
asynchronous `oxfmt.format()` JavaScript API.

The project uses one long-lived Node worker per dprint plugin session and targets
byte-for-byte single-file formatting parity with the pinned official Oxfmt
package.

## Requirements

Development and release packages use these exact tool/runtime versions:

- Node 24.16.0
- pnpm 10.30.3
- Oxfmt 0.59.0
- dprint CLI 0.55.1 for release smoke testing

Release packages use **system Node 24.16.0**. They include the worker and pinned
Oxfmt production dependencies, but they do not bundle a Node executable. Ensure
that `node` on `PATH` reports `v24.16.0` before running the plugin.

## Development

Install, build, and test the Node runtime:

```bash
pnpm --dir runtime install --frozen-lockfile
pnpm --dir runtime build
pnpm --dir runtime test
```

Build and test the Rust process plugin:

```bash
cargo build -p dprint-plugin-oxfmt
cargo fmt --all -- --check
cargo test --workspace
```

The real process-protocol parity tests are ignored by default because Cargo does
not build the sibling plugin binary or TypeScript runtime automatically. Run
them after building both artifacts:

```bash
pnpm --dir runtime build
cargo build -p dprint-plugin-oxfmt
cargo test -p dprint-plugin-oxfmt-integration-tests -- --ignored --nocapture
```

The parity tests use fixtures under `tests/fixtures/basic` and
`tests/fixtures/errors` and assert byte-for-byte output, including no-change and
diagnostic cases.

## Build a release package

The package script requires Rust, Node 24.16.0, pnpm 10.30.3, `tar`, and `zip`.
The release smoke additionally requires `unzip` and dprint 0.55.1. On Windows,
run the scripts from Git Bash with those archive tools available on `PATH`:

```bash
scripts/package.sh
```

Cross-compilation is intentionally not supported. The script builds for the
current Rust host and emits a manifest containing only that real host artifact;
it does not add placeholder entries for other platforms.

The generated outer bundle is written under the Git-ignored `dist/releases`
directory:

```text
dprint-plugin-oxfmt-0.1.0-<rust-host>.tar.gz
├── plugin.json
└── dprint-plugin-oxfmt-0.1.0-<rust-host>.zip
    ├── dprint-plugin-oxfmt[.exe]
    └── runtime/
        ├── package.json
        ├── pnpm-lock.yaml
        ├── dist/
        │   ├── worker.js
        │   └── protocol.js
        └── node_modules/
```

The outer tarball also has a `.tar.gz.sha256` sidecar. Verify and extract it,
for example:

```bash
archive=dist/releases/dprint-plugin-oxfmt-0.1.0-aarch64-apple-darwin.tar.gz
(
  cd "$(dirname "$archive")"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum --check "$(basename "$archive").sha256"
  else
    shasum -a 256 --check "$(basename "$archive").sha256"
  fi
)
mkdir -p .dprint-plugin-oxfmt
tar -xzf "$archive" -C .dprint-plugin-oxfmt
```

The tarball is a distribution wrapper. dprint reads `plugin.json`, validates the
selected platform ZIP checksum, extracts that ZIP into its plugin cache, and
starts the root `dprint-plugin-oxfmt` executable. `RuntimeLocator` then finds the
sibling `runtime/dist/worker.js` from that extracted artifact.

Run the same real-CLI package smoke used by release CI with:

```bash
scripts/smoke-dprint-cli.sh "$archive"
```

This requires dprint CLI 0.55.1 (the version pinned in `.dprint-version`). The
smoke validates the outer sidecar, the tar and platform ZIP path boundaries, the
single manifest platform against the current Rust host, and the ZIP checksum. It
also proves that incorrect manifest and platform checksums are rejected, loads the
packaged Oxfmt production dependency, and checks the executable's sibling
`runtime/dist` layout in an isolated dprint cache. Finally, it validates TypeScript,
JavaScript, `singleQuote`, unchanged input, and syntax-error behavior through the
unpacked release. Set `DPRINT_BIN` when that executable is not the `dprint` found
on `PATH`; release CI installs it in an isolated temporary directory rather than
using the repository development `node_modules`.

The Release workflow runs the same host-native package and smoke flow on
`ubuntu-latest`, `macos-latest`, and `windows-latest`. A manual run or `v*` tag
uploads one checksummed GitHub Actions artifact per runner, named with the runner
OS and architecture. It does not create a GitHub Release or combine the three
single-platform manifests.

## Configure dprint

A process-plugin manifest reference must include the SHA-256 of the exact
`plugin.json` bytes. Compute it after extracting the release:

```bash
manifest=.dprint-plugin-oxfmt/plugin.json
source scripts/release-common.sh
manifest_checksum=$(release_sha256_file "$manifest")
printf '%s\n' "$manifest_checksum"
```

Put that digest after `@` in `dprint.json`:

```json
{
  "plugins": [
    "./.dprint-plugin-oxfmt/plugin.json@<plugin-json-sha256>"
  ],
  "oxfmt": {
    "printWidth": 100,
    "singleQuote": true,
    "semi": true
  }
}
```

The plugin configuration is the `oxfmt` object. Its properties are passed to the
pinned official Oxfmt API without renaming, default injection, or Rust-side
interpretation. Use the options documented by the pinned `oxfmt` package.
dprint global formatting options are not implicitly mapped to Oxfmt options.
The plugin supports whole-file formatting for its declared JavaScript and
TypeScript extensions; range formatting is intentionally a no-op.

You can also provide the checksummed manifest directly on the command line:

```bash
printf 'const value={message:"hello"}\n' \
  | dprint fmt \
      --config-discovery=false \
      --plugins "./.dprint-plugin-oxfmt/plugin.json@$manifest_checksum" \
      --stdin ts
```

There are three distinct checksums in the release flow:

1. `plugin.json@<checksum>` uses the SHA-256 of the manifest bytes and is required
   by dprint for a process-plugin reference, including local manifests.
2. The selected platform entry inside `plugin.json` contains the SHA-256 of the
   referenced ZIP bytes; dprint validates it before extraction.
3. The optional `.tar.gz.sha256` sidecar validates the outer distribution bundle
   and is not passed to dprint.

## Current release limitations

- Node is a required system dependency and is not bundled.
- Each generated package and uploaded Actions artifact contains only the current
  runner host artifact.
- Release validation uploads Actions artifacts but does not create a GitHub
  Release.
- The project does not yet perform Cargo cross compilation or publish a combined
  multi-platform process-plugin manifest.

This process plugin is not sandboxed. Formatting may load and execute trusted
project Tailwind configuration, plugins, and other JavaScript-backed formatter
configuration. Use it only in repositories you trust.
