#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

declare_id!("11111111111111111111111111111111");

#[program]
pub mod counter {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, value: u64) -> Result<()> {
        ctx.accounts.counter.value = value;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = payer, space = 8 + 8)]
    pub counter: Account<'info, Counter>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct Counter {
    pub value: u64,
}
