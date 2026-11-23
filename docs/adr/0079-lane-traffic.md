# ADR-0079: Lane traffic: lanes and junctions as entities, the IDM, junction grants, braking for people
- Status: Accepted (M2 phase 2.5d, 2026-11-22; author decisions N6 and N7)
- Date: 2026-11-22
- Builds on: ADR-0063 (determinism contract), ADR-0069 (storage kinds, commands, signals), ADR-0070 (templates and traits), ADR-0076 (Terra LOD), ADR-0077 (navigation and crowds)

## Context
M2's ports need vehicles that drive roads: rule-based, deterministic at any thread count, and authored by agents over MCP like the rest of a level. AI-driven behaviour stays in M7 and M4. Crowds (ADR-0077) already snapshot every crowd agent and character each tick in `CrowdGrid`. Physics has kinematic bodies, which follow their `Transform` and push what they meet.

## Decision
1. **Lanes and junctions are scene entities (N6).**
   - A `Lane` holds a polyline in its entity's space (translation and rotation; never scale, as in physics), a speed limit, a width, its `next` lanes and a junction `priority`.
   - A `Junction` lists its connector lanes. Two connectors conflict when their polylines cross sideways (away from a shared end), or when they lead into a common lane (a merge).
   - Roads are therefore saved in scenes, undone, edited over MCP and spawned from templates, with no new file format or tool.
   - At play start the lanes and junctions become a `TrafficGraph` resource: derived, unhashed, and static during play, like navmeshes.
   - Problems (a dangling reference, too few points, a zero length, a connector in two junctions) are logged, drawn red in the `lanes` layer, and put on their entity as `TrafficProblems` in the play world for `query.run`. Bad lanes are left out, and only their vehicles stop.
2. **Vehicles follow the Intelligent Driver Model** (Treiber, Hennecke and Helbing 2000), integrated with Treiber and Kesting's ballistic update.
   - `Vehicle` holds the IDM's parameters, with the paper's urban defaults, and an optional destination lane. `VehicleState`, which is hashed, holds the lane, the distance along it, the speed, the route and a status.
   - Routes come from Dijkstra over the lane graph by travel time, ties broken by lane id. A vehicle without a destination wanders: at each lane end it takes `next[SplitMix64(vehicle, lane) mod n]`.
   - It follows the nearest thing within 150 m ahead: a vehicle, a stop line it hasn't been let past, a dead end or its destination's end (each a stopped obstacle s₀ past the line), or a person.
   - Its `Transform` gets the lane point and a heading: a turn about y to the ground direction, then a pitch to the slope. It never rolls, and no trigonometry is used.
   - A vehicle has arrived when it stops at its destination lane's end, or in the queue behind a vehicle that has arrived there.
3. **Junction grants, from last tick's snapshot.** Each junction keeps its grants for vehicles still about to enter, on, or with their rear in a connector. A vehicle asks when its next lane is a connector and it is within twice the IDM's desired gap of the line. Requests are granted greedily, in order of (priority descending, waiting since, id), whenever they conflict with nothing held or granted. Everything a vehicle reads (the `TrafficSnapshot`, the grants, the graph) is fixed before it drives, so the hash is the same at any thread count.
4. **Cars brake for people (N7).** Crowd agents and characters in `CrowdGrid` count as stopped obstacles when their feet are within width / 2 plus their radius of a lane ahead, and within 2 m of it vertically. People don't yet steer round cars (M7). A vehicle template may add a kinematic body and collider, so physics characters bump into it.
5. **Where it lives:** `concordia-terra-crowd`, beside the crowd whose `CrowdGrid` it reads. `CrowdModule` adds traffic's processors (`traffic.place`, `traffic.snapshot`, `traffic.junctions`, `traffic.drive`) after `crowd.move`, so `[nav]` enables traffic. Nothing opts into simulation LOD: driving is cheap, and a held speed would run far vehicles into the car ahead.

## Alternatives
- **A `.lanes.ron` file per road network** (rejected by N6): it needs a format, a loader and its own editing and undo, and roads couldn't come from templates.
- **Vehicles that only see vehicles** (rejected by N7): a car would drive straight through a crowd.
- **Both cars and people avoiding each other now:** the crowd steers with circles, and cars are long boxes. Left for M7, with AI-driven behaviour.
- **Gap acceptance (time-to-collision checks) at junctions instead of grants:** realistic, but it decides from continuous estimates, which drift under LOD and are harder to make thread-count independent. Grants are a deterministic reservation.
- **Physics vehicles (wheels, suspension):** out of scope for rule-based traffic; a kinematic body is enough for collisions with characters.
- **A new `concordia-terra-traffic` crate:** it would have to depend on the crowd crate for `CrowdGrid` anyway, or duplicate the people snapshot.

## Consequences
- `[nav]` projects gain the `lanes` debug layer and the `Lane`, `Junction`, `Vehicle`, `VehicleState`, `JunctionState`, `TrafficProblems`, `VehicleArrived` and `VehicleRouteFailed` types, and the `Traffic` trait. Play-world hashes change only where those components exist.
- Lanes edited during play aren't picked up until play restarts.
- Studio: the level designer gains the `concordia-traffic` skill, and the gameplay programmer's `concordia-nav-agents` gains "Vehicles". No tool changes.
- Lane changes, traffic lights, gap acceptance, people avoiding cars, and traffic at M7's scale are later work.
