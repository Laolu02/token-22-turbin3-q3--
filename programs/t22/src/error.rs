use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Only the counter authority can update this counter")]
    Unauthorized,

    #[msg("Counter has reached the maximum value")]
    CounterOverflow,

    #[msg("Mint carries an extension this program has not been written to handle")]
    UnsupportedExtension,

    #[msg("Mint does not have a transfer-fee configuration")]
    MissingTransferFeeConfig,

    #[msg("Could not calculate the transfer fee for the current epoch")]
    FeeCalculationFailed,

    #[msg("Source or destination token account is not in the initialized state")]
    TokenAccountNotTransferable,

    #[msg("Token account is not frozen")]
    TokenAccountNotFrozen,
}