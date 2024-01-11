use anchor_lang::prelude::*;

#[account]
pub struct LiquidityLockbox {
  // Lockbox bump
  pub bump: [u8; 1],
  // Bridged token mint address
  pub bridged_token_mint: Pubkey,
  // Fee collector ATA for token A
  pub fee_collector_token_owner_account_a: Pubkey,
  // Fee collector ATA for token B
  pub fee_collector_token_owner_account_b: Pubkey,
  // Total liquidity in a lockbox
  // Considering OLAS and SOL inflation, it will never practically be bigger than 2^64 - 1
  pub total_liquidity: u64,
  // Total number of lockbox positions
  // Even if position is created every second, it would take 136+ years to create 2^32 - 1 positions
  pub num_positions: u32
}

impl LiquidityLockbox {
  pub const LEN: usize = 8 + 1 + 32 * 3 + 8 + 4;

  pub fn seeds(&self) -> [&[u8]; 2] {
    [
      &b"liquidity_lockbox"[..],
      self.bump.as_ref()
    ]
  }

  pub fn initialize(
    &mut self,
    bump: u8,
    bridged_token_mint: Pubkey,
    fee_collector_token_owner_account_a: Pubkey,
    fee_collector_token_owner_account_b: Pubkey
  ) -> Result<()> {
    self.bridged_token_mint = bridged_token_mint;
    self.fee_collector_token_owner_account_a = fee_collector_token_owner_account_a;
    self.fee_collector_token_owner_account_b = fee_collector_token_owner_account_b;
    self.total_liquidity = 0;
    self.num_positions = 0;
    self.bump = [bump];

    Ok(())
  }
}