# UintAdd chiplet — modular addition over the UintStore

> **AIR reference:** [`airs/uint-add.md`](../airs/uint-add.md) — complete column / constraint / bus reference for this chiplet.

The addition relation `a + b ≡ c (mod p)` over stored 256-bit uints.
**Implemented** — files: `src/uint/add/{mod,trace}.rs`; companion to
[`uint.md`](uint.md) (the store this is a
[relation chiplet](uint.md#relation-chiplets-over-the-store) over) and
[`uint-mul.md`](uint-mul.md) (the multiply side of the split).

**Why not MAC it:** add-via-mul (`a·1 + b`) would run a full 16×16
convolution for a trivial product, and ECC point formulas are
add/sub-heavy. A dedicated adder is far leaner: no quotient, no
`Range16` on carries, a boolean reduction bit — and it consumes the
store's existing `UintVal` view, needing nothing the store didn't
already provide.

## The identity

`a, b < p ⟹ a + b < 2p`, so at most one modulus subtraction:

```
a + b − k·p = c,    k ∈ {0, 1},    p = bound + 1
```

With the store holding `bound = p − 1` (so any modulus, incl. 2²⁵⁶,
stays representable), the looked-up value is `bound` and the `+1`
becomes a `−k` correction at `β⁰`. Verified at the LogUp challenge β
(the [β-reuse precedent](uint.md#why-its-sound) the store set) by one
**block-local** ext constraint on the open row's two-row window — the
open row sees `a ‖ b` locally and `c ‖ p` on next, so the whole check
is a single local assertion, with no accumulation register, no closure
row, and no wrap-around special case for the trace's final block:

```
a(β) + b(β) − c(β) − k·bound(β) − k + (β − t)·Γ(β) = 0,    t = 2³²
Γ(β) = Σⱼ₌₀⁶ γⱼ·βʲ,    γⱼ ∈ {−1, 0, 1}
```

`D(X) = a + b − c − k·bound − k` has `D(t) = 0`, so `(X − t) ∣ D` with a
degree-6 quotient → exactly **7 carries `γ₀..γ₆`, no top-carry slot**
(the bit-256 overflow cancels in the difference, since
`a + b = c + k·p`) — mirroring the store's missing `c₇`.

**The carries are signed ternary.** Because the identity is a
*difference*, each carry `γⱼ` — the difference between the binary carry
chain of `a + b` and that of `c + k·p` — lies in `{−1, 0, 1}`. It is
committed directly and range-checked by one ungated `γ(1−γ)(1+γ)` per
carry column: the carry columns host nothing but Γ slots, so no row
selector is needed and the check stays at degree 3 — **no `Range16` on
carries**. Operands are *not* re-range-checked: they inherit the
store's 16-bit checks through the `UintVal` tie, and no-wrap holds
trivially — every per-limb coefficient of the identity is bounded by
`≲ 2³⁴ ≪ 2⁶³`, so coefficients vanishing over the field vanish over ℤ
and the limb equations chain to the exact integer identity.

**Same-modulus keyed for free:** all four `UintVal` lookups (a/b/c + the
modulus) carry the block's `bound_ptr`, so the store's providers force
the operands onto one modulus, and the modulus consume
`(bound_ptr, bound_ptr, …)` matches only a self-referential pin.
Canonicity of `c` (`< p`) is the store's range-membership on interning;
this AIR checks only the reduction identity.

## Layout (period-2, two values per row)

16×32 per row (two whole 256-bit values): the **open row** carries
`a ‖ b`, the **closing row** `c ‖ p`. The SZ identity is asserted on
the open row, whose local/next window sees all four values at once; the
`UintAdd` provide and its multiplicity cell sit on the closing row.

Cells 16–19 are the carry columns (γ₀–γ₃ on the open row, γ₄–γ₆ on the
closing row, whose fourth slot is structurally zero — [`GAMMA_SLOTS`]
is the placement table the AIR and trace-gen both read, so the two
cannot drift). Cells 20–23 host the block scalars, one per row; columns
20 and 23 carry a boolean on both rows, so a single ungated booleanity
check per column covers both residents.

| col   | open row (`a ‖ b`)    | closing row (`c ‖ p`) |
|-------|-----------------------|-----------------------|
| 0–7   | `a`'s limbs           | `c`'s limbs           |
| 8–15  | `b`'s limbs           | `p`'s limbs           |
| 16–19 | γ₀ γ₁ γ₂ γ₃           | γ₄ γ₅ γ₆ (19 zero)    |
| 20    | `is_b_zero`           | `k`                   |
| 21    | `w`                   | `c_on`                |
| 22    | `wS`                  | `mult`                |
| 23    | `b_on`                | `is_c_zero`           |
| 24–29 | `a_ptr b_ptr c_ptr bound_ptr act nz` (cycle-constant) |

Two zero-sentinel modes, one per row: **`is_c_zero`** drops the
`c` side (`a + b ≡ 0` — negation with an unstored zero result) and
**`is_b_zero`** drops the `b` side (`a + 0 ≡ c` — the stored-value
**equality certificate** `a = c`, both canonical under one modulus;
consumed e.g. by the EC group law's `x₁ = x₂` / `y₁ = y₂` case ties).

**Nonzero certificate.** A block's cycle-constant `nz` flag ([`COL_NZ`])
additionally certifies `b ≠ 0` when set, in place of a full inverse
modmul: `S = Σⱼ bⱼ` — a native sum of `b`'s eight 32-bit limbs, no
β-weighting, `< 2³⁵ < p_Goldilocks` so no wrap — is `0 ⟺ b = 0`, and
`nz · (w·S − 1) = 0` with a witnessed candidate inverse `w`
([`CELL_D_W`], `w·S` hoisted to [`CELL_D_WS`] to keep the check degree 3)
proves `S ≠ 0`. `nz` rides the `UintAdd` bus tuple as a 5th field, so a
consumer can demand `nz = 1` on the same block that already proves
`a + b ≡ c` — the EC group law's generic-add case uses this on its
`d = x₂ − x₁` subtraction instead of a separate disequality MAC.

The gated `b`/`c` `UintVal` consumes read a witnessed activity gate
`on = act·(1 − is_zero)` locally on their firing row (`b_on` on the
open row, `c_on` on the closing row): `sel·on` is degree 2, folding the
`act` gate in so the gated consume pairs with another degree-2 fraction
instead of sitting alone at degree 3.

## Columns

**Main 30**: 16 limb cells (two full `UintVal` values per row), 4 carry
cells (16–19), 4 block-scalar cells (20–23), then `a_ptr, b_ptr, c_ptr,
bound_ptr, act, nz` (cycle-constant). The four ptrs are forced to
columns — they need joint visibility at the closing-row provide *and*
at their own row's consume, which only cycle-constancy transports — and
`act ∈ {0, 1}` gates every consume. `nz` rides a cycle-constant column
too: it's read on both the open row (where the certificate is checked)
and the closing row (where it rides the provide tuple). `act` gating
every consume means **padding blocks are all-zero rows that touch no
bus**.

**Aux 3** — the three LogUp fraction columns, ≤ 2 fractions each so
every closing constraint stays at degree ≤ 3 (lqd 1). The SZ identity
is a block-local main-trace constraint, so no register accompanies
them:

| col | contents |
|---|---|
| 0 | LogUp running sum: `a`'s `UintVal` consume, alone |
| 1 | `b` + `c`'s gated `UintVal` consumes |
| 2 | `p`'s `UintVal` consume + the `UintAdd` provide |

## Buses

| Bus | Tuple | Direction |
|---|---|---|
| `UintAdd` (11) | `(bound_ptr, a_ptr, b_ptr, c_ptr, nz)` | provide on the closing row, mult = the op's consumer count (identical relations collapse onto one block, mults accumulating; 0 = dormant); a 0 ptr-slot reads as "the unstored zero" (`c_ptr = 0`: "≡ 0"; `b_ptr = 0`: the `a + 0 ≡ c` equality form) |
| `UintVal` (10) | 8×32 view, full value | consume ×4/op (a, b, c, modulus; ×3 when `is_b_zero` / `is_c_zero`) |

The result `c` is **caller-assigned** (a nondeterministic witness),
which is what lets arrangements name their result — and `is_c_zero`
skip it.

## The require layer

```rust
UintRequire::add(a_ptr, b_ptr) -> ptr    // a + b mod p
UintRequire::sub(x_ptr, y_ptr) -> ptr    // x − y via the arrangement y + z = x
UintRequire::neg(v_ptr) -> ptr           // −v via is_c_zero: v + z ≡ 0
UintRequire::add_to_zero(a_ptr, b_ptr)   // a + b ≡ 0 over stored ptrs (no result)
UintRequire::value_eq(a_ptr, c_ptr)      // a = c via is_b_zero: a + 0 ≡ c
```

`UintRequire` (a transient view over store + add + mul accumulators)
resolves operands from the store, reduces, interns the result
**canonically** (ptr ≥ 2¹⁶, deduped by `(value, modulus)` — the `is`
completeness contract) and records the op with its tuple provided at
multiplicity 1 — every op recorded through the layer is consumed
exactly once by its requester (an eval `UintOp` node or an EC
certificate). Ptrs travel as `UintPtr` handles minted only by the
store's interning entries, so a raw address can't enter the layer. The
chiplet-level `UintAddRequires::record` / `record_to_zero` stay pure
ptr recorders (values resolve at trace-gen; explicit mult, 0 =
dormant) that **intern by relation identity**: a duplicate of an
already-recorded arrangement collapses onto its block, the mults
adding. Sub needs no negative anything — the arrangement swaps the result
slot. The public DAG-level `uint_add` / `uint_sub`
([uint.md](uint.md#the-dag-surface)) drive the layer from the Session;
callers express negation as `uint_sub(0, x)` with a typed zero leaf.

## Negation: the `is_c_zero` mode

`z = −v` as `v + z = k·p + 0` puts a *zero* in the result slot — but
with pin_ptr-anchored, modulus-typed values there is no untyped zero to
name for an arbitrary modulus (a typed zero would itself have to be
pinned, and pinning originates in the DAG). A boolean **`is_c_zero`
flag** (a closing-row scalar) instead treats `c` as the **unstored
zero**:

- the identity degenerates to `a + b − k·p = 0`; `k` stays witnessed,
  so both cases are provable — `a + b = p` (`k = 1`) **and**
  `a = b = 0` (`k = 0`), giving `−0 = 0` with no special case (`z = p`
  is not internable, so there's no cheat);
- the tuple carries **`c_ptr = 0` as the "≡ 0" sentinel** (address 0 is
  never stored, so it reads as "none" on the bus), constraint-tied by
  `is_c_zero · c_ptr = 0`;
- the `c` consume and its identity term gate through the witnessed
  `c_on = act·(1 − is_c_zero)` cell, keeping the fraction's
  multiplicity at degree 2;
- cost: one cell, zero new rows, no bus changes; per negation, one add
  block (c half dead) + the transient's store block.

Why not cheaper? The store witnesses `comp = bound − v` for every uint,
but that is the *complement* `~v`, off by a carry-rippling `+1` from
`−v` — no linear view can bridge it, and the store-side alternatives
that make comp the true negation pay per-block costs on every uint (see
[the settled alternatives](uint.md#the-witnessed-modulus)). Pay-per-use
wins for an op that occurs per point-subtraction, not per ladder step.

## Equality: the `is_b_zero` mode

The mirror sentinel on the operand side: `b` as the unstored zero turns
the block into `a + 0 ≡ c (mod p)` — with `a`, `c` stored canonical
under one modulus, exactly the **value-equality certificate `a = c`**,
ptr-free and pin-free. `k` stays witnessed but only `k = 0` is
satisfiable (`a = c + p` is out of range for canonical values). Same
mechanics as `is_c_zero`: `is_b_zero · b_ptr = 0` ties the tuple
sentinel, and the `b` consume and its identity term gate through the
witnessed `b_on` cell. The consumer this was built for: the EC group
law's case ties (`x₁ = x₂` for `double`/`cancel`, `y₁ = y₂` for
`double`) — value-level, so two distinct ptrs binding equal coordinates
still add correctly, with no limb views in the add relation chiplet
(see [ec-group-add.md](ec-group-add.md)).

## Tests

`tests::uint_add` — constraints (carrying and reduction cases), the
`k = 1` path, sub as an arrangement, tampered-result rejection, bus
balance against store + BPL, the act-gated padding regression (3 ops →
a pad block that must stay off every bus), negation balancing with no
stored zero, the equality certificate holding + balancing with no `b`,
the nonzero-certificate tests (holds and balances; forged zero
rejected; wrong witness rejected), the sentinel rejections (forged
`c_ptr` under `is_c_zero`, forged `b_ptr` under `is_b_zero`,
`is_b_zero` forged onto unequal values), and the lqd-1 design-target
regression.
