# Map, Terrain & Movement (MP)

> Scope: the geography of the world and how things move across it.
> Depends on: TM1, TM2, WD2 · Feeds into: AR8, CB6, TR2, NV8, ES1, CO1

## Decisions

### MP1. Map scope ✅
- **Decision: the whole world.** Europe, the Mediterranean and Near East, the Americas, Africa, India, East Asia and the oceans in between. Overseas transport, ports and colonies therefore exist in the main game (CO, NV). How deep fleet management and the colony system go is open (NV1, CO1, both 🟡 "in scope, depth open").
- Every nation on the map is playable: see WD2.
- At TM1 pacing, oceanic travel is playable: an Atlantic crossing under sail takes about 35–45 game days, i.e. roughly 1.5–2 real days.
- Open details: province detail by region (full detail in Europe, coarser elsewhere); total province count and the compute budget (MP2, TS13); rendering and fog of war at world scale (TS9, MP9); how much of the unexplored world is hidden at the start.
- Cross-checks: TM1, WD2, MP2, MP9, NV1, CO1, TR1, TS9, TS13

### MP2. Spatial model 🟡
- **Draft: irregular provinces with a hierarchy** (province → region → area → country). Target roughly **30,000 provinces** across the whole world (MP1), denser in Europe and coarser elsewhere.
- **Why so many:** a nation is run by a team (PL1), so there must be enough meaningful work to split across many people. Each province carries real work (pops, buildings, resources, garrison, unrest), and each level of the hierarchy maps onto a delegable team role (PL7, PL8).
- **Depth for teams, automation for the small:** a default governor AI runs provinces by simple policy so a solo player or a small team is not overwhelmed, and a team takes over the provinces it cares about. This is part of the caretaker AI (AI1, AI3).
- Rejected: hexes (cannot represent real borders, rivers and passes); node graph (less intuitive, weaker fit for history).
- Why it is expected to be affordable (to be confirmed by the TS13 spike): the server does nothing between events and each nation's economy runs about once per real hour (TM2), so province count should not be the server's limit.
- **Assumes** MP9 (fog of war) and PL7/PL8 (roles map onto the hierarchy); revisit this draft if they go another way. The real limits are content authoring, per-client fog-of-war and network bookkeeping, and interface (map modes, filters, automation).
- Open details: exact count and how it is split by region; how provinces are authored (TS8); how a large nation's interface avoids busywork (UI).
- Cross-checks: TM1, TM2, WD2, MP1, MP9, PL1, PL7, PL8, AI1, AI3, TS5, TS7, TS8, TS9, TS13

### MP3. Terrain types 🟡
- **Draft (2026-10-11, minimal for build-order stage 6):** four terrain types, one per province: **plains, forest, hills, mountains**. For now terrain only affects march speed (MP7): plains full speed, forest and hills 25% slower, mountains half speed.
- Marsh, desert and urban, and terrain's effect on battles (CB), supply (AR8) and the economy, are decided when those systems are built.
- Cross-checks: MP7, CB6, AR8, TS8

### MP7. Movement model 🟡
- **Draft (2026-10-11, minimal for build-order stage 6):** one army speed for now: **20 km per game day** on plains, the historical pace TM1 is calibrated on (60 km of open ground = 3 game days = 3 real hours at 24x). Armies march from province to adjacent province along the border between them; each crossing has a length in km. The terrain of the province being entered sets the speed for that crossing (MP3).
- Arrival is an exact-timestamp event (TM2); the daily pass recomputes speed and reschedules only arrivals whose speed changed (T12).
- **Stage 6 details (2026-10-11):**
  - Speeds are whole meters per game-day: terrain, then winter, then supply, each rounding down (e.g. winter, foreign forest: 20,000 x 3/4 x 3/4 x 3/4 = 8,437 m/day). Positions round down, arrivals round up, so an army never arrives before it has covered the road.
  - Speed is set when a road is started and recomputed by the owner's daily pass; a sudden change (a province changing hands) reaches marching armies at that pass.
  - **Stubs, agreed with the user, replaced when their systems are designed:** winter = December to February on 365-day years from 1 January 1800 (TM4, MP6), 25% slower; out of supply = entering a province the army's nation does not own (AR8), 25% slower.
  - **Route (MP8 stub):** the player picks a destination; the game picks the fastest route by terrain (season and supply ignored for routing), ties to the lower province ID. A marching army finishes its current road before following a new route; one halted on a road may turn back.
  - **Meetings (contact stub until combat, CB):** armies of different nations meet when they share a point on a road (head-on, walking into a halted army, or a faster one catching up), or when one arrives where another stands. Both stop; a contact is recorded. Found when a road is planned or re-planned (TM2). Armies of the same nation pass freely.
- Later: speed by unit type, forced march, fatigue, roads (MP5), rivers (MP4).
- Cross-checks: TM1, TM2, MP3, MP5, MP8, AR8, AR1, CB

### MP9. Fog of war 🟡
- **Draft (2026-10-11, minimal for build-order stage 7):** everyone always sees the whole map and who owns each province. **Armies are visible only near you:** in your own provinces, in provinces bordering yours, and around your own armies (the province an army stands in, or both ends of the road it is on, plus the provinces next to those). An army on a road is visible if either end of the road is. Example: a French army marching inside Austria is hidden from Britain unless a British army is nearby.
- The server filters before sending (TS4): a client never receives what its nation cannot see, so a modified client cannot reveal it.
- Later: scouting radius and cavalry screens, information delay (MP10), spies (ES), seeing other nations' provinces in detail.
- Cross-checks: MP2, MP7, MP10, TS4, TS7, ES1

## Topic backlog

- ⬜ **MP4 Rivers & crossings** — bridges, fords, pontoons, river lines as defenses.
- ⬜ **MP5 Roads & infrastructure** — road quality, building roads, canals.
- ⬜ **MP6 Weather & climate** — winter, mud season, rain, regional climate (Russian winter, Spanish heat).
- ⬜ **MP8 Orders & pathfinding** — waypoints, march orders, rally points, follow/shadow orders.
- ⬜ **MP10 Information delay** — news travels at courier speed (links TM7, ES5).
- ⬜ **MP11 Control model** — ownership vs occupation vs military control of a province.
- ⬜ **MP12 Strategic locations** — fortresses, mountain passes, ports, straits, capitals.
- ⬜ **MP13 Zones of control** — can armies block or pin enemy movement?
- ⬜ **MP14 Map modes** — political, terrain, supply, trade, unrest, etc. (links UI3).
