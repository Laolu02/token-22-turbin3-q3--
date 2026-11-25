//! Anchor CPI handlers for Token-2022 confidential transfers.
//!
//! The client generates and verifies ZK proofs, then supplies the resulting
//! context-state accounts. The token-account owner signs each operation.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;

use anchor_spl::token_interface::TokenInterface;

use crate::spl_token_2022::{extension::confidential_transfer::{
    instruction as confidential_ix,
    DecryptableBalance,}, 
    solana_zk_sdk::encryption::pod::elgamal::PodElGamalCiphertext
};

use spl_token_confidential_transfer_proof_extraction::instruction::ProofLocation;

// -----------------------------------------------------------------------------
// CONFIGURE ACCOUNT
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct ConfigureConfidentialAccount<'info> {
    /// CHECK: Token-2022 validates the account layout and mint association.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: Token-2022 validates this mint.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: Pre-verified VerifyPubkeyValidity context-state account.
    pub pubkey_validity_context: UncheckedAccount<'info>,

    /// The token-account owner must sign. ATA creation authority is not enough.
    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_configure_confidential_account(
    a: &ConfigureConfidentialAccount,
    zero_balance: [u8; 36],
    max_pending_credit_counter: u64,
) -> Result<()> {
    let zero = DecryptableBalance::from(zero_balance);
    let context_key = a.pubkey_validity_context.key();

    let proof_location =
        ProofLocation::ContextStateAccount(&context_key);

    let ix = confidential_ix::inner_configure_account(
        &a.token_program.key(),
        &a.token_account.key(),
        &a.mint.key(),
        &zero,
        max_pending_credit_counter,
        &a.owner.key(),
        &[],
        proof_location,
    )?;

    invoke(
        &ix,
        &[
            a.token_account.to_account_info(),
            a.mint.to_account_info(),
            a.pubkey_validity_context.to_account_info(),
            a.owner.to_account_info(),
            a.token_program.to_account_info(),
        ],
    )?;

    Ok(())
}

// -----------------------------------------------------------------------------
// APPROVE ACCOUNT
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct ApproveConfidentialAccount<'info> {
    /// CHECK: Token-2022 validates this configured token account.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: Token-2022 validates the confidential-transfer authority.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// Must be the confidential-transfer mint authority.
    pub issuer: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_approve_confidential_account(
    a: &ApproveConfidentialAccount,
) -> Result<()> {
    let ix = confidential_ix::approve_account(
        &a.token_program.key(),
        &a.token_account.key(),
        &a.mint.key(),
        &a.issuer.key(),
        &[],
    )?;

    invoke(
        &ix,
        &[
            a.token_account.to_account_info(),
            a.mint.to_account_info(),
            a.issuer.to_account_info(),
            a.token_program.to_account_info(),
        ],
    )?;

    Ok(())
}

// -----------------------------------------------------------------------------
// DEPOSIT PUBLIC TOKENS INTO CONFIDENTIAL PENDING BALANCE
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct DepositConfidentialTokens<'info> {
    /// CHECK: Token-2022 validates the confidential token account.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: Token-2022 validates mint association.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_deposit_confidential_tokens(
    a: &DepositConfidentialTokens,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    let ix = confidential_ix::deposit(
        &a.token_program.key(),
        &a.token_account.key(),
        &a.mint.key(),
        amount,
        decimals,
        &a.owner.key(),
        &[],
    )?;

    invoke(
        &ix,
        &[
            a.token_account.to_account_info(),
            a.mint.to_account_info(),
            a.owner.to_account_info(),
            a.token_program.to_account_info(),
        ],
    )?;

    Ok(())
}

// -----------------------------------------------------------------------------
// APPLY PENDING BALANCE
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct ApplyConfidentialPendingBalance<'info> {
    /// CHECK: Token-2022 validates the confidential account extension.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_apply_confidential_pending_balance(
    a: &ApplyConfidentialPendingBalance,
    expected_credit_counter: u64,
    new_decryptable_available_balance: [u8; 36],
) -> Result<()> {
    let balance =
        DecryptableBalance::from(new_decryptable_available_balance);

    let ix = confidential_ix::apply_pending_balance(
        &a.token_program.key(),
        &a.token_account.key(),
        expected_credit_counter,
        &balance,
        &a.owner.key(),
        &[],
    )?;

    invoke(
        &ix,
        &[
            a.token_account.to_account_info(),
            a.owner.to_account_info(),
            a.token_program.to_account_info(),
        ],
    )?;

    Ok(())
}

// -----------------------------------------------------------------------------
// CONFIDENTIAL TRANSFER
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct TransferConfidentialTokens<'info> {
    /// CHECK: Token-2022 validates the source account and its mint.
    #[account(mut, owner = token_program.key())]
    pub source: UncheckedAccount<'info>,

    /// CHECK: Token-2022 validates the mint.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: Token-2022 validates the destination account.
    #[account(mut, owner = token_program.key())]
    pub destination: UncheckedAccount<'info>,

    /// Pre-verified VerifyCiphertextCommitmentEquality context.
    /// CHECK: Token-2022 validates the proof context.
    pub equality_proof_context: UncheckedAccount<'info>,

    /// Pre-verified VerifyBatchedGroupedCiphertext3HandlesValidity context.
    /// CHECK: Token-2022 validates the proof context.
    pub ciphertext_validity_context: UncheckedAccount<'info>,

    /// Pre-verified VerifyBatchedRangeProofU128 context.
    /// CHECK: Token-2022 validates the proof context.
    pub range_proof_context: UncheckedAccount<'info>,

    /// Source token-account owner must sign.
    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_transfer_confidential_tokens(
    a: &TransferConfidentialTokens,
    new_source_decryptable_balance: [u8; 36],
    transfer_amount_auditor_ciphertext_lo: [u8; 64],
    transfer_amount_auditor_ciphertext_hi: [u8; 64],
) -> Result<()> {
    let new_balance =
        DecryptableBalance::from(new_source_decryptable_balance);

    let ciphertext_lo: PodElGamalCiphertext =
        bytemuck::pod_read_unaligned(&transfer_amount_auditor_ciphertext_lo);

    let ciphertext_hi: PodElGamalCiphertext =
        bytemuck::pod_read_unaligned(&transfer_amount_auditor_ciphertext_hi);

    let equality_key = a.equality_proof_context.key();
    let validity_key = a.ciphertext_validity_context.key();
    let range_key = a.range_proof_context.key();

    let ix = confidential_ix::inner_transfer(
        &a.token_program.key(),
        &a.source.key(),
        &a.mint.key(),
        &a.destination.key(),
        &new_balance,
        &ciphertext_lo,
        &ciphertext_hi,
        &a.owner.key(),
        &[],
        ProofLocation::ContextStateAccount(&equality_key),
        ProofLocation::ContextStateAccount(&validity_key),
        ProofLocation::ContextStateAccount(&range_key),
    )?;

    invoke(
        &ix,
        &[
            a.source.to_account_info(),
            a.mint.to_account_info(),
            a.destination.to_account_info(),
            a.equality_proof_context.to_account_info(),
            a.ciphertext_validity_context.to_account_info(),
            a.range_proof_context.to_account_info(),
            a.owner.to_account_info(),
            a.token_program.to_account_info(),
        ],
    )?;

    Ok(())
}

// -----------------------------------------------------------------------------
// WITHDRAW CONFIDENTIAL TOKENS INTO PUBLIC BALANCE
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct WithdrawConfidentialTokens<'info> {
    /// CHECK: Token-2022 validates the confidential token account.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: Token-2022 validates the mint.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// Pre-verified VerifyCiphertextCommitmentEquality context.
    /// CHECK: Token-2022 validates the proof context.
    pub equality_proof_context: UncheckedAccount<'info>,

    /// Pre-verified VerifyBatchedRangeProofU64 context.
    /// CHECK: Token-2022 validates the proof context.
    pub range_proof_context: UncheckedAccount<'info>,

    /// Token-account owner must sign.
    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_withdraw_confidential_tokens(
    a: &WithdrawConfidentialTokens,
    amount: u64,
    decimals: u8,
    new_decryptable_available_balance: [u8; 36],
) -> Result<()> {
    let new_balance =
        DecryptableBalance::from(new_decryptable_available_balance);

    let equality_key = a.equality_proof_context.key();
    let range_key = a.range_proof_context.key();

    let ix = confidential_ix::inner_withdraw(
        &a.token_program.key(),
        &a.token_account.key(),
        &a.mint.key(),
        amount,
        decimals,
        &new_balance,
        &a.owner.key(),
        &[],
        ProofLocation::ContextStateAccount(&equality_key),
        ProofLocation::ContextStateAccount(&range_key),
    )?;

    invoke(
        &ix,
        &[
            a.token_account.to_account_info(),
            a.mint.to_account_info(),
            a.equality_proof_context.to_account_info(),
            a.range_proof_context.to_account_info(),
            a.owner.to_account_info(),
            a.token_program.to_account_info(),
        ],
    )?;

    Ok(())
}