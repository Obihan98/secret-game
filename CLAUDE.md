# CLAUDE.md

Persistent, real-time Napoleonic grand strategy game (1800–1830). Server-authoritative: a Rust simulation library + server, and a Godot 4.7 (2D, PC-only) client that only renders and sends orders.
Design lives in `docs/`: read `docs/README.md` (rules) and `docs/DECISIONS.md` (status board, tensions) first. Do not `@`-import docs here; open the one domain file you need.

## Working rules
- IMPORTANT: build only what `docs/` has decided (🟡 or ✅). If a topic is ⬜ or sits in a Tension, ask before coding around it.
- A design decision goes into the domain file first, then the status board. IDs (e.g. `TM2`) are stable and never renumbered.
- IMPORTANT: follow `docs/tech/build-order.md`. One stage at a time; a stage is done only when its gate is green (`scripts/check.ps1`) and the user signs off. Never start the next stage, or add later-stage features, early.
- Never commit, push, install software or download files unless asked. Ask first, naming file, source and size.
- Prefer the smallest change that solves the task. No speculative abstractions, no unrequested refactors.
- Comments explain *why*, not *what*. Match the surrounding code's style and density.

## Architecture (from TS3–TS6, TM2, TM9)
- `sim` crate: pure library, NO Godot, NO I/O, NO wall clock, NO threads. The server links it; the client may load it via GDExtension for previews only, never as authority.
- State = flat tables (`Vec`) indexed by typed IDs (`ProvinceId(u32)` newtypes). No ECS crate, no `Rc<RefCell<>>` graphs.
- All state changes happen by running events from one queue ordered by `(game-second, priority, sequence)`, priority from a fixed table per event kind. Systems read state and emit events; they never call each other.
- The log records INPUTS only (orders, admin actions, RNG seeds) plus a state hash every game-day; replay re-derives everything else. Each game update = snapshot + ruleset version.
- One thread per world; scale by running many worlds. SQLite file per world (WAL, `synchronous=FULL`), format-versioned. Ack and broadcast to clients only after the covering flush.
- The client holds no game formulas: it only extrapolates server-sent parameters. Previews are server queries.
- Idle worlds hibernate lazily (clock keeps running logically). Pauses are capped (2 h each, 8 h/week) and public.

## Rust rules
- IMPORTANT, determinism in `sim`: time is `i64` game-seconds; state math is integer or fixed-point (never `f32`/`f64`); no `HashMap`/`HashSet` iteration (use `Vec`, `BTreeMap`, or sort by ID); one seeded RNG stream per system, drawn in fixed order; no `Instant::now()`/`SystemTime`.
- `#![forbid(unsafe_code)]` in `sim`. No `unwrap`/`expect`/`panic!` outside tests; return typed errors (`thiserror` in libs, `anyhow` only in binaries).
- Cargo workspace, shared `[workspace.lints]` (clippy `pedantic` warn, `unwrap_used` deny), edition 2024, toolchain pinned in `rust-toolchain.toml`.
- Hot paths: no allocation in per-event loops, prefer indices over references, iterate contiguous data. Measure (criterion benches) before optimizing; budgets live in TS13.
- Every sim feature needs a determinism test: same seed + same orders = identical state hash, and snapshot + log replay = live state.
- Before finishing: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.

## Godot / GDScript rules
- Typed GDScript everywhere (typed vars, params, returns; `:=` when obvious). Warnings for untyped code are errors.
- Official style guide: tabs, LF, UTF-8, lines <100 chars, `snake_case` files/functions/vars/signals, `PascalCase` classes and nodes, `CONSTANT_CASE` consts, past-tense signals (`door_opened`), `_private` prefix.
- Code order: `class_name`, `extends`, signals, enums, consts, `@export`, vars, `@onready`, then callbacks, then methods.
- The client holds NO game rules. Call down, signal up. Autoloads only for small global services. Prefer unique scene names (`%Node`) over long `get_node()` paths.
- Keep `_process` light; no per-frame allocations or tree searches. Use a shader/lookup for province coloring, not one node per province.
- Run headless checks with `godot --headless --path . --quit` (the `godot` shim is on PATH).

## Folder layout (decided, TS15; `project.godot` still at the root until stage 0 moves it to `client/`)
```
docs/            design docs (add an empty .gdignore if the Godot project root contains it)
crates/sim/      pure simulation library
crates/server/   world host, persistence, networking
crates/protocol/ order/event types shared by server and client bridge
crates/gdext/    only if the client ever loads the sim (not in v1)
scripts/         check.ps1 (fmt, clippy, tests, godot headless)
client/          Godot project; features grouped together: client/map/, client/ui/, client/net/ (scene + script + assets side by side)
client/addons/   third-party code only
```
Files and folders are lowercase `snake_case` (exports are case-sensitive on Linux).

## Environment (Windows 11)
- Shells: PowerShell and Git Bash. Installed: Godot 4.7 (`godot` on PATH), Rust stable (rustup, clippy, rustfmt), MSVC Build Tools, git, gh, Node, Python. New terminals may be needed to see `~/.cargo/bin`.
