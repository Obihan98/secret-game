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
- World (stage 2): `World` holds `Vec` tables (`nations`, `provinces`), the queue and the RNG streams; `run_until(t)` pops every event with time <= t. Event kinds and their priorities live in one `match` (`event.rs`); stream IDs in one table (`rng::streams`). The state hash covers clock, streams, tables and the pending queue (TM9). Golden hash for the tiny world is committed in `crates/sim/tests/tiny_world.rs`.
- Movement (stage 6): `movement.rs` adds `impl World` methods for the `March` order and the `Arrive`/`Meet` events. Armies are a table (`Army`: owner, position, remaining route, generation); `neighbours` is derived from `borders` on load and never saved. A re-planned army bumps its `generation`, and queued events carrying the old value do nothing when they come up: the heap cannot remove events, so a stale one costs one pop.
- Log and replay (stage 3): `log::Recorder` wraps the world and is the only way inputs reach it: `submit(order)`, `admin(action)`, `advance_to(t)` (which writes a daily hash at each day boundary), `snapshot()`, and `Recorder::replay(snapshot, log)`. Orders are sim types for now (`order.rs`, one stub order); the protocol crate takes over the wire format at stage 7.
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
- **Format v1 (stage 3):** our own fixed-width little-endian encoding, no serialization crate, so a dependency update can never change what an old file means. Decoding is strict (exact length, known tags), so different bytes never decode to the same value. A log record is `format version (u16), game-second, tag, payload`. A snapshot is `format version (u16), ruleset version (u32), number of log records it covers, the running daily log digest`, then the world state in the same order the state hash uses. Golden bytes for v1 are committed in `crates/sim/tests/replay.rs`; the first layout change bumps the format version and must load those bytes through a migration (the stage 3 "N loads in N+1" test becomes real then).
- **Snapshot format v2 (stage 6):** records and snapshots now carry separate versions (`RECORD_FORMAT` stays 1: a new order tag extends the layout without changing old bytes; `SNAPSHOT_FORMAT` is 2). Snapshot v2 adds terrain per province and the border, army and contact tables. Version 1 snapshots load with every province as plains and no borders or armies; the committed v1 golden snapshot loads in the v2 build and runs on byte-identically to the live world, which is the first real "N loads in N+1" test. `RULESET_VERSION` stays 1 although movement changes outcomes: no world has been released, so there is no history to keep apart yet; bumps start with the first public world.
- **Stage 5 (2026-10-11):** `crates/server`: `store::Store` owns the file: table `log` (one row per record, `seq` = log index, the record's v1 bytes) and table `snapshot` (snapshot bytes plus the log length and game time they cover); table layout versioned in `PRAGMA user_version` (schema 1). `persist` commits all pending records in one transaction (the batched flush); `save_snapshot` commits the pending log and the snapshot together, so a stored snapshot never covers records the file lacks. Recovery = latest snapshot + replay of the log (TM3). `host::Host` holds every ack in an outbox until the flush that makes its record durable; broadcasts will use the same outbox (stage 7). Backups use SQLite's online backup API and hold what was committed at that moment.
  - Gate tests (`crates/server/tests/persistence.rs`): the world runs in a child process that is killed 8 times at random points and restarted from its file; every recovery equals the clean run's log at a flush point, never loses committed records, and contains every ack a client was sent; the final state equals the clean run. Kills are process kills, not power cuts: power-loss safety rests on SQLite's WAL + `synchronous=FULL` guarantee, not on our test.
- Open details: flush interval (~1 s, set by the stage 7 loop), snapshot interval and retention, backup strategy (when and where), migration approach.
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

### TS13. Performance budgets & profiling 🟡
- **Draft: one full game-day of the whole world costs at most 1 second of single-thread server time** (decided with the user 2026-10-11, before measuring). That is every nation's daily pass on the stage 4 synthetic world (30,000 provinces, ~200 nations), release build, one thread. Against the 1 real hour a game-day lasts at 24x (TM1) this is ~0.03% of the hour, so one machine can host many worlds (WD1).
- Stage 4 also measures and records here, without a pass/fail target yet: event throughput, snapshot size and time, and event fan-out filtering for ~200 simulated clients.
- **Results (stage 4, 2026-10-11): GO.** `cargo bench -p sim` (`crates/sim/benches/scale.rs`), release build, one thread, AMD Ryzen 9 7900X, Windows 11. Synthetic world: 30,000 provinces with random owners among 200 nations.
  | Measure | Result | Note |
  |---|---|---|
  | One game-day: all 200 daily passes | 1.31 ms | |
  | One game-day: passes + daily hash record (**the gate**) | **1.61 ms** | budget 1 s: ~620x headroom |
  | Event queue, pop + push, 1k / 100k / 1M pending | 32 / 93 / 408 ns per event | ~2.4 million events/s even with 1M pending |
  | Snapshot size | 371 KB | after 30 game-days |
  | Snapshot save / load | 40 µs / 53 µs | |
  | State hash | 274 µs | byte-at-a-time FNV-1a; the slowest part of a game-day, still cheap |
  | Fan-out filter, 200 clients | 175 ns per event | stub visibility: own provinces + 300 others per nation (MP9 not designed) |
  - Caveat: the economy is still the stage 2 stub (each pass sums its nation's provinces, scanning all 30,000), so the real systems may cost far more per day. The ~620x headroom is what they can spend before the 1 s budget is hit; each system's benchmark is checked against it (build-order stage 10).
- **Results (stage 6, 2026-10-11): still GO.** Same machine and bench. Stage 4's world is kept unchanged so the comparison is like for like; stage 6 adds a 200 x 150 grid map (30,000 provinces, roads of 20-60 km, mixed terrain, each nation a 20 x 15 block) with 2,000 armies marching up to 15 provinces away.
  | Measure | Stage 4 | Stage 6 | Note |
  |---|---|---|---|
  | One game-day: passes + daily hash (**the gate**) | 1.61 ms | **1.64 ms** | +1% |
  | One game-day on the grid map, 2,000 armies marching | | 3.35 ms | arrivals, speed changes, meetings; plus 1.04 ms to hash that world |
  | Event queue, 1M pending | 408 ns | 455 ns | queue code unchanged; repeatable on reruns, so likely codegen or machine state; still ~2.2M events/s |
  | Snapshot size / save / load | 371 KB / 40 µs / 53 µs | 401 KB / 53 µs / 102 µs | expected: terrain per province, and loading rebuilds the road lists |
  | State hash | 274 µs | 297 µs | +8%, as many more bytes |
  - Margin: the build order asks that stage 4's numbers not regress "beyond the agreed margin"; no separate margin was agreed, so the rule applied is the 1 s game-day budget, with every change above 10% explained here.
- Open details: RAM per world, client FPS, profiling tools, the budget for the other numbers above.
- Cross-checks: TM1, TM2, WD1, WD2, MP2, TS5, TS12

## Topic backlog

- ⬜ **TS7 Networking** — transport (ENet / WebSocket / WebRTC), sync model (deltas, events), interest management by fog of war.
- ⬜ **TS8 Map data pipeline** — how provinces/terrain are authored and stored (province bitmap, vector, generated), editor tooling.
- ⬜ **TS9 Map rendering** — province color-lookup shader, map modes, zoom levels, unit icons, renderer choice.
- ⬜ **TS10 Content data format** — Godot Resources vs JSON/CSV for goods, units, events; moddability.
- ⬜ **TS11 Backend services** — accounts, login, world browser, chat (links PL2, PL3, UI5).
- ⬜ **TS14 Build, deploy & hosting** — CI, export presets, server hosting, containers, update rollout.
- ⬜ **TS16 Server validation & anti-cheat** — order validation, rate limits, what the client is allowed to know (links FA).
- ⬜ **TS17 Logging, metrics & admin tools** — server logs, telemetry, GM/admin console.
- ⬜ **TS18 Localization** — languages at launch, string pipeline.
