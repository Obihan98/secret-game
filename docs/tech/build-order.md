# Build Order

> Scope: the order in which we write code, and the test gate that must pass before the next step starts.
> Rule: slow and steady. One stage at a time, each ends with passing tests, recorded numbers and a commit. No stage starts before the previous gate is green and the user signs off.
> Depends on: TS3–TS6, TS15, TM2, TM9 (all ✅ except TS5, which waits for stage 4). If a stage contradicts a decision, change the decision first (docs/README.md rules).

## Rules for every stage

1. **Decide before build.** Check the "Decide first" list. If a topic there is still ⬜, ask the user; do not guess.
2. **Smallest slice.** Build only what the stage lists. No features from later stages, no speculative abstractions.
3. **Tests first, or with the code.** Every stage ships its tests. A stage without a passing gate is not done.
4. **One command checks everything:** `scripts/check.ps1` (created in stage 0) runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, and the Godot headless smoke check. It must be green before every commit.
5. **Record numbers.** Benchmark results and budgets go into the TS13 section of `tech/stack.md`, never only in chat.
6. **Docs follow code.** When a stage changes or confirms a decision, update the domain file and the status board in the same change.
7. **Commit at each gate** (when the user asks), message `stage N: <what>`; tag `stage-N`.

## Determinism checklist (applies to every stage touching `sim`)

- Time is `i64` game-seconds. State math is integer or fixed-point. No `f32`/`f64` in state.
- No `HashMap`/`HashSet` in `sim` (enforce with clippy `disallowed_types`); no `Instant::now()`/`SystemTime` (clippy `disallowed_methods`).
- One seeded RNG stream per system, our own small PRNG (not a crate whose algorithm may change); golden-value tests pin its output.
- Queue key `(game-second, priority, sequence)`; priority from one fixed table.
- State changes only by running events; the input log holds orders, admin actions and seeds; a state hash is written every game-day.

## Stages

### Stage 0: Scaffold and tooling gate
- **Do:** move the Godot project into `client/` (`project.godot`, `icon.svg`, `icon.svg.import`; recreate `.godot/`); create the Cargo workspace (`crates/sim`, `crates/server`, `crates/protocol`), `rust-toolchain.toml`, `rustfmt.toml`, workspace lints, `.gdignore` where needed, `scripts/check.ps1`; add the sim lints above.
- **Decide first:** TS12 test tooling. Decided (🟡): `cargo test`, `proptest` for properties, criterion for benchmarks, committed golden hashes; GDScript tests chosen at stage 8.
- **Gate:** `scripts/check.ps1` is green on empty crates; `godot --headless --path client --quit` exits 0; a deliberately failing test turns the script red (proves the gate works).

### Stage 1: Sim primitives
- **Do:** typed IDs (newtypes), game-time type, fixed-point number type, own seeded PRNG with per-system streams, event queue with the ordering key.
- **Gate:** unit + property tests: queue ordering and tie-breaks; fixed-point add/mul/div overflow and rounding; PRNG golden values and stream independence (drawing from stream A never changes stream B); the clippy bans are active (a test file using `HashMap` fails the lint).

### Stage 2: A tiny world that runs
- **Do:** flat tables for a hand-made world (3 provinces, 2 nations); `run_until(t)`; staggered recurring daily event per nation (the economy stub); state hash function.
- **Gate:** same seed + same inputs ⇒ identical state hash after 365 game-days, run 100 times; different seed ⇒ different hash; stagger offsets differ per nation and are stable.

### Stage 3: Input log, snapshot, replay
- **Do:** versioned snapshot format; input log (orders, admin actions, seeds); daily state-hash records; `replay(snapshot, log)`; ruleset-version stamp.
- **Gate:** snapshot + log replay equals the live state at 1,000 random times; a flipped bit in the log is detected by the next daily hash; a snapshot from format version N loads in N+1 via a migration test.

### Stage 4: Scale spike (TS13, resolves T8)
- **Do:** a generator for a synthetic world (30,000 provinces, ~200 nations); criterion benchmarks for the daily pass, event throughput, snapshot size and time, and event fan-out filtering for ~200 simulated clients. Sim only, no network.
- **Gate:** numbers recorded in TS13 with a go/no-go: the daily passes of all nations for one game-day take at most 1 second of single-thread time in a release build (TS13, agreed with the user 2026-10-11). If it fails, change the design (province count, fidelity levels, table layout) before any later stage.

### Stage 5: Persistence
- **Do:** SQLite file per world (rusqlite, bundled), WAL + `synchronous=FULL`, ~1 s batched flush, snapshots in the same file, format versions, ack and broadcast only after flush.
- **Gate:** a crash test that kills the process at random points and recovers to a state equal to a clean run; ordering test proving nothing is acknowledged before it is durable; backup via the SQLite backup API restores correctly.

### Stage 6: Movement slice
- **Do:** a 10-province test map with graph edges; army march as an exact-timestamp event; daily pass recomputes speed and reschedules only changed arrivals (T12); one supply stub and a season modifier.
- **Gate:** hand-computed arrival times match exactly; a speed change mid-march reschedules once; interception cases covered; replay equality still holds; benchmarks from stage 4 do not regress beyond the agreed margin.

### Stage 7: Server skeleton
- **Do:** one process per world; protocol crate (orders in, events out); fog-of-war filter stub; lazy hibernation (unload and drain on wake); pause with the public caps.
- **Decide first:** TS7 networking transport and sync model.
- **Gate:** integration tests with fake clients: two clients with different visibility get different events; a restart resumes without catch-up; a hibernated world wakes in the same state as one that never slept; pause caps enforced and logged.

### Stage 8: Godot client skeleton
- **Do:** connect to the server; draw the test map with a province-colour lookup shader; show armies by extrapolating path + start time; send a march order.
- **Decide first:** TS8 map data pipeline, TS9 rendering, TS12 GDScript test tool.
- **Gate:** headless smoke test connects and receives state; screenshot or scene test shows the army at the right position at two timestamps; the client holds no game formulas (review checklist).

### Stage 9: Close the loop
- **Do:** two real clients play the movement slice against one server; disconnect/reconnect; admin pause.
- **Gate:** a scripted two-client session passes; manual playtest notes recorded in `docs/`.

### Stage 10 onward: gameplay, one system at a time
For each system (economy, pops, supply, combat, ...), in the P1/P2 order of `DECISIONS.md`:
1. Decide the topic in the docs (✅ or 🟡 with the user).
2. Write the events and state tables; add the system's PRNG stream and priority entries.
3. Tests: unit, property, golden hash, replay equality, benchmark vs the stage 4 budget.
4. Playtest note, then the next system.

## Decisions that block stages

| Before stage | Needs |
|---|---|
| 0 | TS12 test tooling |
| 4 | agreed performance target (TS13: 1 s per game-day, decided) |
| 6 | MP7 movement basics, MP3 terrain (minimal) |
| 7 | TS7 networking, MP9 fog of war (minimal) |
| 8 | TS8 map pipeline, TS9 rendering |
| 10+ | TM6/AI2, TM7, AR8, and each system's own topics |
