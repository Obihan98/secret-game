# World, Servers & Game Lifecycle (WD)

> Scope: what a "world" is, how it starts, how long it lasts, how it ends.
> Depends on: TM1 · Feeds into: PL3, PL4, MP1, EV1, BA3, MO4

## Decisions

### WD1. World structure ✅
- **Decision: many independent worlds.** Players create a world; **we host it** (see PL4). Each world is its own campaign with its own clock (TM1), starts on the same date (WD3) and ends on a fixed date (WD4). Others join by request (PL3, PL6).
- A world runs continuously in real time whether or not anyone is logged in. AI covers nations with no active player (AI1, AI2). Pauses are only the capped, public ones allowed by TM3 (the world's creator, or us for maintenance).
- **Who runs what:** we run every machine; a world's creator is its **admin** (invites, settings, capped pauses; see PL4, TM3). Player-hosted servers stay rejected.
- One deterministic single-threaded simulation per world (TM2, TM9), so we scale by adding worlds, and idle worlds can be hibernated.
- **Hibernation is lazy simulation:** the world's logical clock keeps running, but it is unloaded from memory and its event queue is drained deterministically when someone wakes it (or when an AI/scheduled event needs it). Nothing is lost and no schedule drifts. TM3's "no catch-up" applies to outages only.
- Rejected: one permanent shared world (no historical arc, no natural reset for snowballing, a bug or exploit hits everyone); player-hosted servers (the world dies when the host closes their PC, and the host can tamper with it).
- Open details: who may create a world and what it costs (MO, PL2); archive policy; when a world hibernates and what wakes it; public vs private worlds.
- Cross-checks: TM1, TM2, TM3, TM9, WD2, WD3, WD4, PL3, PL4, MO4, AI1, TS4, TS5, TS6

### WD2. World size 🟡
- **Draft: the map is the whole world (MP1), and every nation on it is playable.** Any nation can be claimed by a team (PL5, PL6). If nobody claims one, AI runs it (AI1, AI2) and it can be claimed later, including mid-game (WD7).
- **Compute is saved by activity, not by a list of nations.** An unclaimed AI nation with nothing happening to it can be simulated at **reduced fidelity** (simpler economy, slower cadence, fewer systems) and upgraded to full detail when a player claims it or something touches it (war, trade, diplomacy, conquest). The details of this dormant mode belong to AI1 and TS13.
- Rejected: a fixed list of unplayable minor nations (a nation that matters later, e.g. a colony that breaks away, could not be played; it needs a list to maintain; the compute saving is available without it).
- A world can have anywhere from a handful of players (mostly AI) to hundreds. Team size per nation is uncapped (PL1), so a one-player minor and a 20-player great power can coexist; keeping that fair is BA1's job.
- Open details: the dormant-mode model and what wakes a nation up; minimum and maximum players per world; world size presets; how small nations stay viable for a solo player (PL17).
- Cross-checks: MP1, MP2, PL1, PL5, PL17, AI1, BA1, TS13

### WD3. Start date ✅
- **Decision: every world starts on 1 January 1800.** Seven weeks after Napoleon took power (coup of 18 Brumaire, 9–10 Nov 1799), with France still at war with the Second Coalition (Austria, Britain and others), so there is something to do from day one.
- The Consulate, the Italian campaign (spring 1800, Marengo 14 Jun 1800) and the Empire (proclaimed May 1804, coronation 2 Dec 1804) are history that may or may not happen; they are not scripted. In a player-run world history diverges, and Napoleon only holds power if the players running France keep him there.
- Rejected for now: starting on 9 Nov 1799 with the coup as the opening scripted event (more drama, more setup). It remains a possible event (EV1).
- Open details: whether other start dates are offered as scenarios later; initial borders, rulers and armies (data work).
- Cross-checks: TM1, WD4, EV1, TE1

### WD4. Length & end ✅
- **Decision: every world ends in 1830, on 1 January 1830 unless the exact date is moved** (30 game years, ~456 real days, ~15 real months at TM1 pacing).
- **Why 1830:** the world should end when its own mechanics stop making sense, not at a round number. The Liverpool & Manchester Railway (opened 15 Sep 1830) is the first real intercity steam railway: from then on marching speed and foraging-based supply are no longer the limits of war, so the walking-speed movement, supply and courier-order mechanics no longer fit. After this the era needs a different game.
- After the end the world is archived read-only as a chronicle (TM10).
- **Assumes** courier-order latency (TM7) and foraging-based supply (AR8); revisit the 1830 end if either goes another way. "Telegraph" below means the electric telegraph.
- Railways exist only as an early, marginal technology near the end of the world (Stockton & Darlington, 1825); the electric telegraph (1837+) and rifled muskets (~1849) fall outside the span and are not modelled.
- Rejected: endless worlds (hosting cost never stops; no reset for snowballing; the Napoleonic setting runs out); a 20-year world to 1820 (arbitrary, and ends before the era's mechanics change); ending in 1849 at the rifled musket (~2 real years per world, and it forces railways and telegraph to be modelled as in-world technology that rewrites movement and order latency).
- Open details: exact end date (1 Jan 1830 vs the 15 Sep 1830 railway opening, ~467 days); whether the end is a pre-announced final phase; winner and scoring (WD5); optional sandbox continuation; whether a world can be shorter by host setting.
- Cross-checks: TM1, TM3, TE1, CB1, MP7, AR8, TM7, WD1, WD3, WD5, WD6, WD7, MO4, TS6

## Topic backlog

- ⬜ **WD5 Victory types** — domination, prestige score, historical goals, alliance/team victory.
- ⬜ **WD6 Seasons & carry-over** — what (if anything) carries between worlds.
- ⬜ **WD7 Late joiners** — entering a world that is already months old.
- ⬜ **WD8 Rule variants** — hardcore, casual, historically locked, sandbox.
- ⬜ **WD9 Scoring & leaderboards** — per nation, per player, per role.
- ⬜ **WD10 Admin/GM tools** — game masters, live intervention, event triggering.
