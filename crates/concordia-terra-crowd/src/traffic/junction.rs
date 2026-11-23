//! Junction grants: who may drive into which connector this tick (pure).

use concordia_core::id::EntityId;

use super::components::Grant;

/// A vehicle asking to enter a connector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Request {
    pub vehicle: EntityId,
    pub lane: EntityId,
    pub priority: u8,
    pub waiting_since: u64,
}

/// Grants `held` plus each request, in order of (priority descending, waiting since, vehicle
/// id), whose connector conflicts with none already held or granted.
pub(crate) fn grant(
    held: &[Grant],
    requests: &[Request],
    conflict: impl Fn(EntityId, EntityId) -> bool,
) -> Vec<Grant> {
    let mut order = requests.to_vec();
    order.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(a.waiting_since.cmp(&b.waiting_since))
            .then(a.vehicle.cmp(&b.vehicle))
    });
    let mut out = held.to_vec();
    for r in order {
        if out.iter().any(|g| g.vehicle == r.vehicle) {
            continue;
        }
        if out
            .iter()
            .all(|g| g.lane == r.lane || !conflict(g.lane, r.lane))
        {
            out.push(Grant {
                vehicle: r.vehicle,
                lane: r.lane,
            });
        }
    }
    out.sort_by_key(|g| g.vehicle);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> EntityId {
        format!("018bcfe5-6800-7000-a424-{n:012x}").parse().unwrap()
    }

    fn ask(vehicle: u64, lane: u64, priority: u8, since: u64) -> Request {
        Request {
            vehicle: id(vehicle),
            lane: id(lane),
            priority,
            waiting_since: since,
        }
    }

    #[test]
    fn junction_grants_follow_priority_then_arrival_then_id() {
        // Connectors 1 (main, priority 1) and 2 (side, priority 0) cross; 3 conflicts with
        // nothing. Vehicle 10 already holds 3. Requests: 21 for 2 (waiting since tick 5), 22 for
        // 1 (since 9), 23 for 1 (since 9) and 24 for 3.
        // Order: 22 and 23 (priority 1; same tick, so by id), then 21, then 24 (priority 0; 21
        // has waited longer). 22 gets 1; 23 shares 1 (the same connector doesn't conflict with
        // itself); 21's 2 crosses 1: refused; 24 shares 3 with 10.
        let crossing = |a: EntityId, b: EntityId| {
            let (a, b) = (a.min(b), a.max(b));
            (a, b) == (id(1), id(2))
        };
        let held = [Grant {
            vehicle: id(10),
            lane: id(3),
        }];
        let requests = [
            ask(21, 2, 0, 5),
            ask(23, 1, 1, 9),
            ask(22, 1, 1, 9),
            ask(24, 3, 0, 9),
        ];
        let got = grant(&held, &requests, crossing);
        let want = vec![
            Grant {
                vehicle: id(10),
                lane: id(3),
            },
            Grant {
                vehicle: id(22),
                lane: id(1),
            },
            Grant {
                vehicle: id(23),
                lane: id(1),
            },
            Grant {
                vehicle: id(24),
                lane: id(3),
            },
        ];
        assert_eq!(got, want);
        // Two equal-priority requests for the crossing connectors: 31 (for 2) has waited since
        // tick 3, 30 (for 1) since tick 7. The longer wait goes first though its id is higher: 31
        // gets 2, and 30 is refused.
        let got = grant(&[], &[ask(30, 1, 0, 7), ask(31, 2, 0, 3)], crossing);
        assert_eq!(
            got,
            vec![Grant {
                vehicle: id(31),
                lane: id(2)
            }]
        );
        // With 22 holding 1 already, the side request is refused whatever its wait.
        let held = [Grant {
            vehicle: id(22),
            lane: id(1),
        }];
        assert_eq!(grant(&held, &[ask(21, 2, 0, 0)], crossing), held.to_vec());
    }
}
