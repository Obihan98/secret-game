# Game Design Docs

A persistent, real-time grand strategy game set in the Napoleonic era. Armies march in real time, campaigns last months, and nations are run by teams of players, not individuals. Depth of EU5 (economy, pops, diplomacy, politics) combined with the military flavor of Napoleon Total War, resolved through stats and not tactical play.

These docs record *what the game is* and *why*, one decision at a time. They are not the implementation plan.
**Nothing is final yet.** We build the game up slowly and decide each topic when we get to it.

---

## Layout

```
docs/
├── README.md          ← this file: vision, rules, map of all domains
├── DECISIONS.md       ← what to decide next (priority order) + status board
├── mechanics/
│   ├── foundation/    ← time, world, map, players: everything else sits on these
│   ├── military/      ← armies, combat, navy
│   ├── economy/       ← pops, production, finance, trade
│   ├── statecraft/    ← diplomacy, government, characters, spies, tech, events, colonies
│   └── meta/          ← AI & offline, balance, fairness, interface, monetization
└── tech/              ← engine, language, architecture, tools
```

## Status legend

✅ Final · 🟡 Draft (leaning this way, will revisit) · ⬜ Open (not decided) · ❌ Dropped
★ = foundational: many other topics depend on it, decide early.

## Working rules

1. **One topic at a time.** Take the next topic from [DECISIONS.md](DECISIONS.md), discuss, decide, record it in its domain file.
2. **Decide when we build.** Only foundations get decided up front. Other topics are decided when we start building that system.
3. **Drafts are not final.** 🟡 becomes ✅ only after we revisit it and nothing conflicts.
4. **IDs are stable.** A topic keeps its ID (e.g. `EC4`) forever. Never renumber; dropped topics stay as ❌.
5. **Every decision lists cross-checks.** Each decided topic has a `Cross-checks:` line naming the IDs it touches.
6. **Alignment pass.** Before marking ✅, re-read every topic in its cross-checks. Conflicts go into *Tensions* in [DECISIONS.md](DECISIONS.md).
7. **Index stays small.** Details live only in domain files.

---

## Domains

### Foundation
| Prefix | File | Covers |
|---|---|---|
| TM | [time-simulation.md](mechanics/foundation/time-simulation.md) | Clock, events, calendar, determinism |
| WD | [world-lifecycle.md](mechanics/foundation/world-lifecycle.md) | Worlds/servers, start date, length, victory, resets |
| MP | [map-movement.md](mechanics/foundation/map-movement.md) | Map scope, provinces, terrain, weather, movement, fog of war |
| PL | [players-teams.md](mechanics/foundation/players-teams.md) | Accounts, joining, hosting, team roles, permissions, coups |

### Military
| Prefix | File | Covers |
|---|---|---|
| AR | [army-logistics.md](mechanics/military/army-logistics.md) | Units, recruitment, organization, supply, attrition |
| CB | [combat-sieges.md](mechanics/military/combat-sieges.md) | Battle model, phases, sieges, casualties |
| NV | [naval.md](mechanics/military/naval.md) | Fleets, naval combat, blockades, transport |

### Economy
| Prefix | File | Covers |
|---|---|---|
| PO | [population-society.md](mechanics/economy/population-society.md) | Pops, classes, culture, religion, manpower |
| EC | [economy-production.md](mechanics/economy/economy-production.md) | Goods, production chains, buildings, agriculture, industry |
| FI | [finance-budget.md](mechanics/economy/finance-budget.md) | Treasury, taxes, budget, debt, banking, inflation |
| TR | [trade-tariffs.md](mechanics/economy/trade-tariffs.md) | Markets, trade routes, tariffs, embargoes, smuggling |

### Statecraft
| Prefix | File | Covers |
|---|---|---|
| DI | [diplomacy.md](mechanics/statecraft/diplomacy.md) | Treaties, alliances, war declaration, peace, coalitions |
| GO | [government-politics.md](mechanics/statecraft/government-politics.md) | Government types, stability, factions, laws, rebellion, succession |
| CH | [characters.md](mechanics/statecraft/characters.md) | Rulers, marshals, ministers, traits |
| ES | [espionage-intelligence.md](mechanics/statecraft/espionage-intelligence.md) | Intel, spies, covert ops, couriers |
| TE | [technology-doctrine.md](mechanics/statecraft/technology-doctrine.md) | Tech, inventions, doctrines |
| EV | [events-history.md](mechanics/statecraft/events-history.md) | Historical/random events, missions, decisions |
| CO | [colonies-overseas.md](mechanics/statecraft/colonies-overseas.md) | Colonies, overseas expeditions |

### Meta
| Prefix | File | Covers |
|---|---|---|
| AI | [ai-offline.md](mechanics/meta/ai-offline.md) | AI nations, offline play, standing orders, abandoned nations |
| BA | [balance.md](mechanics/meta/balance.md) | Anti-snowball, team-size balance, catch-up |
| FA | [fairness-moderation.md](mechanics/meta/fairness-moderation.md) | Multi-accounting, collusion, bots, griefing |
| UI | [interface-communication.md](mechanics/meta/interface-communication.md) | Platforms, notifications, reports, messaging |
| MO | [monetization.md](mechanics/meta/monetization.md) | Business model, what is sold |

### Tech
| Prefix | File | Covers |
|---|---|---|
| TS | [stack.md](tech/stack.md) | Engine, language, architecture, servers, storage, tools |
| — | [build-order.md](tech/build-order.md) | Order of implementation and the test gate for each stage |
