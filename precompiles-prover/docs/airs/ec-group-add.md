# EcGroupAdd AIR (`ec::add::EcGroupAddAir`)

> **Scope.** Complete column / constraint / bus reference.
> Design rationale: [../chiplets/ec-group-add.md](../chiplets/ec-group-add.md).
> Bus tuple shapes: [relation-registry.md](relation-registry.md).
> Source: `src/ec/add/mod.rs`, `src/ec/add/trace.rs`.

## Purpose

A **relation** chiplet proving the short-Weierstrass group law
`R = P + Q` for **any** pair of stored operands, across all five
exceptional cases. It mints no coordinate value: every predicate and
every piece of field arithmetic rides **ptr-level certificate tuples**
consumed from the uint relation chiplets ([`UintAdd`](relation-registry.md#11--uintadd),
[`UintMul`](relation-registry.md#12--uintmul)) — no coordinate limb ever
enters this trace. The AIR's own job is *proving which case applies* and
tying the right certificate set to the result.

It **provides** two relations: [`EcGroupAdd`](relation-registry.md#16--ecgroupadd)
`(group, p, q, r)` (the group-law fact, dormant until ladder / DAG / MSM
consumers), and [`EcOnCurveCert`](relation-registry.md#17--econcurvecert)
`(group, r)` (a fresh result's membership certificate — the **closure
cert**, consumed by `r`'s point-store row in place of the on-curve MAC
trio). It **consumes** [`EcPoint`](relation-registry.md#15--ecpoint)
(operands `P`, `Q` and the result `R`), [`EcGroup`](relation-registry.md#14--ecgroup)
(curve context), [`UintMul`](relation-registry.md#12--uintmul) /
[`UintAdd`](relation-registry.md#11--uintadd) (the field-arithmetic
certificates), and [`Range16`](relation-registry.md#1--range16) (the
closure-cert ptr-ordering limbs).

### Core structure

**Five cases**, selected by a prover-witnessed near-one-hot
(`Σ caseᵢ = act + pai_p·pai_q`; src/ec/add/mod.rs:332):

| case | condition | result |
|---|---|---|
| `pai_p` | `P = ∞` | `r_ptr = q_ptr` (pass-through tie) |
| `pai_q` | `Q = ∞` | `r_ptr = p_ptr` (pass-through tie) |
| `cancel` | finite, `x₁ = x₂`, `y₁ + y₂ ≡ 0` | the group's PAI row |
| `double` | finite, `x₁ = x₂`, `y₁ = y₂ ≠ 0` | tangent |
| `generic` | finite, `x₁ ≠ x₂` | chord |

Exhaustive because the store's eager on-curve invariant pins `y₂ = ±y₁`
whenever `x₁ = x₂`; `double` and `cancel` are structurally disjoint
(`2y ≡ 0 ∧ y ≠ 0` is impossible for odd `p`). `∞ + ∞` is the one legal
overlap: each infinite operand needs `is_pai = 1` on its own consumed
`EcPoint` tuple, so *both* pass flags fire and the two ties force
`p = q = r` — the canonical PAI row (src/ec/add/mod.rs:325-343).

**Slope / tail witness.** Each live case interns its formula transients
(`slope_aux`, `λ`, `t`, `e`, `x₃`, `y₃`) as uint-store ptrs and demands the
arithmetic by certificate: the chord/tangent slope MACs, `generic`'s
disequality (`d ≠ 0` rides the `nz = 1` field on `d`'s own `UintAdd`
tuple, no separate inverse MAC), `cancel`'s `is_c_zero` negation tuple
`y₁ + y₂ ≡ 0`, and the shared tail `x₃ = λ² − t, e = x₁ − x₃,
y₃ = λ·e − y₁` — the two mul-subtracts fused via `UintMul`'s `is_sub`
flag, so no `w` / `u` intermediate is ever stored (src/ec/add/mod.rs:602-807).
The `x₁ = x₂` (cancel/double) and `y₁ = y₂` (double) equalities are
native degree-2 in-cell constraints rather than certificates: the res-row
`EcPoint` consumes already pin the coordinate columns to the operands'
stored (value-interned) coordinates, so an in-cell equality is exactly
the value equality (src/ec/add/mod.rs:359-376).

**Closure certificate.** A fresh `generic`/`double` result is on-curve
by group-law closure, so its store row pays no membership MAC trio — it
consumes one `EcOnCurveCert` instead. To keep the self-referential
layer (adds consume `EcPoint`; certified points consume `EcOnCurveCert`)
well-founded, a witnessed `mints` flag marks the op that first mints `r`,
pinned by a case guard (`mints ⟹ generic ∨ double`) and a strict ptr
ordering (`r_ptr > p_ptr ∧ r_ptr > q_ptr`, via Range16-checked limb
diffs); a mint op then *provides* the cert (src/ec/add/mod.rs:386-412,
829-846).

## Trace shape

| Property | Value |
|----------|-------|
| Main width | `NUM_MAIN_COLS = 22` |
| Period | `PERIOD = 4` rows = one add op (`slope`/`tail`/`res`/`term`) |
| Height | `(n_ops · 4)` rounded up to a power of two; trailing rows are all-zero (`act = 0`) padding that touches no bus |
| Periodic columns | `4` one-hot role selectors (verifier-computed), one per row of the period |
| Aux width | `12` = `12` LogUp columns (`COLUMN_SHAPE = [1, 2, 2, 2, 2, 2, 2, 2, 1, 2, 2, 1]`), batching 21 fractions at `log_quotient_degree = 1`; no Schwartz–Zippel / fingerprint register |

One op = one 4-row block (src/ec/add/trace.rs:148-210). The 4 ptr cells
per row hold transients **and** the hosted per-block scalars (what an
earlier 16-row layout carried as separate cycle-constant columns), read
across two-row (local/next) windows; the remaining columns 4–21 are
cycle-constant over the block.

## Main columns

Columns **0–3** (`NUM_CELLS = 4`) are **role-polymorphic**: their meaning
depends on the row (the periodic selector firing there). Columns **4–21**
are **cycle-constant** (held constant across the 4-row block by a
transition gate; src/ec/add/mod.rs:379-384).

| Col | Name | On rows | Range / values | Meaning |
|-----|------|---------|----------------|---------|
| 0 | `CELL_SLOPE_AUX` | `slope` (0) | store ptr | `d = x₂ − x₁` (generic) / `s = 3x² + a` (double) — the slope arrangement transient |
| 1 | `CELL_LAMBDA` | `slope` (0) | store ptr | the slope `λ` |
| 2 | — | `slope` (0) | `0` | unused |
| 3 | `CELL_T` | `slope` (0) | store ptr, or `0` | `t = x₁ + x₂` (generic only — double folds `t = 2x₁` directly into the `x₃` mul-subtract's `κ_c = 2` scale, so this cell is `0`) |
| 0 | `CELL_Y3` | `tail` (1) | store ptr | `y₃` (the result ordinate) — the fused `λ·e − y₁` mul-subtract result |
| 1 | `CELL_E` | `tail` (1) | store ptr | `e = x₁ − x₃` |
| 2 | — | `tail` (1) | `0` | unused |
| 3 | `CELL_X3` | `tail` (1) | store ptr | `x₃` (the result abscissa) — the fused `λ² − t` mul-subtract result |
| 0 | — | `res` (2) | `0` | reserved |
| 1 | `CELL_R` | `res` (2) | EC point ptr | the result `r`'s point ptr |
| 2 | `CELL_SBOUND` | `res` (2) | store ptr | the group's scalar-field bound ptr (closes the `EcGroup` 5-tuple) |
| 3 | `CELL_GROUP` | `res` (2) | EC group ptr | the group ptr |
| 0 | `TERM_CELL_MULT` | `term` (3) | `[0, 2³²)` | the `EcGroupAdd` provide multiplicity = consumer count |
| 1 | `TERM_CELL_P` | `term` (3) | EC point ptr | operand `P`'s point ptr |
| 2 | `TERM_CELL_Q` | `term` (3) | EC point ptr | operand `Q`'s point ptr |
| 3 | — | `term` (3) | `0` | unused term cell (hosts nothing) |
| 4 | `COL_PX` | all | store ptr, or `0` | `P`'s x-coordinate ptr (`0` when `P = ∞`) |
| 5 | `COL_PY` | all | store ptr, or `0` | `P`'s y-coordinate ptr (`0` when `P = ∞`) |
| 6 | `COL_QX` | all | store ptr, or `0` | `Q`'s x-coordinate ptr (`0` when `Q = ∞`) |
| 7 | `COL_QY` | all | store ptr, or `0` | `Q`'s y-coordinate ptr (`0` when `Q = ∞`) |
| 8 | `COL_A_PTR` | all | store ptr | curve param `a`'s ptr |
| 9 | `COL_B_PTR` | all | store ptr | curve param `b`'s ptr (the guaranteed-nonzero inverse-MAC anchor) |
| 10 | `COL_BOUND_PTR` | all | store ptr | the base-field modulus `p`'s ptr (shared by all coordinate uints) |
| 11 | `COL_PAI_P` | all | `{0, 1}` | case flag: `P = ∞` (rides `P`'s `EcPoint` consume as `is_pai`) |
| 12 | `COL_PAI_Q` | all | `{0, 1}` | case flag: `Q = ∞` (rides `Q`'s `EcPoint` consume as `is_pai`) |
| 13 | `COL_CANCEL` | all | `{0, 1}` | case flag: cancellation |
| 14 | `COL_DBL` | all | `{0, 1}` | case flag: doubling |
| 15 | `COL_GEN` | all | `{0, 1}` | case flag: generic chord |
| 16 | `COL_ACT` | all | `{0, 1}` | block-active flag: `1` on real ops, `0` on padding (gates every consume) |
| 17 | `COL_MINTS` | all | `{0, 1}` | fresh-mint flag: `1` iff this op first mints `r` (owns its closure cert); guarded `⟹ generic ∨ double` |
| 18 | `COL_RP_LO` | all | `[0, 2¹⁶)` on mint ops, else `0` | low limb of `r_ptr − p_ptr − 1` (proves `r > p`) |
| 19 | `COL_RP_HI` | all | `[0, 2¹⁶)` on mint ops, else `0` | high limb of `r_ptr − p_ptr − 1` |
| 20 | `COL_RQ_LO` | all | `[0, 2¹⁶)` on mint ops, else `0` | low limb of `r_ptr − q_ptr − 1` (proves `r > q`) |
| 21 | `COL_RQ_HI` | all | `[0, 2¹⁶)` on mint ops, else `0` | high limb of `r_ptr − q_ptr − 1` |

Row 0 cell 2, row 1 cell 2, row 2 cell 0, and row 3 cell 3 are never
written (src/ec/add/trace.rs sets only the cells listed above). The
operand-coordinate ptrs (cols 4–7) are `0` for a PAI operand, matching
its store row's none-sentinels (src/ec/add/trace.rs:173-191).

### Periodic columns (verifier-computed, uncommitted)

`NUM_PERIODIC = 4` one-hot selectors, each `1` on exactly one row of the
period (src/ec/add/mod.rs:270-279):

| Selector | Row | Role |
|----------|-----|------|
| `PCOL_SLOPE` | 0 | `slope` row |
| `PCOL_TAIL` | 1 | `tail` row |
| `PCOL_RES` | 2 | `res` row |
| `PCOL_TERM` | 3 | `term` row |

## Constraints

All main-trace (Phase 1) constraints below are degree ≤ 3.

### Case one-hot / mutex

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 1 | `flag · (1 − flag) = 0` for each of `pai_p, pai_q, cancel, dbl, generic, act, mints` (7 constraints) | 2 | every case flag, the active flag, and the mint flag are boolean (src/ec/add/mod.rs:329-331) |
| 2 | `pai_p + pai_q + cancel + dbl + generic − act − pai_p·pai_q = 0` | 2 | near-one-hot: exactly one case per active block, except `∞ + ∞` where both pass flags fire (the `pai_p·pai_q` slack) (src/ec/add/mod.rs:332-335) |

### Operand-coordinate equalities

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 3 | `(cancel + dbl) · (px − qx) = 0` | 2 | `x₁ = x₂` for `cancel`/`double` — a native in-cell equality: the res-row `EcPoint` consumes already pin `COL_PX`/`COL_QX` to the operands' stored (value-interned) coordinates, so no separate `UintAdd` cert is needed (src/ec/add/mod.rs:369-375) |
| 4 | `dbl · (py − qy) = 0` | 2 | `y₁ = y₂` for `double`, ruling out the `P, −P` cancel branch so the tangent case is genuine (src/ec/add/mod.rs:376) |

### Pass-through result ties

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 5 | `PCOL_RES · pai_p · (r − q) = 0` | 3 | `P = ∞ ⟹ R = Q`: on the res row, `r` is local (`CELL_R`) and `q` is the term cell (`next[TERM_CELL_Q]`) (src/ec/add/mod.rs:383) |
| 6 | `PCOL_RES · pai_q · (r − p) = 0` | 3 | `Q = ∞ ⟹ R = P`; `p` is `next[TERM_CELL_P]` (src/ec/add/mod.rs:384) |

### Closure-cert ptr ordering (Phase 1 scaffolding)

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 7 | `mints · (1 − dbl − generic) = 0` | 2 | case guard: only `generic`/`double` mint a fresh result. Forbids `mints` on `cancel` (result is the ∞ row) and pass-throughs (result is an operand) — kills the pass-through self-certification cycle (src/ec/add/mod.rs:396) |
| 8 | `PCOL_RES · mints · (r − p − 1 − rp_lo − 2¹⁶·rp_hi) = 0` | 3 | reconstructs `r_ptr − p_ptr − 1` from the two Range16 limbs (cols 18–19): with non-negative in-range limbs this proves `r > p`. Read on the res row (`r` local, `p` = `next[TERM_CELL_P]`) (src/ec/add/mod.rs:406-410) |
| 9 | `PCOL_RES · mints · (r − q − 1 − rq_lo − 2¹⁶·rq_hi) = 0` | 3 | likewise `r > q` (cols 20–21). Together these ground the induction over point ptrs so a cert point only cites strictly-smaller already-on-curve operands (src/ec/add/mod.rs:411-412) |

### Cycle-constancy

| # | Constraint | Deg | Rationale |
|---|-----------|-----|-----------|
| 10 | `(1 − PCOL_TERM) · (next[col] − local[col]) = 0` for `col ∈ COL_PX..NUM_MAIN_COLS` (cols 4–21, **18 constraints**) | 2 | the operand coordinate ptrs, curve params, modulus, case flags, `act`, `mints`, and the four ordering limbs must hold the same value across rows 0–3 (they gate / name certificates on different rows); the `not_term` gate releases exactly at the block boundary (src/ec/add/mod.rs:417-422) |

Total Phase-1 constraints: **33** (7 + 1 + 2 + 2 + 1 + 2 + 18). Phase-2 LogUp
columns are evaluated by [`LookupAir::eval`](#buses--lookups) and
sit at their per-column degree budget.

## Buses & lookups

`COLUMN_SHAPE = [1, 2, 2, 2, 2, 2, 2, 2, 1, 2, 2, 1]` (src/ec/add/mod.rs:280)
— twelve LogUp columns batching 21 mutually-exclusive fractions at
`log_quotient_degree = 1`.

### Provides

| Bus | Tuple | Multiplicity | Fires on |
|-----|-------|--------------|----------|
| [`EcGroupAdd`](relation-registry.md#16--ecgroupadd) (16) | `(group, p, q, r)` | `−mult · PCOL_RES` | res row (2); col 0 |
| [`EcOnCurveCert`](relation-registry.md#17--econcurvecert) (17) | `(group, r)` | `−(PCOL_RES · mints)` | res row (2); col 11 (mint column) |

The `EcGroupAdd` provide multiplicity is the stored consumer-count cell
`TERM_CELL_MULT` (read via `next` on the res row), negated; it is `0` in
the dormant EC-stack tests and is pinned to actual demand by bus balance.
The cert provide is exactly `−1` per mint op (src/ec/add/mod.rs:520-537,
829-846).

### Consumes

| Bus | Tuple (as emitted) | Multiplicity | Notes |
|-----|--------------------|--------------|-------|
| [`EcPoint`](relation-registry.md#15--ecpoint) (15) | `(p, group, px, py, is_pai=pai_p)` | `act · PCOL_RES` | operand `P`; the case flag *is* the `is_pai` field — a forged flag matches no store row (src/ec/add/mod.rs:541-556) |
| [`EcPoint`](relation-registry.md#15--ecpoint) (15) | `(q, group, qx, qy, is_pai=pai_q)` | `act · PCOL_RES` | operand `Q` (src/ec/add/mod.rs:557-568) |
| [`EcPoint`](relation-registry.md#15--ecpoint) (15) | `(r, group, x₃, y₃, is_pai=0)` | `(dbl + generic) · PCOL_TAIL` | live result against the computed coordinates; emitted in the tail-row window (`x₃` local, `r`/`group`/`y₃` next) (src/ec/add/mod.rs:572-587) |
| [`EcPoint`](relation-registry.md#15--ecpoint) (15) | `(r, group, 0, 0, is_pai=1)` | `cancel · PCOL_RES` | cancel resolves `R` to the group's PAI row (src/ec/add/mod.rs:588-599) |
| [`EcGroup`](relation-registry.md#14--ecgroup) (14) | `(group, a, b, bound, sbound)` | `(cancel + dbl + generic) · PCOL_RES` | curve context for the live cases (src/ec/add/mod.rs:607-618) |
| [`UintAdd`](relation-registry.md#11--uintadd) (11) | `(bound, py, qy, 0, nz=0)` | `cancel · PCOL_RES` | cancel's `is_c_zero` negation cert `y₁ + y₂ ≡ 0` (src/ec/add/mod.rs:619-630) |
| [`UintAdd`](relation-registry.md#11--uintadd) (11) | `(bound, px, slope_aux, qx, nz=1)` | `generic · PCOL_SLOPE` | `d = x₂ − x₁` (arrangement `x₁ + d ≡ x₂`), certified `d ≠ 0` directly by the tuple's own `nz` field — no separate inverse MAC (src/ec/add/mod.rs:641-651) |
| [`UintMul`](relation-registry.md#12--uintmul) (12) | `κₐ=1, κ_c=1, (λ, slope_aux, py, qy, bound)` | `generic · PCOL_SLOPE` | chord MAC `λ·d + y₁ ≡ y₂` (src/ec/add/mod.rs:652-666) |
| [`UintMul`](relation-registry.md#12--uintmul) (12) | `κₐ=3, κ_c=1, (px, px, a, slope_aux, bound)` | `dbl · PCOL_SLOPE` | tangent numerator `s ≡ 3x² + a` (src/ec/add/mod.rs:682-695) |
| [`UintMul`](relation-registry.md#12--uintmul) (12) | `κₐ=2, κ_c=0, (λ, py, bound, slope_aux, bound)` | `dbl · PCOL_SLOPE` | tangent denominator `2·λ·y ≡ s` (shared `r_ptr = slope_aux`); no `y₁ ≠ 0` witness — a smoothness argument rules out `y = 0` on the double case (src/ec/add/mod.rs:696-710) |
| [`UintAdd`](relation-registry.md#11--uintadd) (11) | `(bound, px, qx, t, nz=0)` | `generic · PCOL_SLOPE` | tail `t = x₁ + x₂` (generic only) (src/ec/add/mod.rs:724-734) |
| [`UintMul`](relation-registry.md#12--uintmul) (12) | `κₐ=1, κ_c=1, (λ, λ, t, x₃, bound), is_sub=1` | `generic · PCOL_SLOPE` | fused `x₃ = λ² − t` (generic) (src/ec/add/mod.rs:735-749) |
| [`UintAdd`](relation-registry.md#11--uintadd) (11) | `(bound, x₃, e, px, nz=0)` | `(dbl + generic) · PCOL_SLOPE` | `e = x₁ − x₃` (arrangement `x₃ + e ≡ x₁`) (src/ec/add/mod.rs:757-767) |
| [`UintMul`](relation-registry.md#12--uintmul) (12) | `κₐ=1, κ_c=1, (λ, e, py, y₃, bound), is_sub=1` | `(dbl + generic) · PCOL_SLOPE` | fused `y₃ = λ·e − y₁`, shared by both live cases (src/ec/add/mod.rs:769-783) |
| [`UintMul`](relation-registry.md#12--uintmul) (12) | `κₐ=1, κ_c=2, (λ, λ, px, x₃, bound), is_sub=1` | `dbl · PCOL_SLOPE` | fused `x₃ = λ² − 2x₁` (double): `t = 2x₁` folds straight into the mul-subtract's `κ_c = 2` scale, so double lays no `t` add at all (src/ec/add/mod.rs:788-806) |
| [`Range16`](relation-registry.md#1--range16) (1) | `(rp_lo)` / `(rp_hi)` | `PCOL_RES · mints` each | the ptr-ordering limbs of `r−p−1` (src/ec/add/mod.rs:815-821) |
| [`Range16`](relation-registry.md#1--range16) (1) | `(rq_lo)` / `(rq_hi)` | `PCOL_RES · mints` each | the ptr-ordering limbs of `r−q−1` (src/ec/add/mod.rs:822-828) |

No [`UintVal`](relation-registry.md#10--uintval) traffic: the chiplet
consumes no coordinate views, only relation tuples. The `x₁ = x₂`
(cancel/double) and `y₁ = y₂` (double) equalities, and the generic
disequality `inv·d ≡ b` MAC the previous layout used, are gone entirely —
the former is a native Phase-1 equality (see
[Operand-coordinate equalities](#operand-coordinate-equalities)) and the
latter rides `d`'s own `UintAdd` tuple.

### Mutex batching

The 21 fractions split across twelve σ columns purely to bound
constraint degree; the multiplicities within each column are one-hot by
row (each selector fires on at most one row of the period), so the
fractions are mutually exclusive and legitimately share the running sum.

- **Col 0** (`ec-add-bindings`, 1 fraction): the `EcGroupAdd` provide,
  alone — the gated running-sum anchor (src/ec/add/mod.rs:520-537).
- **Col 1** (`ec-add-bindings`, 2 fractions): the `P`/`Q` operand
  `EcPoint` consumes, res-row window (src/ec/add/mod.rs:541-569).
- **Col 2** (`ec-add-bindings`, 2 fractions): the live-result `EcPoint`
  consume (tail-row window) and the cancel-PAI-result `EcPoint` consume
  (res-row window) (src/ec/add/mod.rs:572-600).
- **Col 3** (`ec-add-bindings`, 2 fractions): the `EcGroup` consume +
  cancel's `y₁+y₂≡0` certificate, both res-row window
  (src/ec/add/mod.rs:603-631).
- **Col 4** (`ec-add-slope`, 2 fractions): generic's `d`-subtract
  (certified nonzero) + chord certificates, slope-row window
  (src/ec/add/mod.rs:636-667).
- **Col 5** (`ec-add-slope`, 2 fractions): double's tangent-numerator +
  slope-pin certificates, slope-row window (src/ec/add/mod.rs:677-711).
- **Col 6** (`ec-add-tail`, 2 fractions): generic's `t`-add + fused `x₃`
  mul-subtract (src/ec/add/mod.rs:719-750).
- **Col 7** (`ec-add-tail`, 2 fractions): the shared `e`-subtract + fused
  `y₃` mul-subtract (src/ec/add/mod.rs:753-784).
- **Col 8** (`ec-add-tail`, 1 fraction): double's fused `x₃` mul-subtract,
  alone — mutually exclusive with col 6's generic form, no partner left
  to pair (src/ec/add/mod.rs:788-807).
- **Col 9 / 10** (`ec-add-mint`, 2 fractions each): the closure-cert
  ptr-ordering Range16 limb pairs for `r−p−1` and `r−q−1`
  (src/ec/add/mod.rs:815-828).
- **Col 11** (`ec-add-mint`, 1 fraction): the result-membership cert
  provide, alone (src/ec/add/mod.rs:829-846).
