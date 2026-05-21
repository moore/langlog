# Langlog Type System Specification

Status: draft 0. This document describes the full static type system model for
Langlog: concrete value types, places, produced values, marker facts, and
structural marker modes.

Normative terms in this document follow RFC 2119.

This document complements, but does not replace, the other Langlog specs:

- [SPEC.md](./SPEC.md) defines surface syntax, diagnostics, and front-end
  behavior.
- [SEMANTICS.md](./SEMANTICS.md) defines broader static and dynamic semantics.
- [HIR.md](./HIR.md) defines the compiler-facing semantic IR boundary.
- [PROOF_IR.md](./PROOF_IR.md) defines the proof-specific IR boundary.
- [MARKER_MODES.md](./MARKER_MODES.md) defines the detailed structural mode
  rules for copy, take, discard, and trusted mode transformation.

The prose explains the design intent and motivation. Requirement bullets define
the normative behavior.

## Problem

Langlog needs a type system that does more than classify runtime values as
`u32`, `String`, `Result<T, E>`, or a user-defined composite type. The language
also needs to track facts that have been proven about a specific place in the
program, such as "this index is less than this array length" or "this key is a
member of this map".

Those facts are marker facts. They are erased before executable lowering, but
they affect whether source programs type check. This gives Langlog a lightweight
way to express refinement-like properties without requiring arbitrary theorem
proving.

Some marker facts also carry resource obligations. `LessThan(index,
array.length)` is an unrestricted proof fact: it can be copied or forgotten.
`Event()` is different. A task should not be able to receive an event and then
continue a cycle without using that event. Future resource and capability
markers may need even stricter behavior. The type system therefore has to track
copy and discard permissions as well as proof facts.

The design goal is a local, finite, function-scale analysis:

- concrete value types catch ordinary type errors;
- marker facts express checked proof obligations;
- structural modes express copy and discard obligations;
- places give facts and modes a stable identity;
- produced values remain lightweight expression results until they flow into a
  place.

This is intentionally not a full dependent type system. Marker facts may refer
to places, but the checker discharges obligations by direct facts and explicit
companion rules, not arbitrary proof search or global constraint solving.

## Design Overview

The Langlog type system has four layers:

```text
place
  is a source-level or IR-level storage identity
  has value state
  has current structural mode
  may contain subplaces

value
  has runtime payload/storage
  has concrete type
  has markings

marking / marker fact
  is an instance of a marker type
  has proof shape

marker type
  defines marker shape
  carries a mode obligation
```

Concrete types describe runtime representation and ordinary operation
compatibility. Markings, also called marker facts, describe static facts known
about a value at a place. A marker type's mode obligation contributes to the
current structural mode of a place when the value is placed. The structural
mode then controls whether that place can be copied or discarded.

This separation is the main design choice. A value can be produced by an
expression without immediately being a place. A place is the thing that can be
read again, copied, taken, projected, overwritten, or diagnosed. Every non-unit
produced value must eventually flow into a place, but intermediate expression
results do not need hidden source-level identities.

For example:

```llg
let x = read_u32();  // `read_u32()` produces a value that flows into place x
let y = x;           // x is a place, so this copies from x into y
let z <- x;          // x is a place, so this moves from x into z
```

`read_u32()` is a produced value. It is not copied from an anonymous source
place. By contrast, `x` is an existing place, so `=` and `<-` have visible
copy-or-move meaning.

## LLG-TS-01 Concrete Value Types

Concrete value types are the ordinary type layer. They classify runtime payloads
and determine which operations are valid before marker facts are considered.

The early language includes scalar types such as `u32` and `bool`, unit `()`,
tuples, fixed arrays, builtin `Option<T>` and `Result<T, E>`, bounded
collections such as `Set<T, N>` and `Map<K, V, N>`, and future user-defined
structs and enums.

Concrete type checking keeps Langlog predictable. Numeric operators do not
perform implicit promotion. Ordinary checked arithmetic on `u32` returns
`Result<u32, ArithmeticError>`. This means failures are visible in the type
system instead of being hidden as wrapping behavior or implicit exceptions.

This may look inconsistent with the marker system at first. Langlog could make
every arithmetic operation require a proof fact saying that overflow,
underflow, divide-by-zero, or remainder-by-zero cannot happen. That is not the
default because ordinary checked arithmetic is usually the simpler and cheaper
operation on modern CPUs. The machine can compute the value and detect failure
with direct arithmetic status or a small branch. Dispatching a marker
obligation for every ordinary arithmetic expression would make common code more
complicated, would push more programs into proof engineering, and would not
usually buy enough performance to justify that complexity.

Proof-backed arithmetic is still useful when the programmer deliberately wants
the static guarantee. That should be explicit at the operation site, not
inferred for ordinary arithmetic. The default rule is therefore checked
arithmetic first, with future raw or proof-backed arithmetic reserved for
spelled-out operations.

- Every expression MUST have a concrete value type after successful semantic
  checking.
- Every live place MUST hold a value with a concrete value type.
- Let annotations, assignments, returns, and call arguments MUST be compatible
  with the declared concrete type expected by the receiving place.
- Unknown concrete types MUST prevent successful HIR lowering.
- Marker facts MUST NOT change the runtime representation of a concrete value
  type.
- Marker facts MUST be erased before executable lowering after all marker and
  mode obligations have been checked.
- Numeric operators MUST NOT perform implicit numeric promotion.
- Ordinary checked arithmetic over `u32` MUST produce
  `Result<u32, ArithmeticError>`.

```llg
let count: u32 = 0;
let next = count + 1; // Result<u32, ArithmeticError>
```

The value type of `next` is determined by the arithmetic rule. Marker facts may
later prove something about the successful payload, but they do not change the
fact that checked arithmetic is fallible.

## LLG-TS-02 Places

A place is the type system's storage identity. Places are where marker facts and
structural modes become stable checker state.

Using places solves a problem that plain expression typing does not solve:
facts such as `LessThan(index, array.length)` are about a particular version of
`index` and a particular version of `array.length`. If either place changes,
the old fact must not silently apply to the new value. The compiler therefore
tracks places through source names, projections, and SSA versions rather than
through identifier text alone.

- A place MUST be a source-level or IR-level storage identity.
- A place MAY be named or unnamed.
- A place MAY contain subplaces.
- A place MUST have value state.
- A place MUST have a current structural mode.
- A place MUST be able to carry marker facts for the value it currently owns.
- Source-level places include local variables, function arguments, fields,
  projections, enum payload positions, and pattern bindings visible in source.
- IR-level places include return slots, call or parameter slots, projections,
  and SSA temporaries materialized by lowering.
- Mutating a source place MUST create a fresh SSA place/version for the new
  value state.
- Marker facts attached to an older SSA place/version MUST NOT automatically
  apply to a newer SSA place/version.
- The compiler SHOULD introduce an IR-level place for an intermediate
  expression result only when that result must be tracked as storage
  independently for copy, move, discard, projection, or diagnostics.

```llg
let key: UserId with MemberOf(key, users) = users.require_key(raw_id);

users.insert(other_key, other_value);
let value = users[key]; // rejected if the mutation changed users' SSA place
```

The marker fact proves membership in the older `users` place/version. It does
not prove membership in the later version unless a stable-fact rule or companion
rule preserves that fact explicitly.

## LLG-TS-03 Produced Values

A produced value is what an expression produces before or while it flows into a
place. It has runtime payload or storage, a concrete type, and zero or more
marker facts. It does not have a source-level place mode by itself.

The payload/storage wording is intentionally broad. It does not imply heap
allocation. It only means the value has some runtime representation: it might
live in a register, a stack slot, a return slot, or later storage chosen by the
lowering pipeline.

Distinguishing produced values from places prevents expression trees from
accidentally inheriting storage behavior. `x + 7` is not a place just because
`x` is a place. The occurrence of `x` is checked according to the operator's
operand rules, then the arithmetic expression produces a new value.

- A produced value MUST have a concrete value type.
- A produced value MAY carry marker facts.
- A produced value MUST NOT have a source-level place identity.
- A produced value MUST NOT have a source-level place mode.
- A produced value MAY carry a mode contribution summary that is used when the
  value flows into a place.
- A produced non-unit value MUST flow into a place.
- A bare expression statement MUST be accepted only when the expression
  produces unit.
- A produced value MUST NOT be copied, moved, discarded, projected, or diagnosed
  as storage until it flows into a place or lowering materializes an IR-level
  place for it.

```llg
read_u32();        // rejected: non-unit value has no receiving place
let x = read_u32(); // accepted: produced value flows into place x
```

The rule is not special to event-like values. Requiring every non-unit value to
have a receiving place avoids accidental loss of values and gives marker-mode
checking one consistent place to observe discards.

## LLG-TS-04 Marker Types, Markings, And Marker Facts

A marker type defines the shape of a fact and the structural mode obligation
associated with facts of that type. A marking, or marker fact, is an instance
of a marker type attached to a value at a place.

This gives Langlog a refinement-like layer without making every concrete type
dependent on arbitrary terms. `u32 with LessThan(index, array.length)` is still
a `u32` at runtime, but the type checker knows an additional compile-time fact
about the place holding that value.

- A marker type MUST have a name.
- A marker type MUST declare its marker parameters.
- Marker parameters that refer to program storage MUST be `place` parameters.
- A marker type MUST carry exactly one base structural mode obligation.
- A marker declaration without an explicit structural mode MUST default to
  `unrestricted`.
- The terms `marking` and `marker fact` MUST refer to the same checker object.
- A marker fact MUST be an instance of a marker type.
- A marker fact MUST have proof shape determined by its marker type and
  arguments.
- A marker fact MUST NOT have an independently mutable structural mode.
- A marker-qualified type or requirement MUST require the corresponding marker
  fact to be present.
- A value with extra marker facts MAY be used where those facts are not
  required, subject to structural mode checks.
- A value without a required marker fact MUST NOT be used where that marker fact
  is required.

```llg
marker LessThan(left: place, right: place) : unrestricted;
marker MemberOf(key: place, map: place) : unrestricted;
marker Event() : relevant;
```

`LessThan` and `MemberOf` are ordinary proof facts. They are useful for proving
operation preconditions, but forgetting them does not change resource behavior.
`Event` is relevant because silently dropping an event-carrying place would let
a task receive input without accounting for it.

## LLG-TS-05 Marker-Qualified Types And Requirements

Marker-qualified type syntax combines a concrete type with required marker
facts. It should be read as "a value of concrete type `T` at a place where these
marker facts are known."

The marker list is a requirement at a boundary, not a runtime field list. This
is why marker-qualified arguments can accept values with extra facts, while
function returns are stricter: a function can only promise to return marker
facts it names in its signature.

Eliding an extra marker fact is not the same thing as discharging a structural
mode. A call that does not require `Event` does not need the `Event` fact for
ordinary marker matching, but the argument place's current mode is still checked
by the copy or move context at the call boundary.

The same distinction applies to returns. A function that returns plain `String`
does not promise `Event` to the caller. If the returned value still carries a
restrictive mode caused by `Event`, the function must transform or move that
obligation rather than hiding it by omitting the marker from the return type.

- `T with Marker` MUST mean concrete type `T` plus a requirement for marker
  fact `Marker` on the relevant place.
- `T with (A, B, ...)` MUST require every listed marker fact.
- A marker-qualified parameter MUST create a call-site obligation for each
  required marker fact.
- A marker-qualified return type MUST require the returned value to provide
  every named marker fact.
- Function return values MUST carry only the marker facts named by the function
  signature.
- Omitting a marker fact from a return type MUST NOT bypass structural mode
  checks for the returned value.
- If a function preserves or creates a marker fact across a call boundary, that
  marker fact MUST appear in the return type.
- Generic type parameters MUST NOT capture unmentioned marker facts from an
  argument.

```llg
marker FromCache() : unrestricted;

fn len(input: String) -> u32;

let line: String with FromCache = cache.read();
let length = len(line); // marker fact not required by len; mode still checked
```

The concrete type requirement is just `String`, so the marker fact is not needed
by `len`. Structural mode still matters independently. If `line` still has a
restrictive current mode, an unrestricted parameter cannot receive it merely
because the marker fact was elided from the concrete requirement.

```llg
fn trim(input: String) -> String;

let line: String with FromCache = cache.read();
let trimmed = trim(line); // String, not String with FromCache
```

The result is plain `String` because `trim` did not promise to preserve
`FromCache`. This keeps marker propagation explicit at function boundaries.

## LLG-TS-06 Mode-Annotated Place Types

Some places are created at public or separately checked boundaries: function
parameters, task parameters, state parameters, task fields, and return slots.
Those boundaries cannot rely on a caller's local inference. The signature must
state the receiving place mode so the function or task can be checked
independently.

Langlog therefore has place type annotations:

```text
PlaceType := Mode? Type
Mode := unrestricted | affine | relevant | linear
Param := take? name: PlaceType
Return := -> PlaceType
```

The mode annotates the receiving place, not the runtime value type. In
`linear Handle`, `Handle` is still the concrete type. `linear` says that the
place receiving the handle starts with linear structural behavior.

Omitted modes are deliberately conservative at API boundaries. A parameter
written `Message with Event` requires the `Event` fact, but it is still an
unrestricted receiving place unless the signature says `relevant Message with
Event` or `linear Message with Event`. This prevents marker requirements from
silently changing ownership behavior.

Local `let` bindings are different because the initializer is visible in the
same checking unit. When the mode is omitted, the compiler can infer the new
place mode from the initializer. A local annotation can also intentionally
strengthen a place:

```llg
let ordinary = 7;             // inferred unrestricted
let token: linear u32 = 7;    // explicit strengthening
```

Explicit modes may preserve or strengthen restrictions, but they must not
weaken restrictions already present on a source place. This is the compatibility
rule for every receiving boundary:

| source mode | allowed receiving modes |
| --- | --- |
| `unrestricted` | `unrestricted`, `affine`, `relevant`, `linear` |
| `affine` | `affine`, `linear` |
| `relevant` | `relevant`, `linear` |
| `linear` | `linear` |

The table is easiest to read as preserving structural restrictions. A
non-copyable source must flow into a non-copyable receiving place. A
non-discardable source must flow into a non-discardable receiving place. A
receiving place may add restrictions, but it cannot make a restrictive source
look unrestricted.

- Function parameters, task parameters, state parameters, task fields, and
  return slots MUST use place type annotations.
- An omitted structural mode in a function parameter, task parameter, state
  parameter, task field, or return slot MUST default to `unrestricted`.
- Local `let` bindings MAY omit a mode and infer the receiving place mode from
  the initializer.
- Local `let` bindings MAY include an explicit mode.
- An explicit local mode MAY strengthen an unrestricted initializer.
- An explicit receiving mode MUST NOT weaken the current mode of an incoming
  source place.
- Marker requirements such as `T with Event` MUST remain separate from place
  mode annotations.
- A marker-qualified parameter with no explicit structural mode MUST have
  unrestricted parameter mode.
- Mode keywords MUST be contextual and MUST NOT prevent values or types from
  using the same names outside mode positions.

```llg
fn inspect(value: Message with Event) -> u32;
fn handle(value: relevant Message with Event) -> ();
fn close(handle: linear Handle) -> ();
fn open() -> linear Handle;
```

`inspect` requires the `Event` fact, but its parameter mode is unrestricted.
A caller whose message place is still relevant cannot pass that place to
`inspect` without first transforming or otherwise moving the obligation.
`handle` can receive a relevant place because the mode is stated explicitly.

## LLG-TS-07 Place Modes

A place's current structural mode is checker state for that place/version. It
controls whether the place may be copied and whether it may be discarded.

The modes correspond to structural permissions:

| mode | copy | discard | meaning |
| --- | --- | --- | --- |
| `unrestricted` | allowed | allowed | ordinary fact/value behavior |
| `affine` | rejected | allowed | may be used at most once |
| `relevant` | allowed | rejected | must be used at least once |
| `linear` | rejected | rejected | must be used exactly once |

The mode lives on the place, not on each marker fact. A marker type contributes
a mode obligation when a marking operation records a fact on a place, or when a
produced value with marker facts flows into a receiving place. The receiving
place stores the current merged mode for that value state.

This preserves a simple user model. Users do not have to reason about hidden
marker-instance identities. They reason about live places: this place can or
cannot be copied, and this place can or cannot be discarded.

- Every live place MUST have a current structural mode.
- A fresh place with no restrictive contribution MUST begin unrestricted.
- A place whose current mode is affine or linear MUST be non-copyable.
- A place whose current mode is relevant or linear MUST be non-discardable.
- A place with both a non-copyable obligation and a non-discardable obligation
  MUST behave structurally as linear.
- Place mode MUST NOT affect ordinary marker fact matching.
- Place mode MUST affect copy, move, overwrite, discard, return, exit, and `go`
  checking.

## LLG-TS-08 Mode Computation And Merge

The current mode of a place is computed from how a value reaches that place.
This is the bridge between values, marker facts, and places.

If a value is copied from an existing place, the receiving place gets the source
place's marker facts and current mode as they were at the copy point. Later
`use` or `consume` operations on the source do not update the copy. If a value
is moved from an existing place, the receiving place gets the source facts and
mode, and the source place becomes unavailable. If a value comes from a
non-place expression, the receiving place's mode is computed from the value's
marker facts and any moved subvalue contributions.

The merge rule is the high-water mark of structural restrictions:
`affine` contributes "cannot copy"; `relevant` contributes "cannot discard";
both restrictions together behave as `linear`.

| left | right | merged place mode |
| --- | --- | --- |
| `unrestricted` | `M` | `M` |
| `affine` | `affine` | `affine` |
| `relevant` | `relevant` | `relevant` |
| `affine` | `relevant` | `linear` |
| `affine` | `linear` | `linear` |
| `relevant` | `linear` | `linear` |
| `linear` | `M` | `linear` |

- Copying a source place MUST copy the source place's current mode to the
  receiving place.
- Copying a source place MUST copy the source place's marker facts to the
  receiving place.
- Moving a source place MUST transfer the source place's current mode and
  marker facts to the receiving place.
- After a move, the source place MUST be unavailable until it is assigned a new
  value.
- Placing a non-place expression result MUST compute the receiving place mode
  by merging mode obligations contributed by the value's marker facts and moved
  subvalues.
- Mode merge MUST be commutative and associative.
- An unrestricted contribution MUST add no copy or discard restriction.
- An affine contribution MUST make the merged mode non-copyable.
- A relevant contribution MUST make the merged mode non-discardable.
- A value that has both non-copyable and non-discardable contributions MUST
  receive linear mode when placed.

```llg
let event = read_event();
unsafe { Event::mark(event); }
let copy = event;
unsafe { Structural::use(event); }

go loop(); // rejected if copy is still live with relevant mode
```

`copy` was made before `event` was updated by `use`, so it keeps the mode that
existed at the copy point.

```llg
let event = read_event();
unsafe { Event::mark(event); }
unsafe { Structural::use(event); }
let copy = event; // copy receives unrestricted mode
```

Here the copy occurs after the mode transformation, so the copied mode is
unrestricted.

## LLG-TS-09 Copy, Move, And Receiving Places

Assignment and binding always put a value into a receiving place. The left-hand
side must be a place or a pattern whose binding positions are places. The
right-hand side may be either an existing source place or a produced value.

`=` creates a copy context. `<-` creates a move context. Those contexts only
copy or move an existing source place when the right-hand side denotes a place.
When the right-hand side is a produced value, there is no source place to copy
or take.

- The left-hand side of `=` MUST be a receiving place or a pattern of receiving
  places.
- The left-hand side of `<-` MUST be a receiving place or a pattern of receiving
  places.
- `=` MUST place the right-hand side result using a copy context.
- `<-` MUST place the right-hand side result using a move context.
- If the right-hand side result of `=` is an existing place, that source place
  MUST be copied.
- If the right-hand side result of `<-` is an existing place, that source place
  MUST be moved.
- If the right-hand side result is a produced value, the result MUST flow
  directly into the receiving place without creating an additional copy or move
  of an anonymous place.
- Copying a place whose current mode is affine or linear MUST be rejected.
- Moving a place MUST be allowed for all structural modes, subject to ordinary
  liveness and type checks.

```llg
let x = read_u32();  // produced value flows into x
let y = x;           // copy from source place x
let z <- x;          // move from source place x
let w <- read_u32(); // produced value flows into w
```

The annoyance of writing `<-` for source-place moves buys a clear distinction:
plain `=` never secretly consumes a source place.

## LLG-TS-10 Function Boundaries

Function parameters are receiving places with two separate pieces of signature
information: a structural mode and a transfer rule. The structural mode says
what kind of place the callee receives. The transfer rule says whether the
argument is copied or moved from the caller.

`take` always means move. Affine and linear parameter modes also imply move,
even when `take` is omitted, because copying into those parameter places would
violate the caller-side source restrictions for affine or linear values and
would make explicit linear receiving places awkward to use. `take` on an affine
or linear parameter is accepted but redundant.

Unrestricted and relevant parameters copy by default. They move only when
`take` is written. This keeps ordinary inspection cheap while still allowing a
relevant argument to be consumed by a callee when the API says so.

This keeps ownership behavior visible in the API and gives diagnostics a clear
source location. If a caller tries to use a value after a moving call, the
compiler can point to the call that moved the place.

- Function signatures MUST express the receiving mode of each parameter.
- A parameter written without an explicit mode MUST default to unrestricted
  mode.
- A parameter written with `take` MUST receive its argument in a move context.
- A parameter with affine or linear mode MUST receive its argument in a move
  context even when `take` is omitted.
- `take` on an affine or linear parameter MUST be accepted as redundant.
- A parameter with unrestricted or relevant mode and no `take` MUST receive its
  argument in a copy context.
- Call sites SHOULD NOT require an additional `take` marker for arguments
  passed to moving parameters.
- Passing an argument to a moving parameter MUST make the caller's source place
  unavailable until reassignment when the argument denotes a source place.
- Passing an argument to a copying parameter MUST leave the caller's source
  place live when the argument denotes a source place.
- The source mode and parameter mode MUST satisfy the receiving-mode
  compatibility table from LLG-TS-06.
- Function return signatures MUST express the receiving mode of the return
  slot.
- An omitted return mode MUST default to unrestricted.
- Return expressions MUST flow into the return slot place and satisfy the
  return slot's declared mode.
- The value produced by a call MUST expose the function's return slot mode to
  the caller's receiving boundary.

```llg
fn inspect(value: Message) -> u32;           // unrestricted, copied
fn remember(value: relevant Message) -> ();  // relevant, copied
fn handle(take value: relevant Message) -> (); // relevant, moved
fn close(handle: linear Handle) -> ();       // linear, moved
fn close2(take handle: linear Handle) -> (); // accepted, redundant take

handle(message);
inspect(message); // rejected: message was moved by handle
```

The signature, not extra call-site syntax, explains why `message` is no longer
available.

## LLG-TS-11 Trusted Marker Operations

Marker facts and structural modes cross a trust boundary. Safe code may use
facts, require facts, copy copyable places, and move places. Safe code must not
be able to simply assert that a marker fact is true or that a resource
obligation has been satisfied.

`mark`, `use`, and `consume` are trusted checker-state operations on places.
They intentionally affect the current place rather than returning a replacement
value. This matters because many useful marker facts are learned by observing a
boolean condition or an operation result and then marking one of the operation's
input places. For example, observing `index < array.length` should mark the
existing `index` place with `LessThan(index, array.length)` on the continuing
path; it should not require creating a new `index` value.

These operations are side effects in the type checker, not runtime mutation.
They update the current proof and mode state for a live place/version. If the
place is copied before the update, the earlier copy keeps the facts and mode it
had at the copy point. Later updates do not retroactively affect earlier
copies.

- Introducing a marker fact MUST be a trusted operation.
- Transforming a relevant mode into unrestricted mode MUST be a trusted
  operation.
- Transforming a linear mode into affine mode MUST be a trusted operation.
- `Marker::mark(place)` MUST target an existing live place.
- `Marker::mark(place)` MUST add the marker fact to the target place in the
  current checker environment.
- `Marker::mark(place)` MUST merge the marker type's base mode obligation into
  the target place's current mode.
- `Marker::mark(place)` MUST NOT create a new runtime value.
- `Structural::use(place)` MUST require the target place to have relevant
  current mode.
- `Structural::use(place)` MUST update the target place's current mode to
  unrestricted.
- `Structural::consume(place)` MUST require the target place to have linear
  current mode.
- `Structural::consume(place)` MUST update the target place's current mode to
  affine.
- `Structural::use(place)` and `Structural::consume(place)` MUST preserve
  marker facts on the target place.
- `Structural::use(place)` and `Structural::consume(place)` MUST NOT add,
  remove, or change marker facts.
- Trusted marker operations MUST NOT make older copied places inherit the
  updated facts or mode.

```llg
let event = read_event();
unsafe { Event::mark(event); }
unsafe { Structural::use(event); }

let resource = open_resource();
unsafe { Resource::mark(resource); }
unsafe { Structural::consume(resource); }
```

The marker facts remain facts. The trusted operations change the checker state
of the target place so later copy and discard checks see the updated mode.

## LLG-TS-12 Operators And Expression Contexts

Expression contexts determine how place occurrences inside expressions are
used. Arithmetic operators copy their operands. Therefore a linear or affine
place cannot be used as an arithmetic operand unless it has first been moved or
transformed in a way the operator accepts.

The result of an operator expression is a produced value, not a place. It does
not inherit placeness from its operands. Marker relationships between operands
and results must be expressed by operator marker implementations or companion
rules.

- Arithmetic operators MUST copy their operands.
- Arithmetic operators MUST reject operand place occurrences whose current mode
  is affine or linear.
- An expression result MUST NOT inherit placeness from place occurrences inside
  the expression.
- Marker facts on an operator result MUST be produced only by the operator's
  marker implementation, a builtin semantic rule, or a companion marker rule.
- If no marker rule applies, an operator MUST NOT preserve input marker facts
  onto the result by default.

```llg
let y = x + 7;
```

This fails if `x` has affine or linear current mode because `+` copies its
operands. It does not fail because `x + 7` inherited placeness from `x`.

## LLG-TS-13 Composite Values And Subplaces

Composite values make the place hierarchy visible. A struct place contains
field places. An enum place contains the active variant's payload places. A
tuple place contains element places. Each reachable subplace can carry marker
facts and a current structural mode.

The containing place must surface restrictive subplace modes. Otherwise a
program could hide a relevant or linear obligation inside a struct and then
drop the outer value.

The summary is about current place state, not just declared type. Moving a
field out, assigning a field, constructing a value, or destructuring a value can
change which place owns a restrictive obligation.

- Composite places MUST contain subplaces for their fields, elements, or active
  payload positions.
- Composite structural summaries MUST be derived by the compiler.
- Users MUST NOT be required to write composite structural summaries by hand.
- A composite place MUST be copyable only when every reachable owned subplace is
  copyable and the composite type itself supports copying.
- A composite place MUST be discardable only when every reachable owned
  subplace is discardable.
- A reachable affine or linear subplace MUST make the containing place
  non-copyable.
- A reachable relevant or linear subplace MUST make the containing place
  non-discardable.
- Taking a field or payload out of a composite MUST update which place owns the
  moved facts and mode.
- Diagnostics for composite copy or discard failures SHOULD identify the
  reachable subplace that blocks the operation.

```llg
struct Packet {
    id: u32,
    payload: Message with Event,
}

let packet = make_packet();
go loop(); // rejected if packet.payload still has relevant mode

let payload <- packet.payload;
unsafe { Structural::use(payload); }
```

The outer `packet` place is blocked because it owns a subplace with relevant
mode. After moving the payload out, that obligation belongs to `payload`
instead.

## LLG-TS-14 Copy And Dupe

Implicit copy should be cheap and predictable. Scalars can usually support it
by default. Composite values may be expensive to duplicate and may hide
subplaces with restrictive modes, so implicit copy should be more conservative
for composites.

The explicit duplication operation is intentionally separate from `=`.
`=` copies only when the type and current mode permit implicit copy. `dupe`
occupies the clone-like space: it may be more expensive, but it still cannot
duplicate values whose current modes are non-copyable.

- Scalar concrete types MAY be implicitly copyable by default.
- Composite concrete types SHOULD NOT be implicitly copyable unless the type
  explicitly supports copying.
- An explicit duplication operation such as `dupe` MAY be provided for values
  that support clone-like duplication.
- `dupe` MUST be rejected when the value or any reachable owned subplace has
  affine or linear current mode.
- `dupe` MAY duplicate values with unrestricted or relevant current mode.
- `dupe` MUST preserve marker facts and current mode at the duplication point.

```llg
let a = 1u32;
let b = a; // scalar copy

let p2 = dupe(packet); // allowed only if packet and reachable parts permit it
```

Relevant places can be duplicated if the type supports duplication, but each
resulting place keeps the relevant obligation state from the moment of
duplication.

## LLG-TS-15 Discard And The Discard Place

Discard must be explicit when a produced value is intentionally ignored, and
implicit discards must be checked when places go out of scope, are overwritten,
or stop being live across control flow.

`_` is the discard place. It is a real receiving place for checker purposes, but
it is not readable and does not bind a name. This is more general than a
separate `drop` operator because `_` also works in patterns.

- `_` MUST be the discard place.
- `_` MUST be a valid receiving place.
- `_` MUST NOT bind a name.
- `_` MUST NOT be readable.
- Flowing a value into `_` MUST be treated as explicit discard.
- Discarding through `_` MUST check the value's current structural mode.
- Discarding through `_` MUST accept unrestricted and affine modes.
- Discarding through `_` MUST reject relevant and linear modes.
- End of scope, overwrite, `return`, `exit`, and `go` MUST check implicit
  discards for places that cease to be live.

```llg
let _ = read_u32(); // explicit discard of a produced value
let _ <- x;         // move x into the discard place

let Pair(keep, _) <- pair; // discard one destructured component
```

The discard place is not a loophole. It only makes discard intentional and gives
the checker a single operation to validate.

## LLG-TS-16 Control Flow, Joins, And Tasks

Marker facts and modes are path-sensitive. A fact proven in one branch is not
automatically known after a join unless every continuing path proves a
compatible fact. A place moved in one branch and used after the join must have
compatible liveness on all paths.

Task transitions add one more requirement: `go` replaces the active state
arguments and preserves task fields. Places that do not flow into the next
state, return value, exit value, or discard place cease to be live and must pass
implicit discard checks.

- Marker facts MUST remain scoped to the control-flow region in which they are
  known.
- Control-flow joins MUST preserve only marker facts that are known on every
  continuing path for the joined place/version.
- Control-flow joins MUST reject inconsistent place liveness when later code
  requires a live place.
- Every place that ceases to be live across `return`, `exit`, or `go` MUST be
  moved somewhere or checked for implicit discard.
- A `go` argument MUST be checked as flowing into the target state argument
  place.
- A task field that remains across `go` MUST remain live with its current mode
  and marker facts unless explicitly replaced.
- A task cycle MUST NOT satisfy event productivity only by carrying an old
  `Event` marker through the cycle.

```llg
state read() {
    let event = read_event();
    unsafe { Event::mark(event); }
    unsafe { Structural::use(event); }
    go read();
}
```

The event is introduced and used in the state body. A carried old event would
not by itself prove that the next cycle received fresh input.

## LLG-TS-17 Diagnostics

The type system should explain errors in place terms. That is the model users
reason about: which place held the fact, where the place was copied or moved,
and which operation tried to copy, discard, or require something invalid.

- Diagnostics for marker-fact failures SHOULD identify the required marker
  fact, the target place, and known near-miss facts when useful.
- Diagnostics for stale marker facts SHOULD identify the older place/version
  that held the fact and the newer place/version that needed it.
- Diagnostics for copy failures SHOULD identify the source place and the mode
  that made it non-copyable.
- Diagnostics for discard failures SHOULD identify the place being discarded
  and the mode or reachable subplace that made it non-discardable.
- Diagnostics for use-after-move failures SHOULD identify the earlier move or
  `take` parameter call.
- Diagnostics for composite failures SHOULD identify the reachable field,
  element, or payload place that blocks the operation.
- Diagnostics SHOULD preserve provenance for `mark`, copy, move, `use`,
  `consume`, field movement, and implicit discard.

```text
cannot discard `packet`
`packet.payload` still has relevant mode introduced by marker `Event`
```

Good diagnostics are a major reason to keep places explicit in the model. A
failed obligation should point to storage the programmer recognizes, not to an
anonymous constraint variable.

## Relationship To Other Type Systems

Langlog's marker layer is closest to a small refinement system over places:
marker facts refine what the checker knows about a concrete value at a specific
storage identity. It is not a full dependent type system because marker
obligations are discharged by direct fact matching and explicit rules rather
than by arbitrary term-level computation or proof search.

Langlog's structural modes borrow from substructural type systems:
unrestricted, affine, relevant, and linear correspond to combinations of copy
and discard permissions. The difference is that these modes are current state
on places, derived from marker type obligations and ownership flow, rather than
being the only way to classify every value type.

This hybrid shape is deliberate. Proof-like facts such as `LessThan` stay cheap
and unrestricted. Resource-like facts such as `Event` can become relevant or
linear without forcing the whole language into a linear core. The compiler gets
stronger guarantees while keeping the checking problem local and explainable.
