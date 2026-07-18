# dprint-plugin-oxfmt

`dprint-plugin-oxfmt` is a process-plugin bridge from dprint to the official
asynchronous `oxfmt.format()` JavaScript API.

The project is under active development. The initial implementation targets a
single long-lived Node worker per dprint plugin session and byte-for-byte
single-file formatting parity with the pinned official Oxfmt package.

## Development

The runtime uses the pinned Node and pnpm versions declared in `runtime/package.json`.
Install its dependencies and run the runtime tests with:

```bash
pnpm --dir runtime install --frozen-lockfile
pnpm --dir runtime test
```

Run the Rust workspace tests and formatting check with:

```bash
cargo fmt --all -- --check
cargo test --workspace
```

The real dprint-to-Node parity tests are ignored by default because Cargo does
not build the sibling plugin binary or the TypeScript runtime automatically.
Run them after building both artifacts:

```bash
pnpm --dir runtime build
cargo build -p dprint-plugin-oxfmt
cargo test -p dprint-plugin-oxfmt-integration-tests -- --ignored --nocapture
```

The parity tests use the fixtures under `tests/fixtures/basic` and
`tests/fixtures/errors` and assert the byte-for-byte output of the pinned
Oxfmt version, including no-change and diagnostic cases.

## Security

This process plugin is not sandboxed. Formatting may load and execute trusted
project Tailwind configuration, plugins, and other JavaScript-backed formatter
configuration. Use it only in repositories you trust.

