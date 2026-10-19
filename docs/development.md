# Development

[README](../README.md) · [Playing btetri](usage.md) · [Engine and library](engine.md) · [Glossary](../GLOSSARY.md)

## Structure

| Module            | Responsibility                                                     |
| ----------------- | ------------------------------------------------------------------ |
| `game`            | Public deterministic engine; modes, pieces, scoring                |
| `app`             | Screen transitions and save/load lifecycle                         |
| `app/input`       | Held movement, DAS/ARR timing, and physical-key normalization      |
| `app/settings`    | Settings draft and typed editors                                   |
| `ui`              | Ratatui layout, gameplay, settings, and records rendering          |
| `config`          | Validated settings, key bindings, palette, database path           |
| `storage`         | SQLite schema, atomic settings writes, idempotent records, ranking |
| `cli` / `runtime` | Arguments, terminal setup/cleanup, input polling, game clock       |

The engine, application, storage, and shared UI tests live beside their modules in
`tests.rs`. Public engine items are checked by `deny(missing_docs)`; rustdoc examples
exercise coordinates, mode navigation, and finished-session behavior.

## Runtime constraints

| Area     | Contract                                                                                       |
| -------- | ---------------------------------------------------------------------------------------------- |
| Build    | CLI requires a C compiler for bundled SQLite; engine-only builds omit it                       |
| Clock    | Synchronous input polling with a maximum 16 ms wait; only the playing screen advances the game |
| Storage  | Synchronous operations can delay input on menu, settings, result, and records screens          |
| Shutdown | Process pending writes; report unsaved settings/results alongside terminal errors              |
| Terminal | Scope guards restore terminal state on return or panic unwinding                               |
| Schema   | `src/schema.sql`; compatibility and data-loss behavior: [stored data](usage.md#stored-data)    |

## Verification

```sh
cargo +nightly fmt --check
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --no-default-features
cargo clippy --no-default-features --all-targets -- -D warnings
cargo doc --all-features --no-deps
```

| Tool                  | Coverage                                                          |
| --------------------- | ----------------------------------------------------------------- |
| `rstest`              | Rules, input cases, screen states, schema variants                |
| `proptest`            | Determinism, bag/board invariants, settings/key/color round trips |
| Ratatui `TestBackend` | Layout, cursor, hints, and error visibility                       |
| `tempfile` + SQLite   | Reopen, rollback, shared connections, locking, retries            |
