# Thoughts on a Reliable Programming Language

Current programming languages have made great strides toward correctness,
especially in memory safety, but they do little to prevent other common
programming errors. In this journal, I want to explore how a language might
eliminate some of those other classes of failure.

## Software Availability

I am particularly interested in preventing resource exhaustion and ensuring
panic freedom. I think a language could achieve both structurally through a
small set of ideas:

1. Any fallible operation should return an `Option` or `Result` type. This
   includes array indexing, but also mathematical operations where
   divide-by-zero or arithmetic overflow are possible.

2. Recursion is unnecessary, and all iteration should be bounded. Turing
   completeness should not be a design goal. In production systems, functions
   should be total; otherwise, you are accepting the possibility of
   denial-of-service bugs.

3. Memory use should always be bounded. Real systems do not have infinite
   memory, so the language must force programmers to reason about exhaustion.
   Declare upper bounds on usage and make the exhaustion path explicit in the
   code.

## Fast Allocation and Reclamation

Java's garbage collector is generational: it splits the heap into young and old
generations. New allocations start in the young generation, which can be
collected quickly. Objects that survive long enough are promoted into the old
generation, which is handled by a tracing collector. This works well because
most allocations are short-lived.

A related observation is that most long-lived allocations belong to collections
such as lists, maps, and trees. These structures already manage memory
internally. Vectors often grow by reallocating into larger buffers. Maps are
usually built from hash tables or B-trees, both of which already maintain their
own storage strategy.

For an event-loop-based language, we could combine these ideas. Dynamic
allocations made during a single pass through the event loop could be
bump-allocated and reclaimed when control returns to the outer loop. Data that
must outlive a single event would need to be moved into an explicit collection
type that manages its own memory. This is similar to Java's young generation,
except that instead of a general-purpose old generation, we would rely on
explicit collection types.

The downside is that reclamation for the short-lived region, though nearly
instant, could happen only at event-loop boundaries. That seems like a
reasonable tradeoff for a scheme that is fast, simple to implement, and easy to
reason about.

## Obligations and Observations

One limitation of checked arithmetic is that performing a checked operation on
every update may be inefficient. Consider this Rust example:

```rust
fn count_large_values(values: &[u32]) -> Option<u32> {
    let mut large_values = 0u32;

    for &value in values {
        if value > 2 {
            large_values = large_values.checked_add(1)?;
        }
    }

    Some(large_values)
}
```

This performs a checked addition on every increment. In this case, however, we
can establish a stronger fact before the loop begins: the loop cannot increment
`large_values` more times than there are elements in `values`.

```rust
fn count_large_values(values: &[u32]) -> Option<u32> {
    if values.len() > u32::MAX as usize {
        return None;
    }

    let mut large_values = 0u32;

    for &value in values {
        if value > 2 {
            large_values += 1;
        }
    }

    Some(large_values)
}
```

The `+=` creates an obligation: the increment must not overflow. The earlier
length check provides the observation that discharges that obligation. A system
built around obligations and observations could replace many repeated dynamic
checks with a smaller number of earlier proofs.

This framework could be extended much further. One application that interests me
is enforcing relational constraints.

## Relational Constraints

Systems often contain multiple collections whose contents are related. For
example:

```rust
let mut employees: Map<EmployeeId, Profile>;
let mut managers: Set<EmployeeId>;
```

If all managers must also be employees, then membership in `managers` should
imply membership in `employees`. With most languese like Rust, that relationship
is usually maintained only by careful review of code that updates both
collections.

A common alternative is to add an `is_manager` flag to `Profile`, but that
approach does not scale well. Every new relation becomes another field to
maintain, and removing a relation later can be awkward.

Instead, we could make the relation explicit in the declaration itself
(extending rust syntax a bit.):

```rust
let mut employees: Map<EmployeeId, Profile>;
let mut managers: Set<EmployeeId>
    implies employees.contains(EmployeeId);
```

Now inserting an ID into `managers` creates an obligation: that ID must already
be present in `employees`. Likewise, removing an employee from `employees` would
create an obligation to show that the ID is no longer present in `managers`.
Observations elsewhere in the program would provide the evidence needed to
discharge those obligations.
