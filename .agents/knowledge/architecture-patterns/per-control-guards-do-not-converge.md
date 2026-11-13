# Per-control guards do not converge; guard the dispatch point

## Symptom

A legacy surface has many ways to perform one privileged write: a header button, a palette command, a canvas
connection handler, an inspector save, a keyboard shortcut, a replay action. A new authority must forbid all
of them while it owns the data. The natural fix is a flag check per control.

That fix does not converge. Four review rounds on one branch produced exactly this sequence:

1. Disabled the header's create control.
2. Disabled the palette command and unwired `onConnect`/`onReconnect`.
3. Disabled the edge inspector's save, its shortcut, and its re-apply entry.
4. Disabled a draft that had been authored before the flag flipped, plus its cross-flag replay.

Each round was correct and each round left the neighbour reachable, because the guard was written where the
control happened to be, not where the write happens. A reviewer who tests the boundary — "can any structural
write still reach the old authority?" — finds a different control every time, and the loop has no natural end.

## What to do instead

Find the single point where the privileged write is dispatched, enumerate the operations it can emit as data,
and guard that one place. Then make the enumeration load-bearing:

- A union type for the operations (`StructuralWriteOp`) and a `Record<StructuralWriteOp, dispatcher>` in the
  test, so adding an operation without wiring its guard is a **compile** error rather than a missing test.
- The guard reads the current authority **at dispatch time** through a ref, not at hook construction. A
  replay that crosses a flag flip is the case that catches a captured boolean.
- Delete the per-control guards once the dispatch guard exists. Keep the flag only where it drives
  user-visible disabled state and its named reason — a second authoritative gate is the defect again, one
  layer down.
- Prove both directions in one parameterised test: refused while the new authority owns the data, forwarded
  unchanged while it does not. The flag-off path must stay byte-identical, and a table is the only form that
  keeps that honest as operations are added.

## Why the enumeration matters more than the guard

A guard on a set you cannot enumerate is a guard on the cases you happened to think of. The enumeration is
the reviewable artifact: a reader can ask "what operations can this surface emit?" and check the list against
the wire contract. In this case the contract had only `create` and `update` for transitions, so the list
carried exactly those and did not invent a `delete` — which is the second half of the lesson: enumerate what
the code can emit, not what the API name suggests it might.

## Cost of getting it wrong

Three extra review rounds, each shipping a guard that the next round superseded, and a surface where the
simulated and approved document could differ from the graph on screen — the precise failure the surrounding
work existed to remove.
