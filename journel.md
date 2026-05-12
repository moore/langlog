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

## Top Level System

Given that all ordinary Langlog functions are meant to be total, any
long-running program needs a separate top-level system form. The top level
should still be hosted in Langlog rather than in a separate host language,
because a separate host language would complicate build and deployment,
especially for embedded and systems programming.

A Langlog system should define one or more tasks or state machines and the
explicit orchestration loops that drive them. These loops are not ordinary
functions. They describe the long-running schedule of the program, while the
task transitions they call remain total Langlog functions.

For orchestration code, Langlog could introduce a distinct `forever` loop. A
`forever` loop is guaranteed not to complete. It can support `continue`, but not
`break`. The only way to leave a `forever` loop is to exit the thread or
program. This cleanly separates total event handlers from intentionally
non-returning top-level systems.

Each `forever` iteration should still be bounded. One pass through the loop
should be a bounded dispatch or polling step, such as waiting for an event,
reading a device, advancing a tick, and dispatching to a task.

Event routing should use ordinary `match` syntax where possible. If programmers
already understand `match`, dispatching an event should look like a constrained
use of the same construct rather than a separate dispatch syntax. Task entry
points should probably be named `dispatch`, because dispatch is the boundary
between orchestration and total Langlog code.

As an example, for a network server there might be a top-level "accept" state
machine that requests accept events and then creates and owns connection state
machines. Each connection task would own its own retained state and expose a
`dispatch` entry point for connection events.

## Dispatch Lifetimes

I think we want to explicitly understand the dispatch of an event, as we will
later tie it to the management of allocation lifetimes. Any allocations required
to process the event can be freed when the event is finished being processed.
Any data that needs to be retained between events must be owned by the state
machine or task so that it is not collected at the end of processing the event.

It should be possible for one task to register another task with the system. We
might consider just making tasks actors. We should study Erlang here.

Langlog should define task structs that implement the dispatch trait. We may not
want to support persistent stacks at all. Instead, Langlog can lower suspended
or multi-event work into explicit task or state structs, similar to how Rust
async functions lower call state into future structs. Event-local temporaries
can still exist during a dispatch, but nothing from that temporary region may be
retained after dispatch returns.

This gives the language a simpler lifetime story. Anything retained between
events is part of a task or state machine and can be bounded, checked, moved,
and reasoned about. Anything allocated during one dispatch is temporary and can
be reclaimed when that dispatch finishes.

## Await State And Persistent Stacks

A related question is whether suspended Langlog tasks should keep a persistent
stack, like a green thread, or whether the compiler should lower suspension
points into explicit task state. Since Langlog functions are total, bounded, and
non-recursive, the usual stack-sizing problem is less severe here than it is in
a general-purpose language. The compiler should be able to compute a
conservative worst-case stack requirement for each bounded dispatch path.

That makes stackful tasks more plausible, but it does not make them the best
default. A persistent stack keeps the suspended call chain alive. Even when the
stack is correctly sized, it may retain values that are no longer conceptually
part of the task's long-lived state. With many suspended tasks, the memory cost
looks like the number of tasks multiplied by the worst-case retained stack size.

An explicit await-state representation has a tighter resource story. Each task
can store an enum or tagged state representing the current await point, plus
only the values that are live across that await. Normal computation during one
dispatch can still use the native call stack and CPU registers. Only values that
must survive after dispatch returns become fields in the task state.

This is not only a performance choice. It also supports the language model:
cross-event state is visible, bounded, inspectable, and subject to the same
resource reasoning as other task-owned data. Dispatch-local temporaries can use
fast temporary allocation and then be reclaimed at the event boundary. Event
data should probably be handled separately through explicit buffer pools: a task
can retain ownership of a pooled buffer until the event is processed or until
the task explicitly releases it. That would avoid unnecessary copies and leave
room for zero-copy or low-copy APIs such as moving data between files and
network sockets with `splice`.

The likely default should therefore be stackless lowering for suspended work:
use the native stack inside a bounded dispatch, but do not give every task a
persistent stack by default. A stackful task model could still be useful as an
explicit advanced feature for cases where direct-style execution is worth the
larger retained memory footprint.

## I/O Programs

The distinction between tasks and handlers needs to be precise. A task is
orchestration code: it may contain a `forever` loop, owns long-lived state and
resources, and drives progress for the system. A handler is a total function
dispatched to handle an event or continue work. Handler execution between
suspension points remains bounded.

Handlers should be allowed to construct and await I/O work, but they should not
directly execute that work. Awaiting an I/O operation should mean yielding an
I/O program to the task/runtime and resuming the handler later with the
program's terminal result. In direct-style handler code, this might look like:

```text
let request = await connection.read(within 5s);
let response = await lease_buffer(1024, within 1s);

encode_response(request, response);

await connection.send_all(response, within 5s);
```

The handler describes business intent. The task/runtime owns the operational
machinery required to make those awaits complete.

An I/O program is a bounded state machine for operational progress. It can
encode retry logic, backoff, readiness handling, completion handling, partial
read and write handling, timeout handling, and resource cleanup. This keeps
unbounded operational loops out of handlers while still allowing direct-style
handler code. The I/O program may make progress across many scheduler
activations, but each activation should do bounded CPU work. Operations that can
wait should carry an explicit timeout duration that bounds total elapsed
operation time. A timeout is a terminal failure for that I/O program. The
handler should not be asked to manually retry partial writes or spin on
`would-block`, because that would move an operational loop back into total
handler code.

An initial I/O program model could start with one operation per program while
still preserving the shape needed for later multi-step programs:

```text
LeaseBuffer { size, timeout } -> BufferLease | Timeout | Error
Read       { source, timeout } -> BufferLease | Closed | Timeout | Error
SendAll    { sink, buffer, timeout } -> Sent | Closed | Timeout | Error
Transfer   { source, sink, limit, timeout } -> Done | Closed | Timeout | Error
```

The important property is not that every backend has these exact operations.
The important property is that an operation has typed inputs, typed outputs,
owned resources, bounded per-activation work, and a terminal result. If a
`SendAll` times out after the backend accepted some bytes locally, the handler
should generally not receive a "sent byte count" or an "unsent remainder". The
runtime cannot know what the peer received or processed, and zero-copy backends
may not have an inspectable original buffer to return. Timeout should therefore
be treated as a terminal I/O-program failure, with cleanup handled by the
task/runtime policy.

This creates a useful separation of concerns. Handlers express business logic:
parse a request, build a response, ask that data be sent, and handle terminal
success or failure. I/O programs express operational policy: how to retry, how
to adapt to a readiness-based backend, how to submit completion-based work, and
how to clean up when progress fails.

The I/O model should be backend-agnostic. It should not bake Linux `io_uring`
into the language, even though a completion-oriented I/O program maps naturally
to `io_uring` submission and completion queues. A readiness backend such as
`epoll` can drive the same program by attempting nonblocking operations,
waiting for readiness when progress would block, and resuming the program later.
Embedded runtimes should also fit the model: interrupt-driven peripherals, DMA
engines, and specialized I/O machines such as Raspberry Pi PIO can all be
represented by task-owned handles and operations that make bounded progress over
time.

The same `SendAll` operation can therefore lower differently on different
systems. On `io_uring`, the runtime may submit one or more writes or linked
operations and resume the handler from completions. On `epoll`, the runtime may
attempt a nonblocking write, register interest when the operation would block,
and resume the I/O program when the descriptor is ready again. On an embedded
target, the runtime may arm an interrupt or DMA transfer and resume the program
when the device reports progress. These are backend details, not handler
semantics.

The core abstraction should be typed handles plus typed operations. Task
implementers can define the concrete handle types and the operations that
produce, use, transform, or consume those handles. Handlers should normally see
only the capabilities exposed by the task, not raw file descriptors, completion
queue entries, DMA descriptors, interrupt tokens, or other backend resources.
Those details belong to the task and the I/O program implementation.

For example, a connection capability exposed to handlers might support
`read(timeout)` and `send_all(buffer, timeout)`, while the concrete task
implementation privately stores a socket handle, a registered-buffer handle, an
`epoll` registration, an `io_uring` operation id, or a device-specific transfer
descriptor. The handler can pass the capability back to its methods, but it
cannot inspect or manufacture the backend handles.

For handler-visible data, the interface should be buffers. A handler that needs
to inspect or construct bytes can await a buffer lease, mutate it while it owns
the lease, and move it into an I/O program such as `send`. Once moved into an
in-flight I/O program, the handler no longer owns the buffer. The program
eventually completes with success or terminal error, and buffer cleanup is part
of the task/runtime's resource policy.

This suggests a linear or affine resource rule for buffers and handles. A buffer
lease can be read or mutated only while the handler owns it. Moving it into
`send_all` transfers ownership to the I/O program. Receiving a buffer from
`read` gives the handler ownership of a filled lease. Explicitly releasing a
lease returns it to the task's pool. The task owns the pool itself, so handlers
do not need to thread scratch buffers through every dispatch call just to reach
the operation that needs storage.

Zero-copy paths should not be forced through buffers. If a handler does not need
to inspect the data, operations such as file-to-socket transfer can remain
handle-to-handle I/O programs. This leaves room for mechanisms like `splice`,
DMA transfers, or backend-specific descriptor chaining while keeping the
handler-facing model simple.

A later multi-step I/O program could be a fixed-size sequence of these typed
operations, where each step produces, consumes, or transforms handles. For
example, a file-to-network program might consume a file-range handle and a
connection handle and produce only a terminal result. A sensor-read program
might arm a DMA handle, wait for a completion handle, and then return a filled
buffer lease to the handler. The sequence must remain statically bounded, and
any waiting step must have an explicit timeout duration.

## Loop Bounds And Complexity

Today Langlog has `for` loops for iterating over collections. Since collections
are bounded, those loops are bounded too. This seems like a reasonable place to
support `break` and `continue`, though nested bounded loops still need careful
thought. A loop inside another loop is still total if both bounds are known, but
the worst-case work is the product of the bounds, which matters for availability
and embedded scheduling.

Thinking further, nested loops deserve special attention to avoid quadratic or
worse dispatch latency. Langlog may want to disallow nested loops in ordinary
total functions, or at least warn on them by default and require an explicit
annotation when the programmer really wants that cost.

Many nested-loop use cases could instead be expressed with co-iteration
operations. Examples might include `zip`, `merge`, `join`, or `intersect`.
Those operations could carry known complexity contracts, so the compiler can
reason about their worst-case behavior without treating them as arbitrary nested
iteration.

As a long-term availability goal, ordinary total functions and event handlers
should default to linear-time or better over their explicit input and task-owned
state. Superlinear work should be visible and explicit. Queries against large or
unbounded task-owned collections should use data structures with declared
`O(log n)` or better worst-case lookup and membership behavior, so dispatch
latency remains predictable even when `n` is large.

## Efficient Collection Queries

Langlog should also explore developer assistance for efficient queries over
multiple collections. Modern databases use query planners that can choose
complex strategies such as index selection, joins, and materialized subqueries.
Langlog probably should not hide that much complexity inside the compiler, but
it can borrow the cost model. Since collections have explicit bounds and
declared operation complexity, the compiler can estimate worst-case query cost
and warn when a query shape is unnecessarily expensive.

Suggestions could include co-iteration, using an indexed collection, declaring
a relation, or explicitly materializing a bounded intermediate result. The
emphasis should be predictable worst-case performance, not average-case
cleverness. If the compiler gives a performance suggestion, it should explain
the bound that led to the warning, such as the product of two collection
capacities or a repeated linear scan over a large task-owned collection.

## Static Cost Budgets

Langlog should eventually produce a static worst-case cost model for total
functions. The model can count conservative operation units rather than exact
CPU cycles. Function calls contribute the callee's cost, bounded loops multiply
the body cost by the loop bound, `match` and `if` use the maximum branch cost,
and collection operations contribute their declared worst-case complexity.

Event handlers should be able to declare maximum budgets, and the compiler
should warn or reject when the worst-case cost exceeds the budget. For
`forever` loops, the loop itself has no finite cost, but each iteration should
be checked against a static per-iteration budget. This would extend totality
from "this handler returns" to "this handler returns within a predictable
budget."

## Guiding Idea

Looking across these ideas, the guiding question seems to be:

> What would a language focused on operational simplicity and reliability look
> like?

Operational simplicity means that deployed behavior should be predictable:
failure paths are explicit, execution is bounded, resource use is visible, and
state transitions preserve declared invariants.

That goal ties the language together. Langlog should help people write software
that is reliable, fast, and predictable. A second goal, on equal footing, is
that the language should still be something developers actually enjoy using. The
language should make safe and predictable code feel natural, not ceremonial.

## Task loops

we implmented task delagation and support recurisve delagation as it dose not
use a stack, but this dose allow for infinite loops that do no work. This is also
true of `foevere` loops. We shoudl see if we can come up with a way to show that
either of these loops is "doing work" that is to say that they are taking in new
input from outsied the world, via IO for example, or that they are making progress
torwas a termination state. This second is harder as it requires proving that there
is a terminal case and that each iteration move twords it. That we might want to
leave for later. It is more tractable at firs to show that each pass thought a
infinet loop gets new external information. This dose not prouve the loop is
productive but it excludes many forms of accdental non prodetive iteration.

## Explisit initilazation
We should not allow `let` statments that don't inilize values. This implies we should
have if/else and match {} as expressions. We may also want to implment `reduce` as a 
first call oppration over iterables.