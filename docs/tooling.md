# Strict tooling

Every check is enforced by CI. Source and examples use concrete types, runtime
validation at foreign-data boundaries, and no unchecked type assertions or
unconstrained dynamic types.

## Rust

`rust-toolchain.toml` pins Rust 1.99.1 with rustfmt and Clippy. Formatting uses the
checked-in stable rustfmt configuration. `cargo fmt --all -- --check` fails for
format drift.

Workspace lint policy denies Clippy's `all`, `pedantic`, and `nursery` groups.
Additional restriction lints deny unchecked casts, direct indexing/slicing,
`unwrap`, `expect`, `panic`, unfinished code, debug macros, and stdout/stderr
printing. Rust forbids unsafe source and denies missing public documentation
and ignored results. CI invokes Clippy with `-D warnings` across all targets and
features. Checked lint exceptions cover public constructors with many keyword arguments.

## JavaScript

Oxlint and Oxfmt are pinned in `package-lock.json`. Oxlint enables every category
as an error, including restriction and nursery rules, with TypeScript, Unicorn,
Oxc, import, promise, and Node plugins. Both type-aware linting and compiler type
diagnostics run through `oxlint --type-aware --type-check`; warnings and unused
disable directives fail the check. Oxfmt checks formatting and import ordering.

TypeScript enables `strict`, exact optional properties, unchecked-index checks,
index-signature access checks, explicit overrides, unused declarations,
side-effect import checks, isolated modules/declarations, and declaration
emission. Dependency declarations are checked with `skipLibCheck: false`.
Type assertions, explicit dynamic escape types, and non-null assertions are
forbidden.

Some all-category rules express conflicting styles or ban features required by
the contract. These exceptions are explicit in `.oxlintrc.json`:

| Rule family | Reason |
| --- | --- |
| Default-only/grouped/last exports | Public named exports keep documentation beside each declaration. |
| Node modules and synchronous I/O | Build scripts use Node filesystem operations; the generation API is synchronous. |
| Null/undefined bans | JSON requires null; optional TypeScript fields use absence. |
| Readonly parameter enforcement | JSON payloads and isolated provider options are intentionally caller-mutable. Public controls and case fields are readonly. |
| Ternary, object-key order, rest/spread bans | Conditional expressions, portable schema key order, and typed option composition remain available. |
| Core undefined-reference rule | TypeScript supplies the authoritative reference/type check. |
| Import declaration ordering | Oxfmt owns ordering; named imports are still checked. |
| Common numeric literals | Only -1, 0, 1, and array indexes are exempt from magic-number checks. |
| Runtime declarations | Module loading and ambient declarations have narrow import-style exceptions. |
| Related error classes | The public error hierarchy shares one module. |
| HTTP examples/tests | Async HTTP and test registration are allowed; tests run request sequences deliberately and use literal independent expected outcomes. |

The one inline structured-clone exception checks model JSON serialization,
including `toJSON` behavior. It is checked for continued necessity.

## Python

uv manages dependencies. Ruff enables all rules, with documented style/test
exceptions, and ty treats every diagnostic as an error. Type definitions retain
docstrings for editor documentation. Fixture payloads and typed model views are
validated before successful results are returned.
