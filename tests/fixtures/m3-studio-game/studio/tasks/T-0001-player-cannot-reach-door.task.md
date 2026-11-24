---
id: T-0001
uid: 0193a1c2-7e40-7000-8000-000000000400
title: The player cannot reach the door
status: todo
role: gameplay-programmer
priority: high
created_by: human:author
acceptance:
- assert: 'within 600 ticks: distance(tag("player"), tag("door")) < 2m'
  playtest: tests/playtests/walk-to-door.playtest.ron
links:
- scripts/player_controller/src/lib.rs
---
Holding W should walk the player to the door 10 m ahead in a couple of seconds, but the player barely moves: after ten seconds they have gone less than a metre. Find out why, fix it test-first, and hand it to QA with a replay that proves the fix.

## Notes

## Review
