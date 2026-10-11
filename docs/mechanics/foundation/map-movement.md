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

## Topic backlog

- ⬜ **MP3 Terrain types** — plains, forest, hills, mountains, marsh, desert, urban; what each affects.
- ⬜ **MP4 Rivers & crossings** — bridges, fords, pontoons, river lines as defenses.
- ⬜ **MP5 Roads & infrastructure** — road quality, building roads, canals.
- ⬜ **MP6 Weather & climate** — winter, mud season, rain, regional climate (Russian winter, Spanish heat).
- ⬜ **MP7 Movement model** — speed per unit type, forced march, terrain/road modifiers, fatigue.
- ⬜ **MP8 Orders & pathfinding** — waypoints, march orders, rally points, follow/shadow orders.
- ⬜ **MP9 ★ Fog of war** — what you can see, scouting radius, cavalry screens.
- ⬜ **MP10 Information delay** — news travels at courier speed (links TM7, ES5).
- ⬜ **MP11 Control model** — ownership vs occupation vs military control of a province.
- ⬜ **MP12 Strategic locations** — fortresses, mountain passes, ports, straits, capitals.
- ⬜ **MP13 Zones of control** — can armies block or pin enemy movement?
- ⬜ **MP14 Map modes** — political, terrain, supply, trade, unrest, etc. (links UI3).
