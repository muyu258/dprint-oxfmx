# ADR 0002: Use the system Node runtime for the 0.1.0 package

- Status: Accepted
- Date: 2026-07-18
- Scope: 0.1.0 development and release artifacts

## Context

The formatter is implemented by the official `oxfmt` JavaScript package and may
load trusted project JavaScript configuration. The Rust plugin already has a
runtime locator boundary, while the repository currently pins Node 24.16.0 and
pnpm 10.30.3.

Bundling a Node executable would make installation more reproducible, but would
also require a platform-specific runtime distribution, additional security
update responsibility, larger artifacts, and a separate license/notice review.
Those concerns are not yet covered by the repository's release pipeline.

## Decision

For 0.1.0, release artifacts require a system Node executable compatible with
Node 24.16.0. The artifact includes the compiled worker, the pinned
`oxfmt@0.59.0` package, its production dependencies, and the corresponding
native binding selected by the package manager.

`RuntimeLocator` remains the only Rust entry point for finding Node and the
worker. A future bundled-Node package can add a bundled runtime at the locator's
reserved layout without changing the worker protocol or handler configuration.

## Consequences

- The installation documentation must state the Node prerequisite.
- Packaging and smoke tests must run without using the repository's development
  `node_modules` directory.
- A Node version mismatch must be reported when the runtime is started.
- A future switch to bundled Node requires a new ADR or an amendment and a
  platform artifact matrix; it must not be introduced implicitly by a release
  script.
