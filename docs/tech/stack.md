# Tech Stack (TS)

> Scope: engine, languages, architecture, servers, storage, tools, workflow.
> Depends on: TM2, WD1, WD2, MP2, UI1, PL4 · Feeds into: everything that gets built.

## Decisions

### TS1. Engine ✅
- **Decision: Godot 4.7 stable** (`4.7.stable.official`), 2D only. Project created with the GL Compatibility renderer.
- Open details: renderer choice (TS9), drop the default 3D Jolt physics setting (unused in 2D).
- Cross-checks: TS2, TS3, TS9

### TS2. Target platforms ✅
- **Decision: PC only** (Windows, Mac, Linux). Same decision as UI1.
- Browser is out, so C# and native GDExtension stay allowed for the client, and the transport is not limited to WebSocket. TS3 has since chosen Rust for the sim and GDScript for the client (C# was the alternative considered); the transport is still open for TS7.
- A mobile companion for notifications may come later; it is server-driven and does not constrain the client.
- Cross-checks: UI1, TS3, TS7

### TS4. Client/server architecture ✅
- **Decision: server-authoritative, with the simulation as an engine-independent library.**
  - A standalone server program runs the sim (one process per world), owns persistence and the event log, and decides what each client may know (fog of war, MP9).
  - The Godot PC client is a viewer: it sends orders and renders the state the server sends. It holds no authoritative rules.
  - The sim library has no Godot dependency. In v1 the client does not load it (see TS3); if it ever does (GDExtension, for instant previews), never as the authority.
- The language of the sim library is not chosen here; that is TS3.
- Who may host or administer a world (PL4) is still open; WD1 already says we host.
- Cross-checks: WD1, TM2, TM9, TS3, TS5, TS6, TS16, PL4, MP9

### TS3. Language & performance strategy ✅
- **Decision: Rust for the simulation library and the server; typed GDScript for the Godot client (UI, rendering, input).**
  - The sim is one Rust crate with no Godot dependency (TS4). The server binary links it.
  - v1: the client does not load the crate. It extrapolates server-sent parameters only, and previews such as march times are server queries (TM2). GDExtension (godot-rust, supports Godot 4.7) stays an option if previews must be instant, never as the authority.
  - Fixed-point math, seeded RNG per system and the integer game-seconds clock (TM9) are implemented in Rust.
  - Client code stays GDScript unless profiling shows a hot spot (e.g. map rendering) that needs native code.
- Alternative considered: C# for both sides. Easier to learn, but slower and heavier for the sim, with less control over determinism.
- godot-rust supports Godot 4.7 (gdext 0.5.4+ has an `api-4-7` feature; checked 2026-10-11).
- Open details: the server's async/networking crate (TS7).
- Cross-checks: TS1, TS4, TS5, TS7, TM9

### TS5. Simulation architecture 🟡
- **Draft: data-oriented, event-driven, deterministic, one thread per world.**
  - **Layout:** world state is flat tables (typed vectors) indexed by typed IDs (e.g. `ProvinceId`, `ArmyId`), not linked objects. Hand-written tables, no ECS library: the entity types are few and fixed, and strict ordering matters more than convenience.
  - **Timeline:** one event queue ordered by `(game-second, priority, sequence number)`, so ties resolve the same way every run. Priority comes from a fixed table per event kind (TM2). Exact events (an army arrives) are queued directly; recurring daily/monthly events re-schedule themselves (TM2).
  - **Changes only through events:** a player order is validated and appended to the input log (TM9), then turned into events. State changes only by running events, so snapshot + input-log replay re-derives the exact world.
  - **Threading:** a world runs on a single thread to stay deterministic. Scale comes from running many worlds in parallel. Parallel read-only work inside a single event or daily pass is allowed later if profiling needs it.
  - **Systems:** economy, movement, combat etc. are separate modules that read state and emit events, never call each other directly. Each has its own seeded RNG stream (TM9).
  - **Snapshots:** the full state is saved periodically (e.g. each game-month); restart = latest snapshot + replay of the log after it (TS6).
- Event queue (stage 1): a binary heap keyed `(game-second, priority, sequence)`; lower priority runs first; the sequence is a per-queue counter, so every key is unique and pop order never depends on heap internals.
- Open details: snapshot interval, how modules share read access to state.
- Cross-checks: TM2, TM9, WD1, WD2, AI1, TS3, TS4, TS6

### TS6. Persistence ✅
- **Decision: one SQLite file per world**, holding that world's event log and snapshots.
  - **Recovery:** load the latest snapshot, replay the log after it (TS5, TM9).
  - **Writing:** each input (order, admin action, seed) and the daily state hash are appended to the log. Writes are flushed in small batches (about every second) with SQLite WAL and `synchronous=FULL`, the only setting that is durable across power loss. An order is acknowledged, and events are sent to clients, **only after the batch covering them is durable** (at most ~1 s delay). A crash therefore loses only what no client has seen.
  - **Snapshots:** taken on a schedule (e.g. each game-month, about 30 real hours at 24x). Old ones are kept for debugging, replays and rollbacks, and pruned later.
  - **Scope:** the world file holds game state only. Accounts, world list and chat live in a separate shared database (TS11).
  - **Versioning:** every snapshot and log record carries a format version from day one, with migration code to upgrade old worlds after game updates. Every update also takes a snapshot and stamps the log with a ruleset version (TM9).
- Alternative considered: one shared PostgreSQL for all worlds. Easier cross-world queries and standard backup tools, but heavier to run and a shared bottleneck and single point of failure.
- Open details: flush interval, snapshot interval and retention, backup strategy, migration approach.
- Cross-checks: TM3, TM9, TS4, TS5, TS11, WD1, WD4

### TS15. Repo layout ✅
- **Decision: one repo.** The repo root holds the Cargo workspace (`crates/sim`, `crates/server`, `crates/protocol`, and later `crates/gdext` if the client ever loads the sim), `docs/` and `CLAUDE.md`. The Godot project moves to `client/` (so `project.godot` lives in `client/`), keeping Godot away from Rust build output and docs.
- Folder and file names are lowercase `snake_case`; third-party Godot code lives in `client/addons/`; client folders are grouped by feature (`client/map/`, `client/ui/`, `client/net/`), each with its scenes, scripts and assets side by side.
- The move to `client/` was done in build-order stage 0.
- Open details: Git LFS for art, branching.
- Cross-checks: TS1, TS3, TS4

### TS12. Testing 🟡
- **Draft: Rust tests with `cargo test`, `proptest` for property tests, `criterion` for benchmarks, and committed golden hashes** (expected state hashes and PRNG values checked into the repo, so any change to sim output fails a test until the golden value is deliberately updated).
  - Every sim feature ships a determinism test: same seed + same inputs = same state hash; snapshot + log replay = live state (TM9).
  - `scripts/check.ps1` is the single gate: `cargo fmt --check`, clippy (`-D warnings`), `cargo test --workspace`, Godot headless smoke check (build-order rule 4).
  - Determinism bans are enforced by clippy (`clippy.toml`): `HashMap`/`HashSet` (`disallowed_types`), `Instant::now`/`SystemTime::now` (`disallowed_methods`). The gate's `lint-bans` step runs clippy on a fixture crate (`scripts/lint_bans/`) that breaks every ban and fails unless clippy rejects each one.
- Open details: GDScript test tool (GUT vs gdUnit4), chosen at build-order stage 8; CI (TS14).
- Cross-checks: TM9, TS3, TS5, TS13, TS14

## Topic backlog

- ⬜ **TS7 Networking** — transport (ENet / WebSocket / WebRTC), sync model (deltas, events), interest management by fog of war.
- ⬜ **TS8 Map data pipeline** — how provinces/terrain are authored and stored (province bitmap, vector, generated), editor tooling.
- ⬜ **TS9 Map rendering** — province color-lookup shader, map modes, zoom levels, unit icons, renderer choice.
- ⬜ **TS10 Content data format** — Godot Resources vs JSON/CSV for goods, units, events; moddability.
- ⬜ **TS11 Backend services** — accounts, login, world browser, chat (links PL2, PL3, UI5).
- ⬜ **TS13 Performance budgets & profiling** — target world size, server CPU/RAM per world, client FPS, tooling.
- ⬜ **TS14 Build, deploy & hosting** — CI, export presets, server hosting, containers, update rollout.
- ⬜ **TS16 Server validation & anti-cheat** — order validation, rate limits, what the client is allowed to know (links FA).
- ⬜ **TS17 Logging, metrics & admin tools** — server logs, telemetry, GM/admin console.
- ⬜ **TS18 Localization** — languages at launch, string pipeline.
