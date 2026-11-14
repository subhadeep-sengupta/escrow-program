use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The escrow has expired")]
    EscrowExpired,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Invalid expiration")]
    InvalidExpiration,
    #[msg("Price mismatch: expected receive amount is higher than accepted")]
    PriceMismatch,
    #[msg("Deposit mismatch: vault balance is less than expected minimum")]
    DepositMismatch,
}
