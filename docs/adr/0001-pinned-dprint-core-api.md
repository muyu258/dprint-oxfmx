# ADR 0001: Follow the pinned dprint-core process-plugin API

- Status: Accepted
- Date: 2026-07-18
- Pinned dependency: `dprint-core` 0.68.2

## Context

The upstream process-plugin development guide is the authoritative starting
point for schema-version-5 plugins. Its `PluginInfo` example currently assigns
`Some(None)` to `update_url`.

In `dprint-core` 0.68.2, `PluginInfo::update_url` has the type `Option<String>`,
so `Some(None)` does not compile. The pinned source also names the handler's
configuration result `PluginResolveConfigurationResult`.

## Decision

Follow the pinned `dprint-core` source when its API differs from the prose or
examples in the development guide. Until a real update endpoint exists, the
plugin sets `update_url` to `None` and returns
`PluginResolveConfigurationResult` from `resolve_config()`.

## Consequences

Upgrading `dprint-core` requires checking both the upstream guide and the
pinned crate source again. Any new discrepancy affecting protocol, lifecycle,
or packaging must be documented before the upgrade is merged.

