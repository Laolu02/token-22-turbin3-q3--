use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token_interface::{
    initialize_mint2, mint_close_authority_initialize, transfer_fee_initialize,
    InitializeMint2, MintCloseAuthorityInitialize, TokenInterface, TransferFeeInitialize,
};
use crate::spl_token_2022::{
    extension::{
        default_account_state::instruction as default_account_state_instruction,
        metadata_pointer::instruction as metadata_pointer_instruction,
        confidential_transfer::instruction as confidential_instruction,
        confidential_transfer_fee::instruction as confidential_fee_instruction,
        transfer_fee::{instruction as transfer_fee_instruction, TransferFeeConfig},
        BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
    instruction as token_instruction,
    state::{Account as TokenAccountState, AccountState, Mint as MintState},
};

use crate::spl_token_2022::instruction::initialize_permanent_delegate;

use crate::MintError;

#[derive(Accounts)]
pub struct CreateComplianceMint<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: created and initialized by this instruction; must sign for its address.
    #[account(mut, signer)]
    pub mint: UncheckedAccount<'info>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_create_compliance_mint(
    accounts: &CreateComplianceMint,
    decimals: u8,
    basis_points: u16,
    maximum_fee: u64,
    withdraw_withheld_authority_elgamal_pubkey: [u8; 32],
) -> Result<()> {
    let extensions = [
        ExtensionType::TransferFeeConfig,
        ExtensionType::MetadataPointer,
        ExtensionType::DefaultAccountState,
        ExtensionType::MintCloseAuthority,
        ExtensionType::PermanentDelegate,
        ExtensionType::ConfidentialTransferMint,
        ExtensionType::ConfidentialTransferFeeConfig,
    ];
    let space = ExtensionType::try_calculate_account_len::<MintState>(&extensions)?;
    let lamports = Rent::get()?.minimum_balance(space);
    let mint_info = accounts.mint.to_account_info();
    let token_program_info = accounts.token_program.to_account_info();
    let payer_key = accounts.payer.key();

    anchor_lang::system_program::create_account(
        CpiContext::new(
            accounts.system_program.key(),
            anchor_lang::system_program::CreateAccount {
                from: accounts.payer.to_account_info(),
                to: mint_info.clone(),
            },
        ),
        lamports,
        space as u64,
        &accounts.token_program.key(),
    )?;

    // All extension initializers must run before InitializeMint2.
    mint_close_authority_initialize(
        CpiContext::new(
            accounts.token_program.key(),
            MintCloseAuthorityInitialize {
                token_program_id: token_program_info.clone(),
                mint: mint_info.clone(),
            },
        ),
        Some(&payer_key),
    )?;

    transfer_fee_initialize(
        CpiContext::new(
            accounts.token_program.key(),
            TransferFeeInitialize {
                token_program_id: token_program_info.clone(),
                mint: mint_info.clone(),
            },
        ),
        Some(&payer_key),
        Some(&payer_key),
        basis_points,
        maximum_fee,
    )?;

    let metadata_ix = metadata_pointer_instruction::initialize(
        &accounts.token_program.key(),
        &accounts.mint.key(),
        Some(payer_key),
        Some(accounts.mint.key()),
    )?;
    invoke(&metadata_ix, &[mint_info.clone(), token_program_info.clone()])?;

    let default_state_ix = default_account_state_instruction::initialize_default_account_state(
        &accounts.token_program.key(),
        &accounts.mint.key(),
        &AccountState::Frozen,
    )?;
    invoke(&default_state_ix, &[mint_info.clone(), token_program_info.clone()])?;

    // Initialize the permanent delegate extension.
    // The payer becomes the permanent delegate.
    let permanent_delegate_ix =
        initialize_permanent_delegate(
            &accounts.token_program.key(),
            &accounts.mint.key(),
            &payer_key,
        )?;

    invoke(
        &permanent_delegate_ix,
        &[mint_info.clone(), token_program_info.clone()],
    )?;

    // Initialize the confidential transfer mint extension.
    // New accounts require explicit issuer approval.
    let confidential_ix = confidential_instruction::initialize_mint(
        &accounts.token_program.key(),
        &accounts.mint.key(),
        Some(payer_key),
        false,
        None,
    )?;

    invoke(
        &confidential_ix,
        &[mint_info.clone(), token_program_info.clone()],
    )?;

  
    let confidential_fee_ix =
        confidential_fee_instruction::initialize_confidential_transfer_fee_config(
            &accounts.token_program.key(),
            &accounts.mint.key(),
            Some(payer_key),
            &withdraw_withheld_authority_elgamal_pubkey.into(),
        )?;

    invoke(
        &confidential_fee_ix,
        &[mint_info.clone(), token_program_info.clone()],
    )?;

    
    initialize_mint2(
        CpiContext::new(
            accounts.token_program.key(),
            InitializeMint2 { mint: mint_info },
        ),
        decimals,
        &payer_key,
        Some(&payer_key),
    )?;

    msg!("compliance mint {} initialized with {} bytes", accounts.mint.key(), space);
    Ok(())
}


#[derive(Accounts)]
pub struct TransferWithCurrentEpochFee<'info> {
    /// CHECK: owner and token-account layout are validated in the handler.
    #[account(mut, owner = token_program.key())]
    pub source: UncheckedAccount<'info>,
    /// CHECK: owner and mint layout are validated in the handler.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,
    /// CHECK: owner and token-account layout are validated in the handler.
    #[account(mut, owner = token_program.key())]
    pub destination: UncheckedAccount<'info>,
    pub authority: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_transfer_with_current_epoch_fee(
    accounts: &TransferWithCurrentEpochFee,
    amount: u64,
) -> Result<()> {
    let mint_info = accounts.mint.to_account_info();
    let mint_data = mint_info.try_borrow_data()?;
    let mint_state = StateWithExtensions::<MintState>::unpack(&mint_data)?;
    let decimals = mint_state.base.decimals;
    let current_epoch = Clock::get()?.epoch;
    let fee_config = mint_state
        .get_extension::<TransferFeeConfig>()
        .map_err(|_| error!(MintError::MissingTransferFeeConfig))?;
    let expected_fee = fee_config
        .calculate_epoch_fee(current_epoch, amount)
        .ok_or_else(|| error!(MintError::FeeCalculationFailed))?;
    drop(mint_data);

    
    let source_data = accounts.source.try_borrow_data()?;
    let source_state = StateWithExtensions::<TokenAccountState>::unpack(&source_data)?;
    require!(
        source_state.base.state == AccountState::Initialized,
        MintError::TokenAccountNotTransferable
    );
    drop(source_data);

    let destination_data = accounts.destination.try_borrow_data()?;
    let destination_state = StateWithExtensions::<TokenAccountState>::unpack(&destination_data)?;
    require!(
        destination_state.base.state == AccountState::Initialized,
        MintError::TokenAccountNotTransferable
    );
    drop(destination_data);

    let ix = transfer_fee_instruction::transfer_checked_with_fee(
        &accounts.token_program.key(),
        &accounts.source.key(),
        &accounts.mint.key(),
        &accounts.destination.key(),
        &accounts.authority.key(),
        &[],
        amount,
        decimals,
        expected_fee,
    )?;
    invoke(
        &ix,
        &[
            accounts.source.to_account_info(),
            mint_info,
            accounts.destination.to_account_info(),
            accounts.authority.to_account_info(),
            accounts.token_program.to_account_info(),
        ],
    )?;

    msg!("transferred {} tokens with current-epoch fee {}", amount, expected_fee);
    Ok(())
}

#[derive(Accounts)]
pub struct ThawKycVerifiedAccount<'info> {
    /// CHECK: token-account layout and frozen state are checked in the handler.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,
    /// CHECK: Token-2022 validates that this mint controls the token account.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,
    /// Trusted KYC/freeze authority; Token-2022 checks it against the mint.
    pub freeze_authority: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_thaw_kyc_verified_account(accounts: &ThawKycVerifiedAccount) -> Result<()> {
    let account_data = accounts.token_account.try_borrow_data()?;
    let account_state = StateWithExtensions::<TokenAccountState>::unpack(&account_data)?;
    require!(
        account_state.base.state == AccountState::Frozen,
        MintError::TokenAccountNotFrozen
    );
    drop(account_data);

    let ix = token_instruction::thaw_account(
        &accounts.token_program.key(),
        &accounts.token_account.key(),
        &accounts.mint.key(),
        &accounts.freeze_authority.key(),
        &[],
    )?;
    invoke(
        &ix,
        &[
            accounts.token_account.to_account_info(),
            accounts.mint.to_account_info(),
            accounts.freeze_authority.to_account_info(),
            accounts.token_program.to_account_info(),
        ],
    )?;

    msg!("KYC verified; thawed token account {}", accounts.token_account.key());
    Ok(())
}






