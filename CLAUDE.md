# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Solana liquidity lockbox for the Olas protocol. Users deposit OLAS and SOL tokens into an Orca Whirlpool full-range position via the lockbox program and receive fungible bridged LP tokens in return. The program uses CPI (Cross-Program Invocation) to interact with the Orca Whirlpool program.

## Required Environment

Exact versions required for successful Orca Whirlpool CPI:
- Anchor CLI: 0.26.0
- Solana CLI: 1.14.29
- Rust: 1.62.0

Use `setup-env.sh` to install the correct environment.

## Build & Test Commands

```bash
# Install dependencies
yarn

# Build
anchor build

# Lint
yarn run lint          # check
yarn run lint:fix      # auto-fix

# Local testing (run validator first in separate terminal)
./validator.sh

# Set env vars
export ANCHOR_PROVIDER_URL=http://127.0.0.1:8899
export ANCHOR_WALLET=artifacts/id.json

# Run initialization test
solana airdrop 10000 9fit3w7t6FHATDaZWotpWqN7NpqgL3Lm1hqUop4hAy8h --url localhost && npx ts-node tests/lockbox_init.ts

# Run integration test (restart validator first)
solana airdrop 10000 9fit3w7t6FHATDaZWotpWqN7NpqgL3Lm1hqUop4hAy8h --url localhost && npx ts-node tests/liquidity_lockbox.ts

# Debug program logs
solana logs -v --url localhost 7ahQGWysExobjeZ91RTsNqTCN3kWyHGZ43ud2vB7VVoZ
```

Tests are run via `npx ts-node` directly (not `anchor test`). The validator must be restarted between test runs because it loads pre-built Whirlpool state from `fork_whirlpool/`.

## Architecture

**Single program:** `liquidity_lockbox` in `programs/liquidity_lockbox/src/`
- `lib.rs` — All instruction logic (initialize, deposit, withdraw) and account validation structs
- `state.rs` — `LiquidityLockbox` account struct and events

**Three instructions:**
1. **initialize** — Sets up the lockbox PDA (seed: `b"liquidity_lockbox"`), validates whirlpool/position, creates fee collector accounts
2. **deposit** — Takes OLAS + SOL, CPI calls Orca `increase_liquidity`, mints bridged tokens 1:1 with liquidity
3. **withdraw** — Burns bridged tokens, CPI calls Orca `decrease_liquidity`, collects and distributes fees

**Key constraints:**
- Only works with the hardcoded OLAS-SOL Whirlpool (`5dMKUYJDsjZkAD3wiV3ViQkuq9pSmWQ5eAzcQLtDnUT3`)
- Full-range positions only (ticks -443584 to 443584, spacing 64)
- All account addresses (Orca program, token mints, whirlpool) are hardcoded constants validated on every instruction

**Local validator setup:** `validator.sh` loads the pre-compiled Whirlpool program and forked account state from `artifacts/` and `fork_whirlpool/` directories into the local validator.

## Program IDs

- Localnet: `7ahQGWysExobjeZ91RTsNqTCN3kWyHGZ43ud2vB7VVoZ`
- Mainnet: `1BoXeb8hobfLCHNsyCoG1jpEv41ez4w4eDrJ48N1jY3`

## Deployment

Deployment scripts are in `scripts/`. See `scripts/deployment.md` for the full procedure. Requires `./scripts/deploy.sh <program_keypair.json> <program_id> <path_to_deployer_key.json>`.
