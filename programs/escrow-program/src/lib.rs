pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("52swaUpav6PwmQbR4bhiz7kawNzGDesWhfTcxbZ74iEG");

#[program]
pub mod escrow_program {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn make(
        ctx: Context<Make>,
        seed: u64,
        deposit: u64,
        receive: u64,
        expiration: i64,
    ) -> Result<()> {
        ctx.accounts
            .init_escrow(seed, receive, &ctx.bumps, expiration)?;
        ctx.accounts.deposit(deposit)
    }

    #[instruction(discriminator = 1)]
    pub fn take(
        ctx: Context<Take>,
        max_expected_receive: u64,
        min_expected_deposit: u64,
    ) -> Result<()> {
        ctx.accounts.check_expiration()?;
        ctx.accounts
            .verify_amounts(max_expected_receive, min_expected_deposit)?;
        ctx.accounts.deposit()?;
        ctx.accounts.withdraw_and_close_vault()
    }

    #[instruction(discriminator = 2)]
    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        ctx.accounts.refund_and_close_vault()
    }

    #[instruction(discriminator = 3)]
    pub fn update(
        ctx: Context<Update>,
        deposit: Option<u64>,
        receive: Option<u64>,
        expiration: Option<i64>,
    ) -> Result<()> {
        ctx.accounts.adjust_deposit(deposit)?;
        ctx.accounts.update_escrow(receive, expiration)
    }
}
