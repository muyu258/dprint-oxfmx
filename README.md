# dprint-plugin-oxfmt

`dprint-plugin-oxfmt` is a process-plugin bridge from dprint to the official
asynchronous `oxfmt.format()` JavaScript API.

The project is under active development. The initial implementation targets a
single long-lived Node worker per dprint plugin session and byte-for-byte
single-file formatting parity with the pinned official Oxfmt package.

## Security

This process plugin is not sandboxed. Formatting may load and execute trusted
project Tailwind configuration, plugins, and other JavaScript-backed formatter
configuration. Use it only in repositories you trust.

