use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::{error::ErrorCode, state::Escrow, ESCROW_SEED};

#[derive(Accounts)]
pub struct Update<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    #[account(
        mint::token_program = token_program
    )]
    pub mint_a: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker,
        associated_token::token_program = token_program
    )]
    pub maker_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        has_one = maker,
        has_one = mint_a,
        seeds = [ESCROW_SEED, maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, Escrow>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
        associated_token::token_program = token_program
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

impl<'info> Update<'info> {
    pub fn update_escrow(
        &mut self,
        receive: Option<u64>,
        expiration: Option<i64>,
    ) -> Result<()> {
        if let Some(new_receive) = receive {
            require_gt!(new_receive, 0, ErrorCode::InvalidAmount);
            self.escrow.receive = new_receive;
        }

        if let Some(new_expiration) = expiration {
            require_gt!(
                new_expiration,
                Clock::get()?.unix_timestamp,
                ErrorCode::InvalidExpiration
            );
            self.escrow.expiration = new_expiration;
        }

        Ok(())
    }

    pub fn adjust_deposit(&mut self, deposit: Option<u64>) -> Result<()> {
        if let Some(target_deposit) = deposit {
            require_gt!(target_deposit, 0, ErrorCode::InvalidAmount);
            let current_amount = self.vault.amount;

            if target_deposit > current_amount {
                let amount_to_deposit = target_deposit - current_amount;
                self.deposit_to_vault(amount_to_deposit)?;
            } else if target_deposit < current_amount {
                let amount_to_withdraw = current_amount - target_deposit;
                self.withdraw_from_vault(amount_to_withdraw)?;
            }
        }
        Ok(())
    }

    fn deposit_to_vault(&self, amount: u64) -> Result<()> {
        let transfer_accounts = TransferChecked {
            from: self.maker_ata_a.to_account_info(),
            mint: self.mint_a.to_account_info(),
            to: self.vault.to_account_info(),
            authority: self.maker.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(self.token_program.key(), transfer_accounts);
        transfer_checked(cpi_ctx, amount, self.mint_a.decimals)
    }

    fn withdraw_from_vault(&self, amount: u64) -> Result<()> {
        let signer_seeds: [&[&[u8]]; 1] = [&[
            ESCROW_SEED,
            self.maker.key.as_ref(),
            &self.escrow.seed.to_le_bytes()[..],
            &[self.escrow.bump],
        ]];

        let transfer_accounts = TransferChecked {
            from: self.vault.to_account_info(),
            to: self.maker_ata_a.to_account_info(),
            mint: self.mint_a.to_account_info(),
            authority: self.escrow.to_account_info(),
        };

        let cpi_context = CpiContext::new_with_signer(
            self.token_program.key(),
            transfer_accounts,
            &signer_seeds,
        );

        transfer_checked(cpi_context, amount, self.mint_a.decimals)
    }
}
