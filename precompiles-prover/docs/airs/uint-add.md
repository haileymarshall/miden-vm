# UintAdd AIR (`uint::add::UintAddAir`)

> **Scope.** Complete column / constraint / bus reference.
> Design rationale: [../chiplets/uint-add.md](../chiplets/uint-add.md).
> Bus tuple shapes: [relation-registry.md](relation-registry.md).
> Source: `src/uint/add/mod.rs`.

## Purpose

A **relation** chiplet (it mints no value): it asserts modular addition
`a + b ≡ c (mod p)` over three uints already living in the
[UintStore](uint-store.md). The operands, result and modulus are pulled
in by pointer over the [`UintVal`](relation-registry.md#10--uintval) bus;
this chiplet ties those pointers to the modular-sum identity and
**provides** the [`UintAdd`](relation-registry.md#11--uintadd) relation,
consumed by the transcript-eval chip's add / sub `UintOp` nodes and by the EC
group law's coordinate certificates.

Two zero-sentinel modes share the layout: `is_c_zero` proves `a + b ≡ 0`
(negation, with an unstored zero result) and `is_b_zero` proves
`a + 0 ≡ c` — the stored-value **equality certificate** `a = c`. A
cycle-constant `nz` flag additionally certifies `b ≠ 0` when set.

## The identity (vertical Schwartz–Zippel)

Since `a, b < p ⟹ a + b < 2p`, one conditional subtraction suffices:

```text
a + b − k·p = c,   k ∈ {0, 1},   p = bound + 1
```

The store holds `bound = p − 1` (so any modulus, including 2²⁵⁶, is
representable), so the `+1` becomes a `−k` correction at `β⁰`. The whole
block is checked at the LogUp challenge `β` by **one block-local ext
constraint on the open row's two-row window** — the open row holds
`a ‖ b` and its next row `c ‖ p`, so all four values are visible at
once and no accumulation register or closure row exists:

```text
D(β) = a(β) + b(β) − c(β) − k·bound(β) − k + (β − t)·Γ(β) = 0,   t = 2³²
Γ(β) = Σⱼ₌₀⁶ γⱼ·βʲ,   γⱼ ∈ {−1, 0, 1}
```

`D(X)` has `D(t) = 0`, so `(X − t) | D` with a degree-6 quotient → exactly
**7 carries** `γ₀..γ₆`, no top-carry slot (the bit-256 overflow cancels
because `a + b = c + k·p`). Each signed carry — the difference between
the binary carry chain of `a+b` and that of `c+k·p` — is **ternary**,
committed directly and range-checked by an ungated `γ(1−γ)(1+γ)` per
carry column (no `Range16` on carries). Operands inherit the store's
16-bit range through the `UintVal` tie; every per-limb coefficient of
the identity is bounded by `≲ 2³⁴ ≪ 2⁶³`, so coefficients vanishing over
the field vanish over ℤ and the limb equations chain to the exact
integer identity.

## Trace shape

| Property | Value |
|----------|-------|
| Main width | `NUM_MAIN_COLS = 30` |
| Period | `PERIOD = 2` rows — the open row (`a ‖ b`) and the closing row (`c ‖ p`) |
| Height | `(n_ops · 2)` rounded up to a power of two; trailing rows are all-zero (`act = 0`) padding |
| Periodic columns | `1` selector (verifier-computed): `1` on the open row; the closing row is its complement |
| Aux width | `3` LogUp columns (`COLUMN_SHAPE = [1, 2, 2]`) — no register (the SZ identity is a main-trace constraint) |

A block lays two full `UintVal` 8×32 values per row: `a ‖ b` on the
open row, `c ‖ p` on the closing row. The SZ identity fires on the open
row (whose local/next window sees the whole block); the `UintAdd`
provide fires on the closing row.

## Main columns

Columns 0–15 hold the row's two full `UintVal` values (`NUM_LIMBS = 8`
each; `CELL_HI = 8` starts the second). Columns 16–19
(`FIRST_GAMMA_COL` onward) are the carry columns; columns 20–23 are
**role-polymorphic** scalar cells whose meaning depends on the row.
Columns 24–29 are **cycle-constant** (constant across the 2-row block).

| Col | Name | On rows | Range / values | Meaning |
|-----|------|---------|----------------|---------|
| 0–7 | limb cells | both | each `∈ [0, 2³²)` (32-bit, via the `UintVal` tie) | `a`'s words (open row) / `c`'s words (closing row) |
| 8–15 | limb cells | both | each `∈ [0, 2³²)` | `b`'s words (open row) / `p`'s words (closing row) |
| 16–19 | carry cells | open | each `∈ {−1, 0, 1}` | γ₀..γ₃ (`GAMMA_SLOTS`) |
| 16–18 | carry cells | closing | each `∈ {−1, 0, 1}` | γ₄..γ₆; cell 19 is structurally zero |
| 20 | `is_b_zero` | open | `{0, 1}` | `CELL_IS_B_ZERO`: when set, `b` is the unstored zero |
| 20 | `k` | closing | `{0, 1}` | `CELL_K`: the modular reduction bit |
| 21 | `w` | open | field element | `CELL_D_W`: the nonzero-certificate's witnessed candidate inverse of `S = Σⱼ bⱼ` |
| 21 | `c_on` | closing | `{0, 1}` | `CELL_C_ON`: `act·(1 − is_c_zero)`, the `c` consume's witnessed activity gate |
| 22 | `wS` | open | field element | `CELL_D_WS`: `w · S`, pinned locally to keep the nz-cert check degree 3 |
| 22 | `mult` | closing | `[0, 2³²)` | `TERM_CELL_MULT`: the `UintAdd` provide multiplicity = consumer count |
| 23 | `b_on` | open | `{0, 1}` | `CELL_B_ON`: `act·(1 − is_b_zero)`, the `b` consume's witnessed activity gate |
| 23 | `is_c_zero` | closing | `{0, 1}` | `CELL_IS_C_ZERO`: when set, `c` is the unstored zero |
| 24 | `COL_A_PTR` | both | store ptr | `a`'s pointer |
| 25 | `COL_B_PTR` | both | store ptr, or `0` | `b`'s pointer (forced `0` when `is_b_zero`) |
| 26 | `COL_C_PTR` | both | store ptr, or `0` | `c`'s (result) pointer (forced `0` when `is_c_zero`) |
| 27 | `COL_BOUND_PTR` | both | store ptr | the shared modulus `p`'s pointer |
| 28 | `COL_ACT` | both | `{0, 1}` | block-active flag: `1` on real op blocks, `0` on padding (gates every consume) |
| 29 | `COL_NZ` | both | `{0, 1}` | nonzero-certificate flag, read on the open row (checked) and the closing row (rides the provide tuple) |

### Periodic columns (verifier-computed, uncommitted)

One selector:

| Selector | Value |
|----------|-------|
| `ab_sel` | `1` on the open row, `0` on the closing row |

`cp_sel = 1 − ab_sel` marks the closing row. Every next-reading
constraint is `ab_sel`-gated, so the cyclic last → first window (whose
local row is a closing row) is dropped for free.

## Constraints

All constraints below are degree ≤ 3 (lqd 1).

### Schwartz–Zippel identity (block-local)

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 1 | `ab_sel · [a(β) + b(β)·(1−is_b_zero) − c(β)·(1−is_c_zero') − k'·(bound(β)+1) + Σⱼ (β^{j+1} − t·βʲ)·γⱼ] = 0` | 3 (ext) | the modular-sum identity at `β`, asserted once per block on the open row's window (primes read the next row: `c`/`p`'s limbs, `k`, `is_c_zero`). Padding rows satisfy it trivially (all-zero cells), so no `act` gate is needed |

### Range checks

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 2 | `γ · (1 − γ) · (1 + γ) = 0` on each of the four carry columns, ungated | 3 | the signed ternary carry range. The carry columns host only Γ slots (or a structural zero), so the check needs no row selector |
| 3 | `f · (1 − f) = 0` on columns 20 and 23, ungated | 2 | each hosts a boolean on both rows (`is_b_zero`/`k`, `b_on`/`is_c_zero`) — one check covers both residents |
| 4 | `act · (1 − act) = 0` | 2 | block-active flag is boolean |
| 5 | `nz · (1 − nz) = 0` | 2 | nonzero-certificate flag is boolean |

### Pointer pins

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 6 | `cp_sel · is_c_zero · c_ptr = 0` | 3 | the unstored zero result has no address; `c_ptr = 0` reads as "≡ 0" to a consumer |
| 7 | `ab_sel · is_b_zero · b_ptr = 0` | 3 | the dropped `b` operand reads as the equality form `a + 0 ≡ c` |

### Activity-gate pins

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 8 | `ab_sel · (b_on − act·(1 − is_b_zero)) = 0` | 3 | pins `b_on` to the witnessed activity gate, all cells local to the open row |
| 9 | `cp_sel · (c_on − act·(1 − is_c_zero)) = 0` | 3 | pins `c_on` likewise, all cells local to the closing row |

### Nonzero certificate

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 10 | `ab_sel · (wS − w·S) = 0` | 3 | pins the hoisted product `wS = w·S` (`S = Σⱼ bⱼ` over the open row's high half, native-summed, no β-weighting) |
| 11 | `ab_sel · nz · (wS − 1) = 0` | 3 | when `nz = 1`, `w` is a genuine inverse of `S` — proving `S ≠ 0 ⟺ b ≠ 0` |

### Cycle-constancy

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 12 | `ab_sel · (next[col] − local[col]) = 0` for `col ∈ {A_PTR, B_PTR, C_PTR, BOUND_PTR, ACT, NZ}` | 2 | the four ptrs need joint visibility at the closing-row provide *and* at their own row's consume; `nz` is read on both rows; `act` gates every consume. The open row pins the closing row; the closing → open edge (the block boundary, and the cyclic wrap) is free |

### Provide gating

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 13 | `cp_sel · (1 − act) · mult = 0` | 3 | a provide must come from an active block. The `UintAdd` provide is gated by `cp_sel` only (not `act`), and the operand consumes *are* `act`-gated — so an `act = 0` block with zeroed limbs (the SZ identity closes on `0 = 0`) and a witnessed `mult` could otherwise provide a *false* relation onto the bus. Forcing `mult = 0` on inactive blocks closes it |

## Buses & lookups

`COLUMN_SHAPE = [1, 2, 2]` — three LogUp columns batching 1, 2 and 2
mutually-exclusive fractions respectively.

### Provides

| Bus | Tuple | Multiplicity | Fires on |
|-----|-------|--------------|----------|
| [`UintAdd`](relation-registry.md#11--uintadd) (11) | `(bound_ptr, a_ptr, b_ptr, c_ptr, nz)` | `−mult · cp_sel` | the closing row |

The provide multiplicity is the stored consumer-count cell `mult`,
negated; it is pinned to the actual demand by bus balance (no range
check).

### Consumes

Four [`UintVal`](relation-registry.md#10--uintval) full-value messages
per block — the 8×32 recombined view `(ptr, bound_ptr, c0..c7)`, each
read entirely from local cells on its firing row (`lo` = cells 0–7,
`hi` = cells 8–15):

| Operand | Row | Limbs | Multiplicity | Notes |
|---------|-----|-------|--------------|-------|
| `a` | open | `lo` | `ab_sel · act` | |
| `b` | open | `hi` | `ab_sel · b_on` | suppressed when `b = 0` |
| `c` | closing | `lo` | `cp_sel · c_on` | suppressed when `c = 0` |
| `p` | closing | `hi` | `cp_sel · act` | the shared modulus |

### Mutex batching

The four consumes plus the provide split across the three σ columns
purely to bound constraint degree:

- **Col 0** (`uintadd`, 1 fraction): `a`'s consume, alone — the running
  sum, since the `+1` gate forbids a degree-3 fraction there.
- **Col 1** (`uintadd`, 2 fractions): `b` + `c`'s gated consumes
  (degree 2 via `sel·on`).
- **Col 2** (`uintadd-pp`, 2 fractions, mixed batch): `p`'s consume +
  the `UintAdd` provide.

Within each column the multiplicities are one-hot by row (each fires on
exactly one row of the period), so the fractions are mutually exclusive
and legitimately share the running sum.
