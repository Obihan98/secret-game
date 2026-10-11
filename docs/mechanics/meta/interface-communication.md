# Interface & Communication (UI)

> Scope: how players see the world and talk to each other. Only mechanics-relevant UI, not visual design.
> Depends on: everything · Feeds into: PL15, DI15

## Decisions

### UI1. Platforms ✅
- **Decision: PC only** (Windows, Mac, Linux desktop client). The full game is played only there.
- No browser build. A mobile companion app is possible later, mainly for notifications; not urgent.
- Notifications are sent by the server (email, Discord, later mobile push), so they do not depend on a game client. Details in UI2.
- Away-from-PC players (decided for v1): standing orders (AI2, AR17) and server notifications (UI2) carry the load. A mobile companion that can issue a few orders is reconsidered after playtests.
- Same topic as TS2, which holds the technical consequences.
- Cross-checks: TS2, UI2

## Topic backlog


- ⬜ **UI2 Notifications** — push, email, Discord; what triggers them.
- ⬜ **UI3 Map UI & map modes** — main view (links MP14).
- ⬜ **UI4 Reports** — battle, economic, intelligence reports.
- ⬜ **UI5 Messaging** — team chat, diplomatic channels, world chat, in-world letters with delay.
- ⬜ **UI6 Planning tools** — order queues, timelines, shared plans and map drawings.
- ⬜ **UI7 Onboarding** — tutorial, joining an existing team as a newcomer.
- ⬜ **UI8 Statistics & ledger** — historical charts and tables.
- ⬜ **UI9 Localization & accessibility**.
