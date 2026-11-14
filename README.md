## Escrow Program

A trustless token-swap escrow built on Solana using the Anchor framework. It allows a maker to lock Token A into a vault and offer it in exchange for a specified amount of Token B from any taker, with a time-based expiration enforced on-chain. All swaps are atomic — either both sides settle in the same transaction or nothing moves.

---

## Project Structure

```
escrow-program/
├── Anchor.toml
├── Cargo.toml
├── rust-toolchain.toml
└── programs/
    └── escrow-program/
        ├── Cargo.toml
        └── src/
            ├── lib.rs
            ├── constants.rs
            ├── error.rs
            ├── state.rs
            ├── instructions.rs
            └── instructions/
                ├── make.rs
                ├── take.rs
                ├── refund.rs
                └── update.rs
```

---

## Tech Stack

| Tool | Version | Purpose |
|---|---|---|
| Anchor | `1.2.2` | Solana smart contract framework |
| anchor-spl | `1.2.2` | SPL Token and Token-2022 CPI helpers |
| LiteSVM | `0.15.0` | Lightweight in-process Solana VM for testing |
| litesvm-token | `0.15.0` | Mint/ATA helpers for LiteSVM tests |
| Rust | `stable (1.89.0)` | Language |

---

## On-Chain State

### `Escrow` Account

Stored at a PDA derived from `[b"escrow", maker_pubkey, seed_as_le_bytes]`. It holds all the terms of the swap.

```rust
pub struct Escrow {
    pub seed: u64,         // unique seed the maker chose, used in PDA derivation
    pub maker: Pubkey,     // the wallet that created the offer
    pub mint_a: Pubkey,    // token the maker is offering (locked in vault)
    pub mint_b: Pubkey,    // token the maker wants in return
    pub receive: u64,      // amount of Token B the maker expects from the taker
    pub bump: u8,          // canonical PDA bump stored for CPI signing
    pub expiration: i64,   // Unix timestamp after which the taker can no longer accept
}
```

The account uses a custom discriminator (`discriminator = 1`) set at the struct level via Anchor's `#[account(discriminator = 1)]` attribute, which separates it from the program's default Anchor-generated discriminators.

### Vault

The vault is a standard Associated Token Account for `mint_a`, whose authority is the `escrow` PDA itself. Because the authority is a PDA, no private key can move those tokens — only the program can sign for them using `CpiContext::new_with_signer` with the escrow's seeds.

### PDA Derivation

```
seeds = [b"escrow", maker_pubkey, seed.to_le_bytes()]
program = escrow_program::id()
```

The `seed` parameter is a `u64` chosen by the maker at creation time. This allows one maker to have multiple simultaneous escrows open, each with a different seed.

---

## Constants

```rust
pub const ESCROW_SEED: &[u8] = b"escrow";
```

Defined in `constants.rs` and used consistently in PDA derivation across all four instructions.

---

## Instructions

### `make` — Create an Offer (discriminator = 0)

The maker calls this to open an escrow. It does two things in sequence: initializes the `Escrow` state account and deposits Token A into the vault.

**Parameters**

| Parameter | Type | Description |
|---|---|---|
| `seed` | `u64` | Unique seed for PDA derivation |
| `deposit` | `u64` | Amount of Token A to lock in the vault |
| `receive` | `u64` | Amount of Token B expected from the taker |
| `expiration` | `i64` | Absolute Unix timestamp after which `take` is blocked |

**What happens inside**

1. `init_escrow`: Writes all swap terms into the `Escrow` account, including the PDA bump stored from `ctx.bumps.escrow`. The `expiration` is stored as an absolute Unix timestamp — it is the client's responsibility to pass `current_time + duration`.
2. `deposit`: Executes a `transfer_checked` CPI from `maker_ata_a` to `vault`. The maker signs directly as the authority.

**Accounts**

- `maker`: Mutable signer. Pays for account initialization rent.
- `mint_a`: Token the maker is offering. Validated via `mint::token_program = token_program`.
- `mint_b`: Token the maker wants back. Also validated against `token_program`.
- `maker_ata_a`: Maker's ATA for `mint_a`. Source of the deposit.
- `escrow`: Initialized PDA account. Space allocated as `Escrow::DISCRIMINATOR.len() + Escrow::INIT_SPACE`.
- `vault`: Initialized ATA owned by the `escrow` PDA for `mint_a`. Destination of the deposit.

---

### `take` — Accept the Offer (discriminator = 1)

The taker calls this to settle the swap atomically. The entire trade — Token B to the maker, Token A to the taker, vault closure — happens in one transaction. It includes built-in slippage/front-running protection to ensure the maker cannot front-run the trade by increasing the price or draining the vault via `update`.

**Parameters**

| Parameter | Type | Description |
|---|---|---|
| `max_expected_receive` | `u64` | Maximum amount of Token B the taker agrees to pay (reverts if `escrow.receive > max_expected_receive`) |
| `min_expected_deposit` | `u64` | Minimum amount of Token A the taker expects to receive from the vault (reverts if `vault.amount < min_expected_deposit`) |

**What happens inside (in order)**

1. `check_expiration`: Reads `Clock::get()?.unix_timestamp` and requires it to be strictly less than `escrow.expiration`. Fails with `ErrorCode::EscrowExpired` if the offer has expired.
2. `verify_amounts`: Validates that `escrow.receive <= max_expected_receive` (fails with `ErrorCode::PriceMismatch` if price increased) and `vault.amount >= min_expected_deposit` (fails with `ErrorCode::DepositMismatch` if vault deposit was reduced).
3. `deposit`: Executes a `transfer_checked` CPI transferring `escrow.receive` amount of Token B from `taker_ata_b` to `maker_ata_b`. The taker signs as the authority.
4. `withdraw_and_close_vault`: Builds the PDA signer seeds and executes two CPIs using `CpiContext::new_with_signer`:
   - `transfer_checked`: Transfers the entire `vault.amount` of Token A from `vault` to `taker_ata_a`, signed by the escrow PDA.
   - `close_account`: Closes the vault account and sends its rent lamports to `maker`.

The `escrow` account itself is closed via Anchor's `close = maker` constraint, which also returns rent to the maker.

**Accounts**

- `taker`: Mutable signer. Pays for any ATA initialization needed.
- `maker`: Mutable `SystemAccount`. Receives vault rent and Token B via their ATA.
- `mint_a`: Token the maker offered, the taker receives.
- `mint_b`: Token the taker pays, the maker receives.
- `taker_ata_a`: Taker's ATA for `mint_a`. Initialized with `init_if_needed` if it does not exist, paid for by the taker.
- `taker_ata_b`: Taker's existing ATA for `mint_b`. Must already exist and hold sufficient balance.
- `maker_ata_b`: Maker's ATA for `mint_b`. Initialized with `init_if_needed` if it does not exist, paid for by the taker.
- `escrow`: Validated with `has_one = maker`, `has_one = mint_a`, `has_one = mint_b`, PDA seeds, and bump. Closed on completion.
- `vault`: The escrow's ATA for `mint_a`. Drained and closed on completion.

**PDA Signing**

```rust
let signer_seeds: [&[&[u8]]; 1] = [&[
    ESCROW_SEED,                          // &[u8]
    self.maker.key.as_ref(),              // &[u8]  (32 bytes)
    &self.escrow.seed.to_le_bytes()[..],  // &[u8]  (8 bytes)
    &[self.escrow.bump],                  // &[u8]  (1 byte)
]];
```

This reconstructs the escrow PDA's signing authority at runtime. `new_with_signer` receives `&signer_seeds` which Rust coerces from `&[&[&[u8]]; 1]` to `&[&[&[u8]]]` via deref coercion. The runtime re-derives the PDA from these seeds, confirms it matches the escrow account address, and then permits the signed CPI.

---

### `refund` — Cancel and Reclaim (discriminator = 2)

The maker calls this to cancel an open escrow at any time and retrieve their locked Token A.

**Parameters**

None.

**What happens inside**

1. Builds the same PDA signer seeds as in `take`.
2. `transfer_checked`: Transfers the entire `vault.amount` of Token A from `vault` back to `maker_ata_a`, signed by the escrow PDA.
3. `close_account`: Closes the vault and returns its rent lamports to the maker.
4. The `escrow` account is closed via `close = maker`.

**Note on timing**: There is no expiration check in `refund`. The maker can cancel at any time, even before the offer has expired. This is an open limit-order style design.

**Accounts**

- `maker`: Mutable signer. Only the original maker (enforced by `has_one = maker`) can call this.
- `mint_a`: Token in the vault.
- `maker_ata_a`: Destination for the returned tokens.
- `escrow`: Validated with `has_one = maker`, `has_one = mint_a`, PDA seeds, and bump. Closed on completion.
- `vault`: Drained and closed on completion.

---

### `update` — Modify an Existing Offer (discriminator = 3)

Only the original maker can call this. All three parameters are `Option` types — passing `None` for any of them leaves that field unchanged. This allows the maker to update one, two, or all three fields in a single transaction.

**Parameters**

| Parameter | Type | Description |
|---|---|---|
| `deposit` | `Option<u64>` | New target vault balance. Deposits more or withdraws partial tokens accordingly. |
| `receive` | `Option<u64>` | New amount of Token B the maker wants from the taker. Must be greater than 0. |
| `expiration` | `Option<i64>` | New absolute Unix timestamp for expiration. Must be strictly in the future. |

**What happens inside (in order)**

1. `adjust_deposit`: If `deposit` is `Some(target)`:
   - If `target > vault.amount`: deposits the difference from `maker_ata_a` into `vault` via `transfer_checked` with the maker as the direct signer.
   - If `target < vault.amount`: withdraws the difference from `vault` back to `maker_ata_a` via PDA-signed `transfer_checked`.
   - If `target == vault.amount`: no token movement occurs.
   - Rejects `target == 0` with `ErrorCode::InvalidAmount`.

2. `update_escrow`: If `receive` is `Some(v)`, validates `v > 0` and writes `escrow.receive = v`. If `expiration` is `Some(t)`, validates `t > Clock::get()?.unix_timestamp` and writes `escrow.expiration = t`. Fails with `ErrorCode::InvalidExpiration` if the new timestamp is not in the future.

**Key behavior: Reactivating expired escrows**

Because there is no check preventing `update` on an expired escrow, the maker can extend a timed-out offer by passing a new future timestamp. This reactivates it without having to close and recreate the escrow account.

**Accounts**

- `maker`: Mutable signer. `has_one = maker` on the escrow ensures only the original maker can call this.
- `mint_a`: Validated via `mint::token_program = token_program`.
- `maker_ata_a`: Maker's ATA for `mint_a`. Source for top-ups, destination for partial withdrawals.
- `escrow`: Mutable. Validated with `has_one = maker`, `has_one = mint_a`, PDA seeds, and bump.
- `vault`: Mutable. The escrow's ATA for `mint_a`.

---

## Error Codes

Defined in `error.rs`:

| Code | Message | Triggered by |
|---|---|---|
| `EscrowExpired` | "The escrow has expired" | `take` when `Clock >= escrow.expiration` |
| `InvalidAmount` | "Invalid amount" | `update` when `deposit` or `receive` is set to `0` |
| `InvalidExpiration` | "Invalid expiration" | `update` when new expiration is not in the future |
| `PriceMismatch` | "Price mismatch: expected receive amount is higher than accepted" | `take` when `escrow.receive > max_expected_receive` |
| `DepositMismatch` | "Deposit mismatch: vault balance is less than expected minimum" | `take` when `vault.amount < min_expected_deposit` |

---

## Token Program Compatibility

All token accounts use `anchor-spl`'s `token_interface` module (`TokenInterface`, `InterfaceAccount<Mint>`, `InterfaceAccount<TokenAccount>`). This means the program supports both the classic **SPL Token** program and **Token-2022** program in the same instruction. However, both mints in a single escrow must belong to the same token program, since there is only one `token_program` account per instruction.

---

## Testing

Tests live in `programs/escrow-program/tests/test_initialize.rs` and use **LiteSVM**, an in-process Solana virtual machine that runs the compiled `.so` binary without a local validator.

### Test Setup

A shared `setup()` function initializes a fresh LiteSVM instance, loads the compiled program binary at `target/deploy/escrow_program.so`, creates a payer keypair, and airdrops 1 SOL to it.

### Test Cases

**`test_make_and_refund`**
- Creates Mint A and Mint B with 6 decimals.
- Maker creates an escrow offering 10 tokens of Mint A for 10 tokens of Mint B, with a far future expiration.
- Verifies vault holds exactly 10 tokens and all escrow fields are correct.
- Maker calls `refund`. Verifies the escrow and vault accounts are both closed on-chain.

**`test_make_and_take`**
- Creates Mint A and Mint B, creates ATAs for both maker and taker.
- Maker opens an escrow with `expiration = 3600` (Unix timestamp 3600, treated as a future time since LiteSVM starts the clock at 0).
- Advances the LiteSVM clock by 3599 seconds (`clock.unix_timestamp += 3599`) — still before expiration.
- Taker calls `take`. Verifies the escrow and vault are both closed on-chain.

**`test_update_deposit_more_and_withdraw_partial`**
- Maker opens escrow with 10 tokens.
- Calls `update` with `deposit = Some(15_000_000)`, `receive = Some(12_000_000)`, `expiration = Some(7200)`. Verifies vault has 15 tokens and escrow fields reflect the new values.
- Calls `update` again with `deposit = Some(6_000_000)`. Verifies vault is reduced to 6 tokens and the maker's ATA has received the 9 withdrawn tokens back.

**`test_update_expired_escrow_to_extend`**
- Maker opens escrow with `expiration = 3600`.
- Advances clock to `unix_timestamp = 4000` (past expiration).
- Taker's `take` transaction fails as expected (escrow expired).
- Maker calls `update` with `expiration = Some(5000)` to extend.
- Calls `program.expire_blockhash()` to generate a fresh blockhash so the taker's transaction is not rejected as a duplicate.
- Taker's `take` transaction now succeeds. Escrow and vault are both closed.

### Running Tests

```bash
cargo test
```

The test runner compiles the program in test mode, executes all four test cases against the in-process VM, and reports results. No local validator or network connection is required.

---

## Building

```bash
anchor build
```

Produces the compiled SBF binary at `target/deploy/escrow_program.so` and a generated IDL at `target/idl/escrow_program.json`.

---

## Program ID

```
52swaUpav6PwmQbR4bhiz7kawNzGDesWhfTcxbZ74iEG
```

Declared in `lib.rs` via `declare_id!` and registered in `Anchor.toml` under `[programs.localnet]`.
