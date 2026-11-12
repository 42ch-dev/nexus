# Isolation tests need an observable difference per isolated object

**Problem.** A test that claims "these two things do not share state" often exercises only one of them.
If the second object never performs the operation that would touch the shared state, the assertion
passes for the wrong reason — the test cannot fail even when the isolation is broken.

**Pattern.** For every isolated object in the claim, drive the operation that would leak. Then assert
both directions: the object's own counter changed, and the other object's counter did not. A negative
control (an object that must record nothing) is a third case, not a substitute for the positive one.

Concrete case: proving a simulation registry is per-run. The first engine dispatched a capability and
its counter incremented. The second engine ran only a capability-free graph, so a registry accidentally
shared with the first would still read zero — every assertion passed. The fix was to make the second
engine dispatch too, then assert its counter is 1 and the first's is still 1. Substituting the first
engine's registry then fails on both counters.

**Verification habit.** Before accepting an isolation test, ask: *what single-line mutation to the
code under test would this test fail to catch?* If the answer is "replacing object A's dependency with
object B's", the test is not yet an isolation test. Implementers should run that mutation and report
the red result; reviewers should ask for it.

**Why it recurs.** Isolation claims are structural ("these are separate allocations") while tests are
behavioural ("this count is zero"). A structural truth is easy to assert vacuously. The distinguishing
question is always whether the exercised path can distinguish the two objects at all.
