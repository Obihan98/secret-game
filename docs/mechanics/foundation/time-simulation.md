# Time & Simulation (TM)

> Scope: the game clock, how the simulation advances, calendar, determinism.
> Feeds into: everything. Every rate, timer and delay in other files is expressed in this clock.

## Decisions

### TM1. Time scale ✅
- **Decision:** 1 real hour = 1 game day (24x speed), single clock for all systems. The ratio is a **per-world setting** with 24x as the default; every timer is stored in game-time so changing it needs no redesign.
- **Calibration point:** Paris → Vienna (~1,200 km by road) at a historical 20–25 km/day is 48–60 game days, which is about **2 real days** at 24x. That is the target feel.
- **Realism vs playability are separate dials:** unit speeds stay historical (km/day by unit type, terrain, roads, season, supply) and are never bent to fix pacing; the clock ratio is the one number tuned for pacing.
- Consequences to keep in mind:
  - 1 game year ≈ 15 real days.
  - 1800–1830 (WD3, WD4: 30 game years) ≈ 456 real days (~15 real months). 1800–1815 (Napoleonic core) ≈ 228 real days (~7.5 months).
  - An army marching 25 km/day covers 25 km per real hour.
  - A threat 100 km from the border arrives in ~4 real hours, 300 km in ~12. A solo player who sleeps can lose a province; this is owned by TM6 (protections / defensive grace), not solved by slowing the clock.
- **Assumes** instant order delivery until TM7 is decided, and that TM6/AI2 will protect sleeping players. Revisit this draft if either goes another way.
- Open details: forced marches (faster, costs attrition: AR8, MP7); whether to ship a slower preset world for casual players.
- Cross-checks: WD3, WD4, MP1, MP7, AR4, CB3, EC5, TM2, TM3, TM6, TM7

### TM2. Tick model ✅
- **Decision: hybrid.** One deterministic event queue per world, integer game-time, server-authoritative. No separate simulation loop.
  - **Exact-timestamp events** for independent things with a known end: movement, combat phases, sieges, construction, treaty timers, courier arrival.
  - **Recurring scheduled events** (daily/monthly) for coupled systems: economy, pops, supply, attrition. They are ordinary queue events that re-schedule themselves. Passes are staggered per nation so players cannot time actions around a shared boundary.
  - **Movement depends on the daily pass (decided):** an army's arrival event is scheduled from its current speed. The daily pass recomputes speed (supply, season) and reschedules arrivals only where speed changed, so the cascade is bounded to once per game-day. A siege ends through the daily supply check or an assault event. Interception is found when paths are planned or re-planned.
  - **Client derives, server settles.** Position is a path plus a start time; balances are `balance + rate × elapsed`. The client animates both locally. The server sends only events the player is allowed to see (fog of war) and settles true values at scheduled events or when a rate changes.
- Why hybrid over the previous draft (fully continuous): coupled systems cascade. One rate change (e.g. a blockade) forces recomputing and rescheduling everything downstream, which is fragile and hard to test. A daily pass sidesteps it, and at TM1 pacing (1 real hour = 1 game day) each nation's pass runs once per game day (about once per real hour), so a world with N nations runs about N passes per real hour, spread out by the stagger. The load is to be measured (TS13 spike), not assumed.
- Implication: simultaneous events need a fixed ordering, `(timestamp, priority, sequence id)`, so outcomes are identical every run (see TM9). Decided: priority comes from a fixed table per event kind (e.g. a combat phase runs before the daily pass in the same second); `sequence id` breaks any remaining tie. TS5 uses the same key.
- Decided: the client holds **no game formulas** beyond linear extrapolation of server-sent parameters (position = path + start time; balance = `balance + rate × elapsed`). Anything else, such as a march-time preview, is a server query. So no formulas are shared with the client in v1 (TS3, TS4).
- Rejected: pure fixed tick (wasted work when idle, coarse timing for marching and battles); pure continuous (see above); client-authoritative simulation (cheating, desyncs).
- Open details: daily stagger scheme; which systems run daily vs monthly (TM8).
- Cross-checks: TM1, TM6, TM8, TM9, MP2, MP7, MP9, FI3, PO5, CB3, TS3, TS4, TS5

### TM9. Determinism & audit log ✅
- **Decision:**
  - **Time:** integer game-seconds (64-bit). No floats for anything that affects ordering.
  - **Randomness:** seeded RNG per world, seeds recorded in the log. One stream per system (combat, events, espionage, ...) so a change in one system's event order does not shift another's rolls. Rolls are drawn in a fixed order.
  - **State math:** integer or fixed-point for anything that affects state. Floats only for display and client-side interpolation; the server's integer result is the truth.
  - **Persistence:** periodic snapshots plus an append-only **input log**: player orders, admin actions (pause, resume, ratio change) and RNG seeds. Replay re-derives every other event by running the sim; derived events are not logged. A **state hash every game-day** is written to the log so a divergence is detected within a day. Any moment inside one ruleset version can be rebuilt by loading a snapshot and replaying forward. Enables audit, dispute resolution and crash recovery; rollback policy belongs to FA6.
  - **Game updates:** each update takes a snapshot and stamps the log with the new **ruleset version**. Replay and audit work within one version; older history is kept as read-only evidence.
- Cost accepted: the simulation must be strictly deterministic (no hash-order iteration, no wall-clock reads, no platform-dependent float behavior). This constrains language and architecture (TS3, TS5), so it is decided before them.
- Open details: snapshot cadence; log retention; whether players can see replays of their own battles (TM10).
- Cross-checks: TM1, TM2, TM3, TM10, TS3, TS4, TS5, TS6, TS12, FA (dispute resolution)

### TM3. Downtime & pausing ✅
- **Decision: the world clock runs only while the server is up.**
  - **Downtime does not exist for the world.** If the server is down for 2 hours, game time does not advance. No battle, siege or arrival happens while players cannot see or react. In-flight timers (marches, sieges, couriers, construction) resume from where they stopped, so nobody is penalised for an outage.
  - **No catch-up.** After a restart the server does not fast-forward through missed time. It loads the latest snapshot, replays the log (TS6) and resumes from the last logged event.
  - **The world admin can pause, within caps.** The admin is the world's creator (PL4); we can also pause any world for maintenance, updates or a bug. The clock stops, all timers freeze, and players are notified (UI2). **Every creator pause is capped at 2 real hours each and 8 real hours per week, and publicly logged** in the world chronicle (TM10), so a pause cannot be used quietly to dodge a losing war. Orders may or may not be accepted while paused (open).
- **Hibernation is not downtime.** An idle world may be unloaded and lazily simulated (WD1); its clock does not stop. "No catch-up" above applies to outages only.
  - **No player-triggered pause or global ceasefire.** It would be exploitable and unfair to other teams in a persistent world. Truces belong in diplomacy (DI), not in the clock.
  - **The end date is in game time** (1830, WD4), so downtime and pauses push back the real-world end date but do not shorten the game.
- Consequence: a world drifts behind its planned real-world schedule by the total time paused. Acceptable; it only shifts the calendar.
- Open details: whether orders can be queued while paused; notice rules and whether our maintenance pauses count against the cap (they do not, by default); whether other teams can veto or vote on a creator pause (FA).
- Cross-checks: TM1, TM2, TM6, TM9, WD1, WD4, TS6, PL4, UI2, FA

## Topic backlog

- ⬜ **TM4 Calendar & seasons** — real Gregorian calendar; seasons as a world-wide modifier source (harvest, campaigning season, winter).
- ⬜ **TM5 Day/night** — does night matter (no marching/fighting at night, battles pause overnight)?
- ⬜ **TM6 Timezone fairness** — players asleep while their army is attacked; order windows, "quiet hours", defensive grace.
- ⬜ **TM7 Order latency** — do orders take effect instantly, or travel by courier from the capital/HQ (links ES5, MP10)?
- ⬜ **TM8 Recurring cycles** — which systems run on schedules (monthly budget, yearly census, harvest) and when.
- ⬜ **TM10 World chronicle** — automatic in-game history of the world (battles, treaties, coronations).
