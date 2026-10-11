# Decisions

What we decide next, in priority order, across mechanics and tech.
Work top to bottom, one at a time. Record the answer in the domain file, then update the status board here.
Rules and status legend: [README.md](README.md).

---

## Status board

**✅ Final**

| ID | Topic | Decision | File |
|---|---|---|---|
| TM1 | Time scale | 1 real hour = 1 game day (per-world setting); Paris→Vienna ≈ 2 real days | [time-simulation](mechanics/foundation/time-simulation.md) |
| TM2 | Tick model | Hybrid: one event queue keyed `(game-second, priority, sequence)`; exact events + recurring daily/monthly events for coupled systems; movement re-planned at the daily pass; sudden changes apply instantly, gradual ones at the pass; per-nation pass offset drawn once from a seeded stream | [time-simulation](mechanics/foundation/time-simulation.md) |
| TM9 | Determinism & audit log | Integer game-seconds, seeded RNG per system, fixed-point state math (Q32.32, round half to even), xoshiro256** streams; input log + daily state hash (own FNV-1a 64) plus a digest of that day's log records; snapshots, with a ruleset version per update | [time-simulation](mechanics/foundation/time-simulation.md) |
| TM3 | Downtime & pausing | Clock runs only while the server is up; no catch-up after outages; the creator can pause (2 h each, 8 h/week, public), we pause for maintenance; hibernation is lazy simulation | [time-simulation](mechanics/foundation/time-simulation.md) |
| WD1 | World structure | Many independent worlds, created by users (creator = admin), hosted by us, running continuously | [world-lifecycle](mechanics/foundation/world-lifecycle.md) |
| WD3 | Start date | 1 January 1800 | [world-lifecycle](mechanics/foundation/world-lifecycle.md) |
| WD4 | Length & end | Fixed end 1830 (~15 real months), when the railway age obsoletes walking-speed movement and supply | [world-lifecycle](mechanics/foundation/world-lifecycle.md) |
| MP1 | Map scope | The whole world; overseas transport, ports and colonies exist (depth open) | [map-movement](mechanics/foundation/map-movement.md) |
| CB1 | Combat resolution | Stat-based, multi-phase, no tactical battles | [combat-sieges](mechanics/military/combat-sieges.md) |
| TS1 | Engine | Godot 4.7 stable, 2D | [stack](tech/stack.md) |
| TS2 / UI1 | Target platforms | PC only (Windows/Mac/Linux); no browser; standing orders and notifications carry away players in v1 | [stack](tech/stack.md), [interface-communication](mechanics/meta/interface-communication.md) |
| TS3 | Language & performance | Rust for the sim library and server; typed GDScript for the Godot client; no formulas in the client | [stack](tech/stack.md) |
| TS4 | Client/server architecture | Server-authoritative; standalone server runs the sim as an engine-independent library; Godot PC client is a viewer | [stack](tech/stack.md) |
| TS6 | Persistence | One SQLite file per world (WAL, `synchronous=FULL`): input log + snapshots; clients are told only after the flush; format versioned from day one | [stack](tech/stack.md) |
| TS15 | Repo layout | One repo: Cargo workspace (`crates/`) + `docs/` at the root, Godot project in `client/` | [stack](tech/stack.md) |

**🟡 Drafts (made, to revisit)**

| ID | Topic | Draft | File |
|---|---|---|---|
| WD2 | World size | Every nation on the whole-world map is playable; unclaimed nations run by AI, dormant ones at reduced fidelity. Scale spike passed (T8, stage 4); ready to revisit for ✅ | [world-lifecycle](mechanics/foundation/world-lifecycle.md) |
| MP2 | Spatial model | Provinces with a hierarchy, ~30,000 across the world. Scale spike passed (T8, stage 4); ready to revisit for ✅ | [map-movement](mechanics/foundation/map-movement.md) |
| TS5 | Simulation architecture | Hand-written ID-indexed tables (no ECS); one event queue; state changes only via events; one thread per world. Scale spike passed (T8, stage 4); ready to revisit for ✅ | [stack](tech/stack.md) |
| TS12 | Testing | `cargo test`, `proptest`, `criterion`, committed golden hashes; `scripts/check.ps1` is the gate; GDScript test tool chosen at build-order stage 8 | [stack](tech/stack.md) |
| TS13 | Performance budget | One full game-day of the whole world (30,000 provinces, ~200 nations) costs at most 1 s of single-thread time; measured at build-order stage 4 | [stack](tech/stack.md) |
| MP3 | Terrain types | Minimal: plains, forest, hills, mountains; for now only march speed (100% / 75% / 75% / 50%) | [map-movement](mechanics/foundation/map-movement.md) |
| MP7 | Movement model | Minimal: 20 km/game-day on plains, province to adjacent province, entered province's terrain sets the speed; exact arrival events, rescheduled by the daily pass | [map-movement](mechanics/foundation/map-movement.md) |
| PL1 | Shared nations | Teams run nations, no player cap per nation (limits open) | [players-teams](mechanics/foundation/players-teams.md) |
| PL4 | Hosting model | We run every machine; a world's creator is its admin (open: succession, private worlds, GM tools) | [players-teams](mechanics/foundation/players-teams.md) |
| NV1 / CO1 | Naval and colony depth | In scope since MP1; how deep is open | [naval](mechanics/military/naval.md), [colonies-overseas](mechanics/statecraft/colonies-overseas.md) |

## Tensions

- **T1: Does 1830 leave enough tech to progress through (WD4 vs TE1)?** Walking-speed movement, couriers and smoothbore combat hold for the whole span, so tech is mostly incremental (doctrine, organization, industry, percussion locks, steam ships). Check that the tech tree (TE1) has enough meaningful choices in 30 years without railways or the electric telegraph.
- **T2: World length (WD4 vs player retention).** ~15 real months is still a long commitment. If playtests show it is too long, the fallback is a faster ratio per world (TM1). The ~15 months holds only at the 24x default: TM1 also floats a slower preset for casual players (open), which would lengthen a world (about 30 real months at 12x) and raise the T3 cost, and WD4 adds a third knob ("shorter by host setting").
- **T3: Hosting (WD1 vs PL4, MO).** We host every world, so creating a world costs us money for ~15 real months (WD4), plus pauses and the read-only archive afterwards. Who may create one, and at what cost, is open (MO, PL2).

_From the P0 alignment pass (2026-10-11). "Blocks ✅" lists the drafts that should stay 🟡 until the tension is resolved._

- **T4 ✔ resolved (2026-10-11): who pauses and administers a world.** A world's creator is its admin and may pause, within public caps (2 real hours each, 8 per week) logged in the chronicle; we can also pause for maintenance; we run every machine. Recorded in WD1, TM3, PL4 (now 🟡). Still open: admin succession, veto/vote on a creator pause (FA).
- **T5 ✔ resolved (2026-10-11): hibernation.** Lazy simulation: the logical clock keeps running; an idle world is unloaded and its queue drained deterministically on wake. "No catch-up" (TM3) applies to outages only. Recorded in WD1, TM3. Still open: what triggers hibernation and wake-up.
- **T6 ✔ resolved (2026-10-11): the sim/log contract.** Event key `(game-second, priority, sequence)` with a fixed priority table per event kind. The log records inputs only (orders, admin actions, RNG seeds) plus a state hash every game-day; replay re-derives all other events. Each game update takes a snapshot and stamps a ruleset version; replay and audit work within one version. Clients receive events only after the covering flush (SQLite WAL, `synchronous=FULL`). Rollback policy goes to FA6. Recorded in TM2, TM9, TS5, TS6.
- **T7 ✔ resolved (2026-10-11): shared formulas.** The client holds no game formulas beyond linear extrapolation of server-sent parameters; previews (e.g. march times) are server queries; no sim crate in the client for v1 (GDExtension stays an option, and godot-rust supports 4.7). Recorded in TM2, TS3, TS4.
- **T8: Scale is assumed, not budgeted (WD2, MP2, TS5 vs TS13).** MP2 argues province count is not the server's limit because "the server does nothing between events", while MP1 lists the compute budget (with TS13) as open; WD2 (hundreds of players, every nation playable) and TS5 (one thread per world) are sized on the same assumption. TM2 also contradicts itself on load: it staggers the passes per nation (about one pass per nation per game day, so N nations means N passes per real hour) yet says the pass "runs about once per real hour per world", and MP2's affordability claim relies on that figure. MP2 itself names per-client fog of war and network bookkeeping as real limits, WD2 leaves what wakes a dormant nation open, and event fan-out to hundreds of clients is not discussed anywhere. TS5 allows parallel read-only work later and TS7 (networking) is open, so where that load lands is undecided. TS13 sits only in the P3 catch-all. Proposal: promote a first TS13 budget to P0 as a spike (e.g. 30,000 provinces, ~200 nations, ~200 simulated clients) before TS5 goes ✅. The authoring load (30,000 provinces plus 1800 data for every nation: MP2, TS8, WD3) is the bigger risk; MP1/MP2's "coarser elsewhere" is the lever. **Scheduled as stage 4 of [build-order](tech/build-order.md).** ✔ **Compute part resolved (2026-10-11, stage 4):** one game-day of a 30,000-province, 200-nation world costs 1.6 ms against a 1 s budget (TS13), and fan-out filtering for 200 clients ~175 ns per event. WD2, MP2 and TS5 are no longer blocked by compute and can be revisited for ✅. Still open: network load (TS7) and the authoring load.
- **T9 ✔ handled (2026-10-11): P0 drafts rest on open P1 topics.** The reliance is now written into each draft as an "Assumes" line: TM1 (instant orders, TM6/AI2), WD4 (TM7 courier orders, AR8 foraging supply), MP2 (MP9, PL7/PL8). Still to decide in P1: TM7 (at 24x a courier at ~100-150 km/day takes ~8-12 real hours Paris to Vienna, so decide it together with TM1), TM6/AI2, MP9, AR8. TM8 (daily vs monthly systems) stays in P3 until needed.
- **T10 ✔ resolved (2026-10-11): PC-only vs pace.** v1 stays PC-only; standing orders (AI2, AR17) and server notifications (UI2) carry away-from-PC play; a mobile order companion is reconsidered after playtests. Recorded in UI1.
- **T11 ✔ resolved (2026-10-11): navy and colonies.** In scope, depth open: NV1 and CO1 are now 🟡 ("in scope, depth open"); MP1 reworded. Fleet management depth is decided with NV1.
- **T12 ✔ resolved (2026-10-11): movement vs supply.** An army's arrival is scheduled from its current speed; the daily pass recomputes speed and reschedules arrivals only where it changed; sieges end through the daily supply check or an assault. Recorded in TM2.

---

## Priority order

- **P0 Foundations**: blocks writing any code. Changing these later means a rewrite.
- **P1 Pillars**: shapes every system. Decide before building gameplay.
- **P2 Systems**: decide when we start building that system.
- **P3 Later**: everything else, in the domain files' backlogs.

### P0 Foundations

| # | ID | Question | Why now |
|---|---|---|---|
| 1 | TM2 ✅ | ~~Tick model~~ decided: hybrid event queue; key `(game-second, priority, sequence)`; movement re-planned at the daily pass. | Every system's update logic, the server loop and determinism depend on it. |
| 2 | TM1 ✅ | ~~Time scale~~ decided: 24x default, per-world setting. | Sets the pace of everything; pairs with #1. |
| 3 | TM9 ✅ | ~~Determinism & event log~~ decided: input log + daily state hash; ruleset version per update. | Follows from #1. Locks the core data types. |
| 4 | WD1 ✅ | ~~One world vs many~~ decided: many worlds, hosted by us, creator = admin; lazy hibernation. | Sets server scale and hosting. |
| 5 | WD2 🟡 | ~~World size~~ drafted: every nation playable, dormant AI nations at reduced fidelity; scale spike passed (stage 4), ready to revisit. | Gives the performance budget. |
| 6 | MP1 ✅ | ~~Map scope~~ decided: the whole world; navy and colony depth open (NV1, CO1). | Sets map size and content load. |
| 7 | MP2 🟡 | ~~Spatial model~~ drafted: provinces with hierarchy, ~30,000; scale spike passed (stage 4), ready to revisit. | Drives data layout, pathfinding, rendering. |
| 8 | TS2 / UI1 ✅ | ~~Target platforms~~ decided: PC only, no browser; standing orders carry v1 for away players. | Rules languages in or out. |
| 9 | TS4 ✅ / PL4 🟡 | ~~Client/server architecture~~ decided: server-authoritative, engine-independent sim library; PL4: creator is admin, we run the machines (open: succession, private worlds). | Decides where the sim runs. |
| 10 | TS1 ✅ / TS3 ✅ | ~~Language & performance~~ decided: Godot 4.7, Rust sim + server, GDScript client (godot-rust supports 4.7, not used by the client in v1). | Depends on #8 and #9. |
| 11 | TS5 🟡 | ~~Simulation architecture~~ drafted: hand-written ID tables, one event queue, changes only via events, one thread per world; scale spike passed (stage 4), ready to revisit. | First code we write. |
| 12 | TS6 ✅ | ~~Persistence~~ decided: SQLite per world (WAL, `synchronous=FULL`), input log + snapshots. | The world must survive restarts. |
| 13 | TM3 ✅ | ~~Downtime & pausing~~ decided: clock runs only while the server is up, no catch-up after outages; creator pauses are capped and public. | Depends on #1 and #12. |

### P1 Pillars

| # | ID | Question |
|---|---|---|
| 14 | PL1 🟡 / PL7 / PL8 | Shared nations, team roles & permissions. |
| 15 | CB1 ✅ / CB2 / CB3 | Combat model final; battle formula, phases & duration open. |
| 16 | EC1 | Economic depth: simple vs goods-based. |
| 17 | PO1 | Pop model: one number vs class × culture × religion pops. |
| 18 | AR1 | Unit granularity: battalion … corps. |
| 19 | AR8 | Supply & logistics. |
| 20 | MP9 | Fog of war. |
| 21 | TM7 | Order latency: instant vs courier. |
| 22 | TM6 / AI2 | Timezone fairness & offline protection. |
| 23 | CH1 / CH2 | Characters, and whether a player *is* a character. |
| 24 | WD3 ✅ / WD4 ✅ / WD5 | Start (1 Jan 1800) and end (1830) final; victory/scoring (WD5) still open. |
| 25 | NV1 🟡 / CO1 🟡 | Naval depth and colony depth (both in scope since MP1; depth open). |
| 26 | TE1 | Tech system. |

### P2 Systems

FI1, FI2, TR1, TR2, DI2, DI4, DI5, GO1, GO2, GO13, AI1, ES1, EV1, BA1, MP3, MP7, AR2, AR4, CB11, PL2, PL3, TS7, TS8, TS9, TS10, TS12 🟡 (GDScript tool at stage 8).

### P3 Later

All remaining ⬜ topics in the domain files.
