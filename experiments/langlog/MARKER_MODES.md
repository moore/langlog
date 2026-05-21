# Langlog Marker Modes Specification

Status: draft 0. This document defines the structural marker-mode system used
to check copying, taking, discarding, and implicit discards for places that
carry marker facts.

Normative terms in this document follow RFC 2119.

This document complements, but does not replace, the other Langlog specs:

- [SPEC.md](./SPEC.md) defines the main surface language and front-end
  behavior.
- [SEMANTICS.md](./SEMANTICS.md) defines broader static and dynamic semantics.
- [TYPE_SYSTEM.md](./TYPE_SYSTEM.md) defines the broader type-system hierarchy
  that marker modes extend.
- [PROOF_IR.md](./PROOF_IR.md) defines the proof-specific IR boundary.

The prose in this document explains the design intent and motivation. The
requirement bullets define the normative behavior.

## Problem

Langlog markers currently act like compile-time facts about places. That works
well for proof facts such as "this index is less than this array length" or
"this key is a member of this map". Those facts can usually be copied,
forgotten, or recomputed without changing the meaning of the program.

Some facts are different. An `Event` marker is not just a passive proof that a
value was received. It carries a usage obligation: the program should not be
able to receive an event and then loop again without doing something meaningful
with that event. Similar obligations appear for resources, capabilities,
protocol states, and values that must be consumed exactly once.

The problem is to add those substructural use rules without turning Langlog
into a full dependent or linear type theory. The system needs to stay local,
finite, and cheap to check. It should preserve the existing marker model where
facts attach to places, while adding enough structure to reject accidental
copies and silent discards.

## Design Overview

The design gives every marker type a base structural mode. Marker facts stay
ordinary proof facts on places. Structural mode is carried by the place, not by
each fact. Marking a place adds a fact and merges the marker type's base mode
into that place's current mode. `use` and `consume` transform the target
place's current mode without adding, removing, or changing marker facts.

Place modes control how places behave when copied, moved, discarded, or
implicitly discarded. Ordinary proof markers are unrestricted. Resource-like
markers can be affine, relevant, or linear.

The user-visible model is place based:

- `=` assigns into a receiving place using a copy context;
- `<-` assigns into a receiving place using a move context;
- explicit modes in place type annotations declare the receiving place mode at
  function, task, state, field, return, and explicit local boundaries;
- `take` on a function parameter means the call moves the argument into the
  callee, while affine and linear parameter modes imply the same move;
- `_` is the discard place;
- `mark`, `use`, and `consume` are trusted checker-state operations on live
  places.

Produced values are not source-level places. A function call result,
constructor result, arithmetic result, or match result can flow into the
left-hand receiving place, but it does not become an anonymous place that
must itself be copied or taken. If the result of the right-hand side is a
place, the assignment context applies to that source place. Place occurrences
inside expressions are still checked by the context that uses them.

Composite values are handled by compiler-derived summaries. If a struct owns a
field whose current mode is relevant, the outer place behaves as if it
contains a relevant obligation until that field value is transformed by `use`
or otherwise moved out. Users should not have to write those summaries by hand.

Marker operations do mutate checker state for a live place. That is intentional:
many useful marker facts are learned by observing a boolean result and then
marking one of the operation's input places. These operations are not runtime
mutation; they update the current proof and mode environment for a place/version.

## LLG-MM-01 Marker Modes

Marker modes are the smallest extension that lets the existing marker system
express substructural obligations. A marker type declares a base structural
mode, but that mode is not stored on each marker fact. Instead, the checker
stores a current structural mode on each SSA place. `mark` can make a place
more restrictive by merging the marker type's base mode into the current place
mode.

This distinction lets a fact such as `LessThan` stay an ordinary proof fact
while letting `Event` or future resource markers affect copy and discard
checks. It also lets `use` and `consume` discharge substructural obligations by
transforming the target place mode without deleting marker facts.

The four place modes correspond to the usual structural permissions:
unrestricted places may be copied and discarded, affine places may be
discarded but not copied, relevant places may be copied but not silently
discarded until `use` transforms them to unrestricted mode, and linear places
may be neither copied nor silently discarded until `consume` transforms them to
affine mode.

- Every marker type MUST have exactly one base structural mode.
- A marker declaration without an explicit structural mode MUST default to
  `unrestricted`.
- The structural modes are `unrestricted`, `affine`, `relevant`, and `linear`.
- Every SSA place MUST have a current structural mode.
- A fresh SSA place with no restrictive marker contribution MUST begin
  unrestricted.
- The marker checker MUST use the following structural rules for each place's
  current structural mode:

| mode | copy with `=` | move with `<-` | implicit discard |
| --- | --- | --- | --- |
| `unrestricted` | allowed | allowed | allowed |
| `affine` | rejected | allowed | allowed |
| `relevant` | allowed | allowed | rejected |
| `linear` | rejected | allowed | rejected |

- Ordinary proof facts such as `LessThan` and `MemberOf` SHOULD be declared
  unrestricted.
- Resource-like facts that must not be silently ignored SHOULD use `relevant`
  or `linear`.
- Copying, moving, and duplicating a place MUST preserve its current structural
  mode at the moment of the operation.
- A place's current structural mode MUST affect copy, move, and discard
  checking.
- A place's current structural mode MUST NOT affect ordinary marker requirement
  matching.
- `mark` MUST merge the marker type's base mode into the target place mode.
- `use` MUST transform a relevant target place mode into unrestricted place
  mode.
- `consume` MUST transform a linear target place mode into affine place mode.

A place's structural behavior is current checker state. Local places can infer
that state from their initializer, while public and separately checked
boundaries state it with place type annotations. In both cases, the checker
maintains the current mode by merging mode contributions from marker operations
and reachable composite parts.

The merge operation is the intersection of structural permissions: the result
may be copied only if both inputs are copyable, and it may be discarded only if
both inputs are discardable. A place with no restrictive contribution is
unrestricted.

The merge operation is commutative and associative. In the table below, `M`
means any structural mode.

| left | right | merged place mode |
| --- | --- | --- |
| `unrestricted` | `M` | `M` |
| `affine` | `affine` | `affine` |
| `relevant` | `relevant` | `relevant` |
| `affine` | `relevant` | `linear` |
| `affine` | `linear` | `linear` |
| `relevant` | `linear` | `linear` |
| `linear` | `M` | `linear` |

- The marker checker MUST maintain each place's current structural mode.
- The marker checker MUST merge a marker type's base mode into the target place
  mode when `mark` adds that marker fact.
- The marker checker MUST include reachable composite part modes when deriving
  a composite place's structural mode.
- A place whose current mode is affine or linear MUST be non-copyable.
- A place whose current mode is relevant or linear MUST be non-discardable.
- A place with both a non-copyable obligation and a non-discardable obligation
  MUST derive linear structural behavior.

For example, marking a place with an affine marker and then a relevant marker
gives the target place linear merged behavior: it cannot be copied because of
the affine contribution, and it cannot be discarded because of the relevant
contribution. `consume` can then update the place to affine mode, leaving the
value non-copyable but discardable. A place that is only relevant can instead
be passed through `use`, which updates the place to unrestricted mode.

Example marker declarations:

```llg
marker LessThan(left: place, right: place) : unrestricted;
marker MemberOf(key: place, map: place) : unrestricted;
marker Event() : relevant;
```

## LLG-MM-02 Places And Produced Values

The design distinguishes places from produced values because marker-mode
obligations are tracked on storage identities. A variable, argument, field, or
IR slot can carry a current mode across checker steps. A produced value is the
result of expression evaluation; it has type and markings, but it is not itself
a source-level or IR-level storage identity unless lowering materializes one.

This distinction avoids requiring take syntax for ordinary fresh results:
`let x = read_u32()` is accepted because the right-hand side is not a place.
There is no source location being copied. By contrast, `let y = x` is a copy
because `x` is an existing place. `let z <- read_u32()` is also well-formed:
the produced value moves directly into `z` without creating an anonymous source
place.

- A place MUST be a source-level or IR-level storage identity.
- A place MAY be named or unnamed.
- A place MAY contain subplaces.
- A place MUST be able to hold a value and carry a current structural mode.
- Source-level places include local variables, function arguments, fields,
  projections, enum payload positions, and pattern bindings that are visible in
  source.
- IR-level places include return slots, call or parameter slots, projections,
  and SSA temporaries materialized by lowering.
- The compiler SHOULD introduce an IR-level place for an intermediate
  expression result only when that result must be tracked as storage
  independently for copy, move, discard, projection, or diagnostics.
- The discard place `_` MUST be a place.
- `_` MUST NOT be readable and MUST NOT bind a name.
- A produced value is the result of evaluating an expression that is not itself
  a place.
- Fresh expression results MUST NOT become places unless lowering materializes
  an IR-level storage identity for them.
- A produced value MAY carry runtime payload or storage, a concrete type, and
  zero or more marker facts.
- A produced value MUST flow into a place before it can be copied, moved,
  discarded, projected, or diagnosed as storage.
- Every `let` MUST initialize a place.
- The left-hand side of a binding or assignment using `=` or `<-` MUST be a
  receiving place, or a pattern whose binding positions are receiving places.
- When the right-hand side of a binding or assignment is a produced value
  rather than an existing place, `=` MUST be accepted without requiring a take,
  because there is no source place to copy from or take from.
- When the right-hand side of a `<-` binding or assignment is a produced value
  rather than an existing place, `<-` MUST be accepted because the produced
  value can move directly into the receiving place.
- Copy and move rules MUST apply when the right-hand side denotes an existing
  place.

```llg
let x = read_u32();  // allowed: RHS is a produced value
let y = x;           // copy from place x
let z <- x;          // move from place x
let w <- read_u32(); // allowed: RHS is a produced value
```

- An expression MUST NOT inherit placeness from place occurrences inside it.
- Place occurrences inside an expression MUST still be checked according to the
  context imposed by that expression.

## LLG-MM-03 Copy And Move Assignment

Copy and move assignment are receiving-place contexts. They describe how the
value produced by the right-hand side flows into the left-hand receiving place.
The left-hand side must be a receiving place, or a pattern of receiving places,
but the right-hand side need not itself be a place.

When the right-hand side result is a source place, copy preserves that source
place and copies the source place's current mode to the receiving place. Move
assignment takes ownership out of the source place and makes later reads of the
source invalid until it is assigned again. When the right-hand side result is a
produced value, the produced value flows directly into the receiving place and
there is no source place to copy from or take from.

Marker facts copy and move as ordinary proof facts. The place mode copies and
moves separately. If a place is copied while its current mode is relevant, the
receiving place receives relevant mode. If the place is copied after `use`
updates it to unrestricted mode, the receiving place receives unrestricted
mode. Later marker operations on the source do not retroactively update earlier
copies. This keeps the user model place based: there is no hidden
marker-instance identity to track in source programs, only live places with
modes and facts.

- `=` MUST assign the right-hand side result into the receiving place using a
  copy context.
- `<-` MUST assign the right-hand side result into the receiving place using
  a move context.
- If the right-hand side result of `=` is an existing place, `=` MUST copy that
  place.
- If the right-hand side result of `<-` is an existing place, `<-` MUST take
  that place.
- Copying a place MUST be allowed only when the source place's current mode
  permits copying.
- Copying a place MUST copy the source place's marker facts to the receiving
  place.
- Copying a place MUST copy the source place's current mode to the receiving
  place.
- Taking a place MUST transfer all marker facts owned by the source place to
  the receiving place.
- Taking a place MUST transfer the source place's current mode to the
  receiving place.
- After a place is taken, later use of that source place MUST be rejected unless
  the place has been assigned a new value.
- Copying a place MUST leave the source place live with its existing mode and
  marker facts.
- Copying a place MUST NOT transform the source place's current mode.
- After a take, the source place no longer owns the transferred obligation.
- Copying a place with current mode affine or linear MUST be rejected.
- A `let` using `=` MUST place its right-hand side in a copy-result context.
- A `let` using `<-` MUST place its right-hand side in a move-result context.
- If an expression result is a produced value, the result context MUST NOT
  create an additional copy or take of that produced value.
- If an expression result position denotes a source-level place, the result
  context MUST apply to that place.
- A `match` expression MUST apply its result context independently to each
  branch result.

```llg
fn handle(take value: relevant Message with Event) -> ();

let x = read_event(); // x has relevant Event
let y = x;            // y also has relevant Event

handle(y);
go loop();            // rejected: x still has relevant Event
```

```llg
let y = match x {
    A(v) => v,
    B(v) => v,
}; // copies each branch result place; rejected for linear branch results

let y <- match x {
    A(v) => v,
    B(v) => v,
}; // takes each branch result place
```

## LLG-MM-04 Produced Values Must Flow To Places

The flow-to-place rule prevents values from disappearing accidentally. A
non-unit expression result should flow into a place: a local place, field
place, argument place, return place, or the discard place `_`. A trusted marker
operation can then target that place. A bare expression statement flows into no
place, so allowing arbitrary non-unit expression statements would create an
implicit discard path outside the marker checker.

This rule is independent of marker modes. Even an unrestricted `u32` result
needs a receiving place, because silently throwing away non-unit values is
usually a bug or a missing effect boundary.

- Every produced non-unit value MUST flow into a place.
- A place receiving a produced value MAY be a local place, function argument
  place, return place, field place, or discard place `_`.
- An assignable place includes a place being initialized and a mutable place
  receiving a replacement value.
- A bare expression statement flows into no place and MUST be accepted only
  when the expression produces unit.

```llg
read_u32();  // rejected: unused non-unit value
log("done"); // accepted if log returns ()
```

## LLG-MM-05 The Discard Place

The discard place gives the language one explicit syntax for intentional
discard. It is more general than a separate `drop` operator because it also
works inside patterns. In a destructuring pattern, `_` means that the
corresponding component is intentionally sent to the discard place rather than
bound.

Discard is still checked. `_` is not a loophole around relevant or linear
obligations. It is only the explicit place for values whose current place
mode permits discard, or whose blocking obligations have been transformed or
moved elsewhere.

- `_` is the discard place.
- `_` MUST be a valid receiving place.
- `_` MUST NOT bind a name.
- `_` MUST NOT be readable by later code.
- Flowing a value to `_` MUST be treated as an explicit discard.
- `let _ = expr` MUST discard a produced value.
- `let _ <- place` MUST take an existing place and discard it.

```llg
let _ = read_u32();
let _ <- x;
```

- `_` MAY appear inside patterns, where it discards the corresponding
  component.

```llg
let Pair(keep, _) <- pair;
```

- Discarding through `_` MUST check marker modes.
- Discarding through `_` MUST accept unrestricted and affine values.
- Discarding through `_` MUST reject values whose current place mode is
  `relevant` or `linear`.

## LLG-MM-06 Function Parameters

Function calls are another place where copy and take must be visible in the
static interface. The signature has two separate parts: the parameter's
receiving place mode and the parameter's transfer behavior. The receiving mode
is written as part of the parameter's place type. The transfer behavior is
written with `take`, or implied by affine and linear parameter modes.

This separation matters because `Message with Event` is only a marker
requirement. It says the argument must carry the `Event` fact. It does not say
that the callee receives a relevant place. A signature that wants that
structural behavior must say `relevant Message with Event` or
`linear Message with Event`.

The call site should not need extra syntax to say that an argument is moved;
the function signature already owns that decision. Keeping that distinction in
signatures also gives the compiler a clear diagnostic site: if the caller uses
an argument after a moving call, the call is where the move happened.

The parameter syntax is:

```text
Param := take? name: Mode? Type
Mode := unrestricted | affine | relevant | linear
```

- Function signatures MUST express the receiving place mode for each
  parameter.
- A parameter whose mode is omitted MUST default to unrestricted mode.
- A parameter whose mode is affine or linear MUST receive a moved argument,
  even when `take` is omitted.
- A parameter written with `take` MUST receive a moved argument.
- `take` on an affine or linear parameter MUST be accepted as redundant.
- A parameter whose mode is unrestricted or relevant and which is not written
  with `take` MUST receive a copied argument.
- Call sites SHOULD NOT require an additional `take` marker for arguments
  passed to moving parameters.
- Marker requirements MUST NOT imply receiving place mode.
- A marker-qualified parameter with no explicit mode MUST still have
  unrestricted receiving mode.

```llg
fn inspect(value: Message) -> u32;
fn needs_event(value: Message with Event) -> u32;      // unrestricted, copied
fn remember(value: relevant Message with Event) -> (); // relevant, copied
fn handle(take value: relevant Message with Event) -> (); // relevant, moved
fn close(handle: linear Handle) -> ();       // linear, moved
fn close2(take handle: linear Handle) -> (); // accepted, redundant take

handle(message);
```

The source place mode must be compatible with the receiving parameter mode.
The receiving mode may add restrictions, but it must not drop restrictions
already present on the source place:

| source mode | allowed receiving modes |
| --- | --- |
| `unrestricted` | `unrestricted`, `affine`, `relevant`, `linear` |
| `affine` | `affine`, `linear` |
| `relevant` | `relevant`, `linear` |
| `linear` | `linear` |

- Passing an argument to a copying parameter MUST leave the caller's place live
  and copy the caller's current mode into the callee parameter place, subject
  to receiving-mode compatibility.
- Passing an argument to a moving parameter MUST make the caller's source place
  no longer live when the argument denotes a source place.
- Passing an argument whose current mode is affine or linear to a copying
  parameter MUST be rejected because affine and linear places cannot be copied.
- Passing an argument whose current mode is relevant to an unrestricted
  parameter MUST be rejected while the source remains relevant, because that
  would weaken a non-discardable source to an unrestricted receiver.
- Passing an unrestricted argument to a linear parameter MUST be accepted and
  must move the source into a linear receiving place.
- If later code tries to use a place taken by a call, the diagnostic SHOULD
  point to the call that took the place.

## LLG-MM-07 Trusted Marker Operations

Marker modes decide whether obligations can be copied, moved, or discarded.
They do not by themselves prove that an obligation has been semantically
fulfilled. That semantic boundary is represented by trusted marker operations.

`mark`, `use`, and `consume` are trusted checker-state operations on places.
They update the current proof and mode state for a live place/version. This is
the right default for marker facts because many facts are learned by observing a
boolean condition or an operation result and then marking an input place. For
example, observing `index < array.length` should add `LessThan(index,
array.length)` to the existing `index` place on the continuing path.

These operations are side effects in the checker, not runtime mutation. They do
not create a new runtime value, and they do not change older copied places. A
copy made before a marker operation keeps the facts and mode it had at the copy
point.

`mark` introduces a marker fact into the checker and merges that marker type's
base mode into the target place mode. `use` discharges the "must be used" part
of a relevant place mode by changing the target place to unrestricted mode.
`consume` discharges the "must be consumed" part of a linear place mode by
changing the target place to affine mode. `use` and `consume` do not add,
remove, or change marker facts. Keeping these operations trusted prevents
ordinary safe code from simply asserting that important resource obligations
were satisfied.

- Introducing a marker fact MUST be a trusted operation.
- Transforming a relevant place mode into unrestricted mode MUST be a trusted
  operation.
- Transforming a linear place mode into affine mode MUST be a trusted
  operation.
- Safe code MAY move, copy when allowed, require markers, and pass marker facts
  through checked operations.
- Safe code MUST NOT introduce or transform marker contracts directly.
- `Marker::mark(place)` MUST target an existing live place.
- `Marker::mark(place)` asserts that the marker contract is true for the target
  place.
- `Marker::mark(place)` MUST add the marker fact to the target place in the
  current checker environment.
- `Marker::mark(place)` MUST merge the marker type's base structural mode into
  the target place's current mode.
- `Marker::mark(place)` MUST NOT create a new runtime value.
- `use(place)` asserts that a relevant obligation represented by the target
  place mode has been meaningfully used.
- `use(place)` MUST require the target place to have relevant current mode.
- `use(place)` MUST update the target place's current mode to unrestricted.
- `use(place)` MUST preserve the target place's marker facts.
- `use(place)` MUST NOT add, remove, or change any marker fact.
- `use(place)` MUST be rejected when the target place's current mode is not
  `relevant`.
- `consume(place)` asserts that a linear obligation represented by the target
  place mode has been meaningfully consumed.
- `consume(place)` MUST require the target place to have linear current mode.
- `consume(place)` MUST update the target place's current mode to affine.
- `consume(place)` MUST preserve the target place's marker facts.
- `consume(place)` MUST NOT add, remove, or change any marker fact.
- `consume(place)` MUST be rejected when the target place's current mode is not
  `linear`.
- After `use(place)` or `consume(place)`, the target place MUST still satisfy
  ordinary requirements for all marker facts preserved on that place.
- Trusted marker operations MUST NOT make older copied places inherit the
  updated facts or mode.

```llg
unsafe { Event::mark(value); }
unsafe { use(value); }

unsafe { Resource::mark(resource); }
unsafe { consume(resource); }
```

- Marker implementations for functions and operators MAY use trusted marker
  operations to describe marker relationships between inputs and results.
- A marker implementation that proves a fact about an input place SHOULD mark
  that input place directly.
- A marker implementation that observes a relevant input obligation SHOULD use
  that input place.
- A marker implementation that consumes a linear input obligation SHOULD
  consume that input place.
- A marker implementation that creates a new obligation on a result SHOULD mark
  the result place, adding the marker fact and merging the marker type's base
  mode into the result place mode.

## LLG-MM-08 Composite Structural Summaries

Composites make place modes structural. A struct value may not have a
restrictive mode directly from a marker on the outer place, but one of its
fields might have relevant or linear current mode. Copying or discarding the
outer place must respect the modes reachable through its fields.

The summary must be derived from the current place state rather than only from
the declared type. After a field is taken out, the outer place no longer owns
that field's obligation. After assignment or construction, new obligations may
appear. This is why composite summaries are compiler-derived dataflow facts,
not user-written annotations.

- Composite places MUST surface the current structural modes of their reachable
  parts.
- The compiler MUST derive composite structural summaries automatically.
- Users MUST NOT be required to write composite structural summaries by hand.
- A composite structural summary MUST describe the current state of a place,
  not only the declared type of that place.
- Composite structural summaries MUST be updated or replaced after operations
  that change ownership of reachable parts.
- Relevant operations include construction, field projection, field take,
  assignment, match destructuring, and pattern binding.

For example:

```llg
struct Packet {
    id: u32,
    payload: Message with Event,
}

let packet = make_packet();
// packet has a compiler-derived summary for packet.payload's current mode

let payload <- packet.payload;
// packet no longer owns payload's mode or marker facts
```

- The compiler MAY represent a summary as a hidden mode fact such as
  `ContainsMode(packet, packet.payload, relevant)`.
- The compiler MAY represent a summary as a diagnostic-oriented fact such as
  `DropBlocked(packet, packet.payload with Event)`.
- The chosen representation MUST allow copy, take, discard, and implicit-discard
  checks to account for reachable place modes.
- A composite place MAY be copied only when every reachable part is copyable.
- A composite place MAY be implicitly discarded only when every reachable part
  is discardable.
- A reachable part with affine or linear current mode MUST make the containing
  place non-copyable.
- A reachable part with relevant or linear current mode MUST make the
  containing place non-discardable until that part's mode is transformed to a
  discardable mode or that part is moved elsewhere.
- A place with both non-copyable and non-discardable reachable obligations MUST
  behave structurally like a linear place for copy and discard checks.
- Diagnostics for composite failures SHOULD identify the reachable part that
  blocks the operation.

```text
cannot discard `packet`
`packet.payload` still has relevant mode introduced by marker `Event`
```

## LLG-MM-09 Copy, Dupe, And Composite Values

Implicit copy should remain cheap and predictable. Scalar values can usually
support that by default. Composite values are more expensive and may contain
obligations hidden inside fields, so they should not become implicitly copyable
unless the type explicitly supports it.

An explicit duplication operation gives the language a place for more
expensive clone-like behavior without weakening the copy rules. It may
duplicate values whose current modes are copyable, but it cannot duplicate
values whose current modes are affine or linear.

- Scalar values MAY be implicitly copyable by default.
- Composite values such as structs and enums SHOULD NOT be implicitly copyable
  unless their type explicitly supports copying.
- An explicit duplication operation such as `dupe` MAY be provided for values
  that can be duplicated without implicit copy syntax.
- `dupe` MUST be rejected if the value, or any reachable member of the value,
  has affine or linear current mode.
- `dupe` MAY duplicate values with relevant or unrestricted mode, preserving
  the source value's marker facts and current mode at the moment of
  duplication.
- `dupe` MUST leave the source value's marker facts and current mode unchanged.

## LLG-MM-10 Arithmetic And Operators

Arithmetic is specified as copying its operands. This keeps ordinary
operators simple and avoids treating expression syntax as if it transferred
ownership by default. A linear value cannot be used in `x + 7` because `+`
would need to copy `x`.

If an operator has meaningful marker behavior, that behavior belongs in the
operator's marker implementation. The expression tree does not infer that a
result inherits markers or placeness from one of its operands.

- Arithmetic operators MUST copy their operands.
- Arithmetic operators MUST reject operands whose place occurrence has affine
  or linear current mode.
- Arithmetic expressions MUST NOT make their produced result inherit placeness
  from their operands.
- Marker relationships between copied operands and produced results MUST be
  expressed by the function or operator marker implementation, not inferred
  from expression shape.

```llg
let y = x + 7;
```

In this example, `x + 7` is not a place. The place occurrence `x` is checked as
an operand to `+`. Because `+` copies its operands, this expression is rejected
if `x` has affine or linear current mode. If the operand checks succeed, the
result of `x + 7` is a produced value and may flow to an ordinary receiving
place such as `y`.

## LLG-MM-11 Implicit Discard Points

Explicit discard through `_` is only part of the story. Programs also discard
values implicitly when scopes end, when a mutable place is overwritten, and when
control flow leaves a region. Those implicit points must be visible to the
marker checker, otherwise relevant and linear obligations could disappear
without any explicit source syntax.

This is where the place-based design becomes a dataflow check. The compiler
tracks which places are live at each control-flow point and rejects any path
that would implicitly discard a place whose current mode is relevant or linear.

- The marker checker MUST account for implicit discards.
- End of scope MUST discard live places that have not been moved elsewhere.
- Assignment overwrite MUST discard the previous value of the overwritten
  place.
- `return`, `exit`, and `go` MUST either move or discard all places that cease
  to be live across the control transfer.
- Implicitly discarding a place whose current mode is relevant or linear MUST be
  rejected.
- The compiler MUST accept the implicit discard after the blocking place mode
  has been transformed to a discardable mode, moved out of the place, or
  otherwise discharged by a trusted marker implementation.

## LLG-MM-12 Diagnostics

Marker-mode errors will often be separated from their cause. A discard failure
may happen at the end of a block even though the marker was introduced much
earlier. A use-after-take error may happen after a function call whose
signature moved the value. Good diagnostics therefore need provenance, not just
a final failed operation.

The checker should preserve enough history to explain the obligation chain in
place terms: where the marker came from, where it was copied or moved, and why
the current operation is not allowed.

- Marker-mode diagnostics SHOULD preserve provenance for marker facts and place
  mode changes.
- Provenance SHOULD include where a marker fact was introduced, where a place
  mode was copied, transformed by `use`, transformed by `consume`, taken, moved
  into a composite, moved out of a composite, or implicitly discarded.
- When a later use fails because a place was taken, the diagnostic SHOULD point
  to the earlier take.
- When a discard fails because the place's current mode is relevant or linear,
  the diagnostic SHOULD point to the operation that performs the discard and to
  the marker or mode provenance that made the place non-discardable.
