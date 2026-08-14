# Internal audit 2 — `valory-xyz/lockbox-solana`

**Date:** 2026-06-02
**Auditor:** audit-claude, 77ph
**Scope baseline:** HEAD `5fa56d0` (2026-05-25 «Update LICENSE») on branch `main`
**Code under audit:** `programs/liquidity_lockbox/` — 2 `.rs` files, **761 LoC** (lib.rs 711 + state.rs 50)
**Toolchain (per `setup-env.sh` / `CLAUDE.md`):** rustc 1.62.0, Solana 1.14.29, Anchor 0.26.0 — pinned for Orca Whirlpool CPI compatibility
**Mainnet program ID:** `1BoXeb8hobfLCHNsyCoG1jpEv41ez4w4eDrJ48N1jY3` (deployed; this audit covers source at HEAD — on-chain bytecode verification is a separate exercise, see F-9)
**Approach:** clean independent first-pass review of the current source. Prior audit artefacts (`audits/internal/` from 2024-02; external Sec3 audit PDF in `doc/`) are noted as historical context but not relied upon for this verdict.
**Methodology:** exhaustive Anchor-specific + Solana-specific + DeFi cross-domain pattern checklist applied to every applicable surface; adversarial sweep on what is NOT explicitly in the documented intent. Security-only audit scope.

---

## 1. Verdict

**PASS-WITH-FINDINGS** — 0 Critical / 0 High / 0 Medium / **2 Low** / **7 Info**.

| Severity | Count | Items |
|---|---|---|
| Critical | 0 | — |
| High | 0 | — |
| Medium | 0 | — |
| Low | 2 | F-1 (bridged-mint `freeze_authority` not enforced in program), F-2 (bridged-mint `decimals` not enforced in program) |
| Info | 7 | F-3 (approve(0) vs token::revoke), F-4 (gas: find_program_address vs stored bump), F-5 (InRange check no-op for full-range), F-6 (toolchain stale + transitive `cargo audit` warnings), F-7 (`mint_authority.unwrap()` pattern), F-8 (signer `#[account(mut)]` inconsistency in withdraw), F-9 (on-chain bytecode-vs-source verification recommended as follow-up) |

The 3 implemented instructions (`initialize`, `deposit`, `withdraw`) are sound under standard Solana / Anchor invariants: the lockbox PDA is the sole authority over the Orca position; deposits and withdraws maintain a 1:1 mint/burn relationship with position liquidity; tokens flow only between signer-owned and program-owned accounts that the type system validates. The Critical-class concerns I checked for — re-initialization, mint-authority bypass, position-NFT theft, CPI/reentrancy abuse, arithmetic overflow, signer confusion — all check out.

**The 2 Low findings (F-1, F-2) are deployment-discipline guards** — the program trusts the deployer to set the bridged-token mint correctly (freeze_authority = None; decimals = 8). The off-chain deployment script `scripts/lockbox_init.ts` does set these correctly (line 73: `createMint(provider.connection, userWallet, pdaProgram, null, 8)`), but the program itself does not enforce them. A fork / re-deployment with different parameters would create a footgun. Adding two one-line constraints closes this defense-in-depth gap.

**The 7 Info findings are non-exploitable code-quality / gas-efficiency / style observations** — none individually meaningful, collectively a moderate hygiene-improvement opportunity if a future maintenance pass is undertaken.

---

## 2. Project intent

`lockbox-solana` is the on-Solana **liquidity-locking bonding mechanism** for the Olas (Autonolas) protocol. The concept (per `doc/Bonding_mechanism_with_liquidity_on_Solana.pdf` and `README.md`):

1. A user contributes **OLAS** and **SOL** tokens to the Olas treasury.
2. In return, they receive a fungible **bridged LP token** that represents their share of an OLAS-SOL liquidity position locked in the Orca Whirlpool (concentrated-liquidity AMM).
3. The lockbox program is the sole authority over a single, hardcoded, **full-range** Orca position in the OLAS-SOL whirlpool.
4. On user deposit, the lockbox CPI-calls Orca's `increase_liquidity` and mints bridged tokens 1:1 with the position's liquidity delta.
5. On user withdrawal, the lockbox burns the user's bridged tokens, calls Orca's `update_fees_and_rewards` + `collect_fees` (fees route to a separate fee-collector address controlled by Olas governance), then `decrease_liquidity` to return OLAS + SOL to the user.

### Hardcoded constants

```rust
const ORCA: Pubkey         = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const WHIRLPOOL: Pubkey    = pubkey!("5dMKUYJDsjZkAD3wiV3ViQkuq9pSmWQ5eAzcQLtDnUT3"); // OLAS-SOL
const SOL: Pubkey          = pubkey!("So11111111111111111111111111111111111111112");
const OLAS: Pubkey         = pubkey!("Ez3nzG9ofodYCvEmw73XhQ87LWNYVRM2s7diB5tBZPyM");
const TICK_LOWER_INDEX: i32 = -443584;
const TICK_UPPER_INDEX: i32 =  443584;  // full-range, spacing 64
const BRIDGED_TOKEN_DECIMALS: u8 = 8;
```

The lockbox is deliberately monolithic: one program, one position, one whirlpool, one bonding pair. Maintenance / upgrade authority over the program is delegated to a separate `lockbox-governor-solana` (timelock-governed admin), per the cross-repo architecture; **that governor is out of scope for this audit and reviewed separately**.

### Trust model

The lockbox program trusts:

- **The deployer** to set the bridged-token mint's `mint_authority` to the lockbox PDA (program enforces this at init), and the `freeze_authority` to None + `decimals` to 8 (program does NOT enforce; **F-1, F-2 findings**).
- **The deployer** to fund the program-owned `pda_position_account` with a freshly-minted Orca position NFT (amount = 1, owner = lockbox PDA, position.liquidity = 0, position whirlpool = OLAS-SOL hardcoded address) before calling `initialize`. The program validates all of these at init.
- **Orca Whirlpool** correctness for CL math — `increase_liquidity`, `decrease_liquidity`, `update_fees_and_rewards`, `collect_fees` are CPI'd as-is.
- **Solana runtime** for ed25519 signature verification, reentrancy prevention, account ownership invariants.

What the lockbox does NOT trust:

- The end user — every account passed in is constrained via Anchor `#[account(...)]` macros + runtime checks; the user cannot substitute a different mint, vault, position, whirlpool, or tick array without triggering a constraint failure.
- The lockbox PDA's own state — `total_liquidity` is updated via `checked_add` / `checked_sub` (overflow protection).

This is a tight trust model. The narrow surface (3 instructions, 1 position, hardcoded everything) is a deliberate scope-reduction choice that makes the audit tractable.

---

## 3. Architectural review

This section answers two questions:

### 3.1 Is the architecture sound?

**Yes, for the explicit scope.** Three deliberate scope choices and their implications:

- **Single hardcoded position** in a specific whirlpool. **Pro**: no position selection / governance / oracle / risk-engine complexity; the lockbox is one stateful smart contract over one financial primitive. **Con**: the entire lockbox TVL is concentrated in one Orca position; if Orca's CL math in this specific configuration has a defect, all lockbox funds are at risk. This is an explicit cross-dependency risk; it is mitigated by Orca being one of the most-audited CL implementations on Solana.
- **Full-range position only** (ticks ±443584). **Pro**: no impermanent-loss in the narrow-range sense; the position is always in range; deposits and withdrawals work at any price. **Con**: capital-inefficient compared to concentrated ranges. This is a known tradeoff intentional to the bonding design — the goal is locked liquidity, not capital-efficient market-making.
- **Fees flow to a separate `fee_collector` address controlled by governance, not back to bridged-token holders.** **Pro**: protocol-owned liquidity model — LP fees fund the Olas treasury rather than diluting the bonding incentive. **Con**: from the depositor's perspective, the bridged token is a claim on liquidity-share-of-tokens only, NOT a fee-earning LP token. **Must be communicated clearly to users** to set expectations correctly. The README / docs do describe this; verify user-facing materials reflect it.

### 3.2 Is the implementation on the right path?

**Yes, with minor defense-in-depth gaps.** What's right:

- **Anchor 0.26 with `seeds = true`** — enables automatic PDA seed verification, the key Anchor safety feature.
- **`overflow-checks = true`** in `[profile.release]` (both root and program Cargo.toml) — runtime arithmetic-overflow defense.
- **Stored `bump` in `LiquidityLockbox` state** — enables deterministic signer-seed derivation without `find_program_address` round-trips (though the program currently uses `find_program_address` instead in a few places — see F-4).
- **Manual position-discriminator check** at init (defense-in-depth on top of Anchor's typed account validation).
- **Manual `pda_position_account.owner == lockbox.key()` check** at init (ensures the position NFT is actually under lockbox control before storing the reference).
- **Hardcoded program / token / whirlpool addresses** in addition to Anchor type-level constraints — defense-in-depth against unexpected account substitution.
- **`init` (not `init_if_needed`)** for the singleton PDA — prevents re-initialization attack by construction; the PDA can be created exactly once.
- **`checked_add` / `checked_sub`** for `total_liquidity` updates — graceful failure on overflow/underflow.
- **`burn_checked`** instead of plain `burn` — enforces decimal expectations at burn time (this is the source of the F-2 risk).
- **Tight token-account constraints** — every token account is validated for its mint, owner, and (where applicable) amount.

What's missing as defense-in-depth (the Low findings):

- **No `freeze_authority` check** on the bridged-token mint at init — F-1.
- **No `decimals` check** on the bridged-token mint at init — F-2.
- The program defers both to off-chain deployment discipline. For the Olas mainnet deployment specifically, the deployment script (`scripts/lockbox_init.ts:73`) sets both correctly. But a future fork / re-deployment without the same discipline would silently mis-configure.

The design choice «hardcode everything; trust the deployer for the few things not hardcoded» is internally consistent. The two additions (F-1, F-2) would close the remaining trust-the-deployer surface and make the program self-defending.

---

## 4. Inventory

| File | LoC | Role |
|---|---|---|
| `programs/liquidity_lockbox/src/lib.rs` | 711 | All instruction logic (`initialize`, `deposit`, `withdraw`); 3 account-validation structs; error code enum; 2 event structs |
| `programs/liquidity_lockbox/src/state.rs` | 50 | `LiquidityLockbox` PDA struct (bump, bridged-mint, fee-collector ATAs, position, pda_position_account, total_liquidity); `initialize()` setter; `seeds()` helper |
| **Total** | **761** | |

Program ID: `7ahQGWysExobjeZ91RTsNqTCN3kWyHGZ43ud2vB7VVoZ` (localnet) / `1BoXeb8hobfLCHNsyCoG1jpEv41ez4w4eDrJ48N1jY3` (mainnet, per README + scripts).

External program dependencies (CPI):
- **Orca Whirlpool** program (`whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc`) — `increase_liquidity`, `decrease_liquidity`, `update_fees_and_rewards`, `collect_fees`.
- **SPL Token** program (`Token::ID`) — `mint_to`, `burn_checked`, `transfer` (via Anchor `token::approve` / Anchor's CPI for transfer-into-position).

---

## 5. Invariants checked

Per AGENT-RULES.md exhaustive-checking discipline, the following invariants were independently verified by source reading:

| # | Invariant | Status |
|---|---|---|
| I-01 | `lockbox.total_liquidity == position.liquidity` after every deposit/withdraw | ✓ enforced by symmetric `checked_add` / `checked_sub` + atomic CPI to Orca |
| I-02 | bridged-token total_supply == lockbox.total_liquidity | ✓ mint and burn happen 1:1 with liquidity_amount in deposit/withdraw |
| I-03 | Only the lockbox PDA can authorize `increase_liquidity` / `decrease_liquidity` / `collect_fees` on the position | ✓ position_authority = pda_position_account.owner = lockbox PDA; lockbox signs CPIs with its own seeds |
| I-04 | The position NFT cannot leave `pda_position_account` | ✓ no instruction in the program transfers the position NFT; position-NFT moves require pda_position_account.owner (= lockbox PDA) to sign, which it never does for transfer |
| I-05 | Re-initialization is impossible | ✓ `init` (not `init_if_needed`); duplicate `initialize` call fails because PDA already exists |
| I-06 | Position must have 0 liquidity at init time | ✓ explicit `if liquidity != 0 → LiquidityNotZero` |
| I-07 | Position must be in OLAS-SOL whirlpool at init time | ✓ `if whirlpool != WHIRLPOOL → WrongWhirlpool` |
| I-08 | Position must be full-range at init time | ✓ `if tick_lower != TICK_LOWER_INDEX || tick_upper != TICK_UPPER_INDEX → OutOfRange` |
| I-09 | Position must be Orca-owned (account-owner program-id check) | ✓ explicit `owner != &ORCA → WrongOwner` |
| I-10 | Position-PDA address derivation match | ✓ `find_program_address([b"position", position_mint], &ORCA)` equals position.key() |
| I-11 | Bridged-token mint authority = lockbox PDA at init | ✓ Anchor constraint `mint_authority.unwrap() == lockbox.key()` |
| I-12 | Bridged-token freeze authority = None at init | **✗ NOT enforced — see F-1** |
| I-13 | Bridged-token decimals = 8 at init | **✗ NOT enforced — see F-2** |
| I-14 | Bridged-token mint stored in lockbox state and validated on every subsequent op | ✓ `address = lockbox.bridged_token_mint` constraint on deposit + withdraw |
| I-15 | Fee-collector token accounts stored in lockbox state and validated on every withdraw | ✓ `address = lockbox.fee_collector_token_owner_account_*` constraints |
| I-16 | Fee-collector token account A holds SOL at init | ✓ `if fee_collector_token_owner_account_a.mint != SOL → WrongTokenMint` |
| I-17 | Fee-collector token account B holds OLAS at init | ✓ `if fee_collector_token_owner_account_b.mint != OLAS → WrongTokenMint` |
| I-18 | Token mints during deposit / withdraw match SOL and OLAS hardcoded addresses | ✓ runtime checks on every instruction |
| I-19 | Token vault addresses match whirlpool's declared vaults | ✓ `token_vault_a.key() == whirlpool.token_vault_a` etc. |
| I-20 | Tick arrays are owned by whirlpool program | ✓ `tick_array_lower.to_account_info().owner == &whirlpool_program.key()` |
| I-21 | Tick arrays belong to the audited whirlpool | ✓ `has_one = whirlpool` constraint |
| I-22 | User can only burn their own bridged tokens | ✓ `signer.key == &bridged_token_account.owner` constraint |
| I-23 | User can only receive into their own token-owner accounts on withdraw | ✓ `signer.key == &token_owner_account_*.owner` |
| I-24 | Deposit cannot exceed user's available token balance + approval | ✓ Anchor `token::approve` enforces approval limits; Orca `increase_liquidity` enforces actual transfers via SPL Token |
| I-25 | Withdraw cannot drain more liquidity than the position has | ✓ `if amount > position.liquidity → AmountExceedsPositionLiquidity` |
| I-26 | Withdraw cannot drain more bridged tokens than caller holds | ✓ SPL `burn_checked` enforces balance |
| I-27 | Slippage protection on deposit / withdraw | ✓ `token_max_a/b` on deposit, `token_min_a/b` on withdraw passed through to Orca CPI |
| I-28 | Position-mint supply == 1 (NFT property) | ✓ constraint `position_mint.supply == 1` |
| I-29 | `pda_position_account` holds exactly the position NFT (amount == 1) | ✓ constraint `pda_position_account.amount == 1` and `pda_position_account.mint == position_mint.key()` |
| I-30 | Solana runtime forbids cross-program reentrancy | ✓ by-construction — programs cannot re-enter themselves on Solana |
| I-31 | Arithmetic overflow protection on `total_liquidity` | ✓ `checked_add` / `checked_sub`; `overflow-checks = true` for release-profile catch on any other arithmetic |
| I-32 | Whirlpool program ID validation on every instruction | ✓ Anchor `Program<'info, whirlpool::program::Whirlpool>` + explicit `whirlpool_program.key() != ORCA` runtime check (defense-in-depth) |
| I-33 | Token program ID validation | ✓ `address = token::ID` constraint |
| I-34 | Reading from arbitrary accounts is constrained | ✓ All accounts are typed (`Box<Account<'info, T>>`); `UncheckedAccount` is not used in this program |

**32 of 34 invariants hold; 2 are gaps closed by Low findings F-1 + F-2.**

---

## 6. Findings

### F-1 — Bridged-token mint `freeze_authority` not enforced at init

**Severity:** Low (deployment-discipline defense-in-depth)
**Anchor:** `programs/liquidity_lockbox/src/lib.rs:462` (init constraint)

```rust
#[account(constraint = bridged_token_mint.mint_authority.unwrap() == lockbox.key())]
pub bridged_token_mint: Box<Account<'info, Mint>>,
```

The init constraint validates that the bridged-token mint's `mint_authority` is the lockbox PDA. It does **not** validate that `freeze_authority` is `None`. If a deployment leaves `freeze_authority` set to any account (an EOA, a multisig, or an arbitrary program), that authority can call `spl_token::freeze_account` against any user's bridged-token account, **permanently blocking that user's ability to withdraw** (because withdraw requires `burn_checked` on the bridged-token account, which a frozen account rejects).

**Current Olas mainnet status (off-chain inference):** the deployment script `scripts/lockbox_init.ts:73` passes `null` for `freezeAuthority`:

```typescript
const bridgedTokenMint = await createMint(provider.connection, userWallet, pdaProgram, null, 8);
//                                                                          ^^^^ freezeAuthority = null
```

So for the specific mainnet deployment, this is presumably fine. **The finding is the program's defensive gap**, not the specific deployment's misconfiguration. On-chain verification of the mainnet mint's actual `freeze_authority` is recommended (see F-9).

**Recommended fix (Anchor 0.26 syntax):**

```rust
#[account(
    constraint = bridged_token_mint.mint_authority.unwrap() == lockbox.key(),
    constraint = bridged_token_mint.freeze_authority.is_none()
)]
pub bridged_token_mint: Box<Account<'info, Mint>>,
```

One additional constraint line; closes the freeze-griefing path against any deployment that forgets the off-chain `null` argument.

---

### F-2 — Bridged-token mint `decimals` not enforced at init

**Severity:** Low (deployment-discipline defense-in-depth)
**Anchor:** `programs/liquidity_lockbox/src/lib.rs:462` (init constraint) + `lib.rs:347` (withdraw `burn_checked`)

The withdraw path uses `burn_checked` with a hardcoded decimals argument:

```rust
const BRIDGED_TOKEN_DECIMALS: u8 = 8;
// ...
&burn_checked(
    ctx.accounts.token_program.key,
    ctx.accounts.bridged_token_account.to_account_info().key,
    ctx.accounts.bridged_token_mint.to_account_info().key,
    ctx.accounts.signer.to_account_info().key,
    &[],
    amount,
    BRIDGED_TOKEN_DECIMALS,  // = 8
)?,
```

`burn_checked` enforces at runtime that the mint's actual decimals match this argument; if they don't, the burn fails. If the bridged-token mint was initialized with `decimals != 8`, **every withdraw call would fail at this step. Deposited funds would be permanently locked.**

The init does not validate `bridged_token_mint.decimals == 8`. The off-chain deployment script does (`createMint(..., null, 8)` — the trailing `8`), but the program is silent.

**Recommended fix:**

```rust
#[account(
    constraint = bridged_token_mint.mint_authority.unwrap() == lockbox.key(),
    constraint = bridged_token_mint.freeze_authority.is_none(),
    constraint = bridged_token_mint.decimals == 8  // ← F-2 fix
)]
pub bridged_token_mint: Box<Account<'info, Mint>>,
```

Failure mode without the fix: silently broken withdrawal pathway — deposits accept tokens, mint bridged tokens, but withdraws never succeed. The user-facing symptom is «my withdrawal failed»; the actual cause is a deployment-time configuration mismatch that should have been caught at init.

---

### F-3 — `token::approve(amount=0)` used to revoke; delegate field stays set

**Severity:** Info (cosmetic; not exploitable)
**Anchor:** `programs/liquidity_lockbox/src/lib.rs:259-281` (revoke-after-deposit block)

After the CPI to `increase_liquidity`, the deposit function sets the delegated approval back to zero on both token accounts:

```rust
// Revoke approval for unused SOL tokens
token::approve(
    CpiContext::new(...),
    0,
)?;
// Revoke approval for OLAS tokens (same pattern)
token::approve(..., 0)?;
```

**Functional behavior:** `token::approve(..., 0)` sets the approved amount to 0 but **leaves the delegate field set to the lockbox PDA**. From a security standpoint this is equivalent to «revoked» — any future delegate transfer would attempt to debit 0 tokens. But the on-chain state is misleading: a casual inspection of the token account shows it has a delegate.

**Idiomatic Solana SPL pattern** is `token::revoke(...)` which clears the delegate to None. Equivalent security, cleaner state.

**Recommendation:** replace the two `approve(..., 0)` calls with `token::revoke(...)`. This is a cosmetic / idiomatic improvement, not a security fix.

---

### F-4 — `find_program_address` for lockbox PDA when stored bump could be used

**Severity:** Info (gas / compute units)
**Anchor:** `lib.rs:140-144` (deposit) and `lib.rs:307-311` (withdraw) — both call:

```rust
let lockbox_pda = Pubkey::find_program_address(&[b"liquidity_lockbox"], &ID);
if lockbox_pda.0 != ctx.accounts.lockbox.key() {
    return Err(ErrorCode::WrongLockboxPDA.into());
}
```

`find_program_address` iterates seeds until a valid bump is found — costs ~12,000 compute units. The lockbox PDA's bump is already stored in `lockbox.bump`. Using `Pubkey::create_program_address(&[b"liquidity_lockbox", lockbox.bump.as_ref()], &ID)` would cost ~1 CU.

This is defense-in-depth — the Anchor `Box<Account<'info, LiquidityLockbox>>` already validates the account discriminator + owner, and the singleton PDA's bump in state is canonical (only one valid bump per seed; init uses `bump` attribute which derives it once). Performing `find_program_address` again at every deposit/withdraw is redundant.

**Recommendation:**

```rust
let lockbox_pda = Pubkey::create_program_address(
    &[b"liquidity_lockbox", ctx.accounts.lockbox.bump.as_ref()],
    &ID
).map_err(|_| ErrorCode::WrongLockboxPDA)?;
if lockbox_pda != ctx.accounts.lockbox.key() {
    return Err(ErrorCode::WrongLockboxPDA.into());
}
```

Saves ~24,000 CU per deposit + ~12,000 CU per withdraw — material on Solana's 1.4M CU per-transaction budget, especially since the deposit transaction already includes a non-trivial Orca CPI.

The same pattern applies to the position-PDA derivation, but the position bump is not stored — `find_program_address` is necessary there.

---

### F-5 — InRange tick check in deposit is essentially a no-op for full-range position

**Severity:** Info (dead code; harmless)
**Anchor:** `lib.rs:165-168`

```rust
// assuming InRange status
if tick_index_current < tick_index_lower || tick_index_upper <= tick_index_current {
    return Err(ErrorCode::OutOfRange.into());
}
```

This check enforces that `tick_index_current ∈ [tick_lower, tick_upper)`. For the lockbox's **full-range position** (`tick_lower = -443584`, `tick_upper = 443584` per `TICK_LOWER_INDEX`/`TICK_UPPER_INDEX`), this is essentially always true — the only way to fail it would be `tick_current_index >= 443584`, which corresponds to a price approaching infinity (unreachable for any realistic OLAS/SOL spot price).

The check appears to be carried over from a narrow-range CL template where in-range vs out-of-range affects whether one or both tokens are needed for `increase_liquidity`. For a full-range position, it's harmless dead code. Reading it makes the reader wonder if narrow-range positions are coming later (they're explicitly out of scope per the hardcoded `TICK_*_INDEX`).

**Recommendation:** either remove the check (and the `// assuming InRange status` comment that signals the intent was different), or add a comment explaining that for full-range positions the check is trivially satisfied except at the absolute tick limits.

---

### F-6 — Toolchain stale (mid-2022) — `cargo audit` flags 2 vulnerabilities + 8 unmaintained warnings

**Severity:** Info (transitive dev-time deps; **NOT in program binary**)
**Source:** `cargo audit` against `Cargo.lock`; pinned versions per `setup-env.sh`, `CLAUDE.md`, and `programs/liquidity_lockbox/Cargo.toml`

Pinned versions (required for Orca Whirlpool CPI compatibility):
- rustc 1.62.0 (June 2022)
- solana-cli 1.14.29 (August 2023)
- anchor-cli 0.26.0 (August 2023)
- spl-token 3.3

`cargo audit` output (2 RUSTSEC vulnerabilities + 8 unmaintained warnings):

| ID | Crate | Title | Solution |
|---|---|---|---|
| RUSTSEC-2024-0344 | `curve25519-dalek` 3.2.1 | Timing variability in `Scalar29/Scalar52::sub` | Upgrade to ≥ 4.1.3 |
| RUSTSEC-2022-0093 | `ed25519-dalek` 1.0.1 | Double Public Key Signing Function Oracle Attack | Upgrade to ≥ 2 |
| RUSTSEC-2024-0375 | `atty` 0.2.14 | unmaintained | — |
| RUSTSEC-2025-0141 | `bincode` 1.3.3 | unmaintained | upgrade |
| RUSTSEC-2025-0161 | `libsecp256k1` 0.6.0 | unmaintained | — |
| RUSTSEC-2024-0436 | `paste` 1.0.15 | unmaintained | — |
| RUSTSEC-2021-0145 | `atty` 0.2.14 | Potential unaligned read | — |
| RUSTSEC-2023-0033 | `borsh` 0.9.3/0.10.3 | Parsing ZST not-copy/clone unsoundness | upgrade |
| RUSTSEC-2026-0012 | `keccak` 0.1.5 | **YANKED** — ARMv8 ASM backend unsoundness | upgrade |
| RUSTSEC-2026-0097 | `rand` (various) | Unsound with custom logger using `rand::rng()` | upgrade |

**Risk assessment for this program:** all listed crates are **transitive dependencies of the build toolchain** (anchor-cli, solana-cli, anchor-lang, anchor-spl). The on-chain Solana program binary (compiled `cdylib`) does NOT include these crates' runtime code — Solana programs use SVM syscalls (`sol_ed25519_verify`, `sol_keccak256`, etc.) instead of user-space implementations. **Runtime exposure of these vulnerabilities is therefore zero for the deployed program.**

The vulnerabilities WOULD affect:
- The host machine compiling the program (build-time tool surface).
- Off-chain client code (`tests/`, `scripts/`) using these crates directly — but only for local testing / one-shot deployment, not for ongoing user-facing interaction.

**Recommendation:** Anchor 0.26 / Solana 1.14.29 / Rust 1.62 are pinned by an Orca CPI compatibility constraint, per `CLAUDE.md`. The pin is reasonable; refreshing requires Orca to publish an upgrade or the lockbox to update its CPI to a newer Orca whirlpool crate. Until then, the cargo-audit findings are flagged for awareness, not for immediate action. If/when Orca publishes a refreshed crate (or the lockbox migrates to it), the toolchain refresh should clear most of these warnings automatically.

---

### F-7 — `mint_authority.unwrap()` panics if authority is `None`

**Severity:** Info (hygiene)
**Anchor:** `lib.rs:462`

```rust
#[account(constraint = bridged_token_mint.mint_authority.unwrap() == lockbox.key())]
```

`Mint.mint_authority` has type `COption<Pubkey>` (Solana's tagged-option type). Calling `.unwrap()` panics if the value is `COption::None`, which would happen if the mint authority was renounced (set to None) before init.

Practical impact: the panic causes the Anchor program to abort and the transaction to fail with a runtime error. From a security standpoint this is acceptable — init simply fails cleanly, no state corruption — but it's a non-idiomatic pattern that produces a less-informative error than a typed check.

**Recommendation:**

```rust
#[account(
    constraint = bridged_token_mint.mint_authority == anchor_spl::token::spl_token::solana_program::program_option::COption::Some(lockbox.key())
)]
```

(Or use a helper / `.map(...) == Some(...)`.) Improves error clarity; same security guarantees.

---

### F-8 — `signer: Signer<'info>` not marked `#[account(mut)]` in withdraw

**Severity:** Info (style inconsistency)
**Anchor:** `lib.rs:567` (withdraw accounts struct) vs `lib.rs:451` (init) and `lib.rs:496` (deposit)

```rust
// init and deposit:
#[account(mut)]
pub signer: Signer<'info>,

// withdraw — no `mut`:
pub signer: Signer<'info>,
```

The signer in withdraw is the transaction fee-payer; in Anchor 0.26's account meta-derivation, the fee-payer is conventionally marked `#[account(mut)]` for consistency. The runtime handles fee-payment regardless of Anchor's `mut` marker (the writable flag is set by the Solana CLI / RPC layer at transaction construction), so this likely does not cause functional breakage — but it's inconsistent with the init / deposit pattern.

**Recommendation:** add `#[account(mut)]` to withdraw's signer for consistency with init and deposit.

---

### F-9 — On-chain bytecode-vs-source verification is a recommended follow-up

**Severity:** Info (out-of-scope for source audit; flagged for completeness)
**Anchor:** README.md + `scripts/lockbox_*.ts` reference mainnet program ID `1BoXeb8hobfLCHNsyCoG1jpEv41ez4w4eDrJ48N1jY3`

This audit covers the source code at HEAD `5fa56d0`. The mainnet program has been deployed and is in use. **Whether the deployed program account's bytecode actually matches a clean build of this source** is a separate verification step — one I have not performed in this audit.

The standard tooling for this:
- `anchor verify <program-id>` (if anchor.toml's `[programs.localnet]` / `[programs.mainnet]` is set up).
- Manual: build the source with the pinned toolchain (`anchor build`), then compare `target/deploy/liquidity_lockbox.so` SHA-256 with the on-chain bytecode (fetched via `solana program dump`).
- Verify the on-chain `LiquidityLockbox` PDA state at the deployed address: bridged_token_mint, fee_collector_token_owner_account_*, position, pda_position_account, total_liquidity — should reflect a normal in-use state.

**Specifically worth checking on-chain to close F-1 and F-2 for the mainnet deployment:**

- Fetch the bridged_token_mint (`CeZ77ti3nPAmcgRkBkUC1JcoAhR8jRti2DHaCcuyUnzR` per `scripts/lockbox_init.ts` console output) and verify:
  - `mint_authority == lockbox PDA`
  - `freeze_authority == None`
  - `decimals == 8`
- If any of these are not as expected, that's a separate finding against the deployment.

This is a recommended next step after this source audit; it cannot be performed from inside the audit-source-only workflow.

---

## 7. Anchor security pattern checklist

| # | Pattern | Status | Notes |
|---|---|---|---|
| A-01 | PDA seed verification (`seeds = true`) | ✓ PASS | Anchor.toml enables this |
| A-02 | Signer enforcement | ✓ PASS | `signer: Signer<'info>` in all three instructions |
| A-03 | `init` PDA space matches struct serialized size | ✓ PASS | `LiquidityLockbox::LEN = 8 + 1 + 32 * 5 + 8 = 177` — matches `[u8; 1] + Pubkey * 5 + u64 + 8B discriminator` |
| A-04 | `init_if_needed` careful usage | ✓ PASS | Not used; plain `init` only — prevents re-init attack |
| A-05 | `realloc` safety | ✓ N/A | No realloc anywhere |
| A-06 | CPI safety (callee program ID + signer seeds) | ✓ PASS | Orca CPI uses `CpiContext::new_with_signer` with lockbox seeds; whirlpool_program ID checked via Anchor type + runtime check |
| A-07 | Owner / discriminator checks on accounts | ✓ PASS | Anchor's `Account<'info, T>` validates owner + discriminator; manual position-discriminator check at init is defense-in-depth |
| A-08 | Account mutability declarations | ⚠ MINOR — F-8 (withdraw signer missing `mut`) | functional; style inconsistency |
| A-09 | Overflow checks (`overflow-checks = true`) | ✓ PASS | Set in both root and program Cargo.toml; explicit `checked_add` / `checked_sub` for total_liquidity |
| A-10 | Reentrancy | ✓ N/A by construction | Solana runtime forbids cross-program reentry |
| A-11 | Closing account / lamports drain | ✓ N/A | No close instruction; no lamport mutations beyond fee-payment |
| A-12 | Token authority transfer correctness | ✓ PASS | All `mint_to` and `burn_checked` use lockbox PDA as authority with signer seeds; user can only burn their own tokens (signer == bridged_token_account.owner) |
| A-13 | Sysvars usage | ✓ PASS | Rent / SystemProgram declared per Anchor convention |
| A-14 | `declare_id!` vs deployed program | ✓ PASS | `declare_id!` matches Anchor.toml localnet ID; mainnet ID matches scripts/README (on-chain verification = F-9) |
| A-15 | Bump persistence (avoid re-`find_program_address`) | ⚠ MINOR — F-4 (re-runs find_program_address in deposit/withdraw) | functional; gas-inefficient |
| A-16 | Token mint constraint completeness | ⚠ Low — F-1 (no freeze_authority check), F-2 (no decimals check) | trust-the-deployer gaps |
| A-17 | Validating CPI program IDs | ✓ PASS | Anchor `Program<'info, T>` + explicit runtime checks against hardcoded `ORCA` constant |
| A-18 | `UncheckedAccount` usage | ✓ N/A | Not used in this program — every account is typed |

---

## 8. Cross-domain pattern sweep

Applicable patterns from general DeFi / DEX / CLMM security catalog:

| Pattern | Applicability | Status |
|---|---|---|
| Re-initialization | ✓ in-scope | PASS — `init` (not `init_if_needed`) for the singleton PDA |
| Front-running on creation | ✓ in-scope | PASS — Config PDA seeded deterministically; init is one-shot for the lockbox singleton |
| Authority bypass on privileged operations | N/A — no privileged operations | ✓ |
| Arithmetic overflow / underflow | ✓ in-scope | PASS — `checked_add`/`checked_sub` + `overflow-checks = true` |
| Oracle dependence | N/A — no oracle dependency | ✓ |
| Slippage protection on swaps / liquidity ops | ✓ in-scope | PASS — `token_max_a/b` (deposit) and `token_min_a/b` (withdraw) passed to Orca CPI |
| In-range vs out-of-range CL semantics | ✓ in-scope (CLMM) | PASS — full-range position is always in-range for practical price ranges; deposit-side InRange check is conservative defense |
| Reentrancy / CPI ordering | ✓ in-scope (CPI to Orca) | PASS — Solana forbids reentry; `update_fees_and_rewards` correctly called before `collect_fees` before `decrease_liquidity` |
| Position-NFT theft | ✓ in-scope | PASS — position NFT is in pda_position_account owned by lockbox PDA; no program instruction transfers it; only Orca's `close_position` could move it (and that requires lockbox PDA to sign, which it never does) |
| Pool / vault address spoofing | ✓ in-scope | PASS — every pool / vault / token account is constrained by hardcoded address or by reference to the validated `whirlpool` account |
| Bridged-token / wrapped-token theft | ✓ in-scope | PASS — bridged-token mint authority = lockbox PDA; users can only burn their OWN bridged tokens (signer == owner constraint); `mint_to` only from inside the program |
| Token-account substitution | ✓ in-scope | PASS — every token account is validated by mint, owner, and (where applicable) amount |
| Permanent-delegate extension on bridged token | ✓ in-scope (SPL token-2022) | N/A — program uses classic SPL Token (`token::ID`), not token-2022; no permanent-delegate extension surface |
| Fee accrual / orphan fees | ✓ in-scope | PASS — `update_fees_and_rewards` is called before `collect_fees` on every withdraw, so accrued fees are flushed to the fee-collector account |
| Liquidity-share fairness (impermanent loss accounting) | ✓ in-scope (LP design) | **Design note** — bridged token represents liquidity-share, NOT fee-share. Fees flow to fee_collector (governance), not to bridged-token holders. User documentation must reflect this. Not a bug; a design choice worth communicating. |

---

## 9. Adversarial sweep — what's NOT in the dev's claims

Three observations that go beyond verifying the documented intent:

1. **No emergency pause / circuit breaker.** If Orca's whirlpool implementation has a defect that affects the OLAS-SOL pool, or if a major exploit happens against Orca's CL math elsewhere on Solana, the lockbox keeps allowing deposits and withdrawals — there is no operator-pausable instruction to halt activity. For a singleton-position bonding contract over a third-party dependency, this is a risk concentration concern. A pause instruction (gated by an admin / multisig / timelock) would be a meaningful operational addition. **Note**: the upgrade authority for the program is held by the separate `lockbox-governor-solana`, which can re-deploy a patched program if needed — but this is a heavier intervention than a pause, and re-deployment changes the program ID's underlying binary which has its own discovery / coordination cost.
2. **No deposit cap.** A user can deposit any amount of OLAS+SOL into the lockbox — there is no per-user or aggregate cap. For protocol-owned-liquidity, this is intentional (more is better), but it concentrates more TVL in the single Orca position, amplifying the F-9 / single-position-dependency risk.
3. **Bridged-token economics worth user-facing clarification.** The bridged token is a claim on liquidity, NOT a claim on fees. Users withdrawing receive OLAS+SOL pro-rata to liquidity, with no fee earnings — fees route to `fee_collector_token_owner_account_*` (governance) instead. This is the intended design but is unusual relative to standard LP tokens; user-facing materials should make it explicit so users don't expect Uniswap-style LP fee earnings.

---

## 10. Compliance report

| Item | Status |
|---|---|
| All `.rs` files read (761 LoC, 2 files) | ✓ |
| Exhaustive Anchor + Solana + DeFi pattern checklist | ✓ (§5, §7, §8) |
| Cross-domain patterns applied — DEX, CLMM, bridged-token, vault | ✓ (§8) |
| `cargo audit` run | ✓ (F-6) |
| Grep sweep for hidden code (CPI, math, transfers, authority changes, mint, burn, unsafe, unwrap) | ✓ |
| README / Anchor.toml / Cargo.toml / scripts metadata cross-checked | ✓ |
| Off-chain deployment script `scripts/lockbox_init.ts` cross-referenced for actual mint init parameters | ✓ (informs F-1, F-2 analysis) |
| Adversarial sweep (what's NOT in dev's claims) | ✓ (§9) |
| Verdict tied to evidence; no premature «all clear» | ✓ — 9 findings documented; «no exploitable issue found» backed by §5 (34 invariants) + §7 + §8 |
| Independent of prior audits per audit instructions | ✓ — `audits/internal/` 2024-02 + external Sec3 PDF noted as historical context only |

---

## 11. Forward-look

1. **F-1 + F-2 fixes** are 2 lines of code. Worth landing in any future maintenance PR.
2. **On-chain verification of mainnet bridged-token mint** (F-9) — query `CeZ77ti3nPAmcgRkBkUC1JcoAhR8jRti2DHaCcuyUnzR` (the bridged-token mint per `scripts/lockbox_init.ts`) and confirm `mint_authority`, `freeze_authority`, `decimals`. Closes F-1 / F-2 specifically for the Olas mainnet deployment.
3. **On-chain bytecode verification** — confirm that the deployed program at `1BoXeb8hobfLCHNsyCoG1jpEv41ez4w4eDrJ48N1jY3` is built from this source revision.
4. **Toolchain refresh path** — Anchor 0.26 / Solana 1.14.29 are pinned by Orca CPI compatibility. If/when Orca publishes a refreshed whirlpool crate (or the lockbox migrates to a newer CPI surface), the toolchain refresh would clear most cargo-audit warnings. Until then, F-6 is informational.
5. **F-3 (revoke vs approve(0)), F-4 (gas), F-5 (dead-code), F-7 (unwrap pattern), F-8 (mut consistency)** — small hygiene/gas improvements, none urgent.
6. **Documentation** — ensure user-facing materials clearly state that bridged tokens are a claim on liquidity (not on fees); fees flow to governance.
7. **Architecture-level: pause / cap mechanisms** (§9) — design decisions, not findings; raise with the protocol team if they're not already considered.

---

## 12. Cross-references

- Source under audit: `programs/liquidity_lockbox/` at HEAD `5fa56d0` (2026-05-25)
- Mainnet program ID: `1BoXeb8hobfLCHNsyCoG1jpEv41ez4w4eDrJ48N1jY3`
- Bridged-token mint (per `scripts/lockbox_init.ts:73-75` console output): `CeZ77ti3nPAmcgRkBkUC1JcoAhR8jRti2DHaCcuyUnzR`
- OLAS-SOL whirlpool: `5dMKUYJDsjZkAD3wiV3ViQkuq9pSmWQ5eAzcQLtDnUT3` (Orca program: `whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc`)
- Prior internal audit folder: `audits/internal/` (2024-02, noted as historical context only)
- Project concept doc: `doc/Bonding_mechanism_with_liquidity_on_Solana.pdf`
- Sister Solana repo (separate audit): `lockbox-governor-solana` — holds the upgrade authority + fee-collector control over this program

---

*audit-claude, 77ph, 2026-06-02*
