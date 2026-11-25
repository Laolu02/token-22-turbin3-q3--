//! Integration tests for the compliance instructions currently exposed by t22.
//!
//! Run from the Anchor workspace after building the SBF program:
//!   cargo build-sbf
//!   cargo test -p t22 --test compliance_tests
//!
//! The test binary loads ../../target/deploy/t22.so using LiteSVM.

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
    InstructionData, ToAccountMetas,
};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use t22::{accounts, instruction, ID};
use t22new:: {
    extension::{
        default_account_state::DefaultAccountState,
        mint_close_authority::MintCloseAuthority,
        metadata_pointer::MetadataPointer,
        transfer_fee::{TransferFeeAmount, TransferFeeConfig},
        BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
    instruction::{initialize_account3, mint_to},
    state::{Account as TokenAccountState, AccountState, Mint as MintState},
};

use zk::encryption::derivation::derive_confidential_keys;

const TOKEN_2022_PROGRAM_ID: Pubkey = anchor_spl::token_interface::spl_token_2022::ID;
const DECIMALS: u8 = 6;
const FEE_BPS: u16 = 250;
const MAX_FEE: u64 = 1_000;

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();

    let program_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy/t22.so");
    assert!(
        program_path.exists(),
        "program binary missing at {}. Run cargo build-sbf first.",
        program_path.display()
    );
    svm.add_program_from_file(ID, program_path).unwrap();
    (svm, payer)
}

fn send(
    svm: &mut LiteSVM,
    fee_payer: &Keypair,
    instructions: &[Instruction],
    additional_signers: &[&Keypair],
) {
    let mut signers: Vec<&Keypair> = vec![fee_payer];
    signers.extend_from_slice(additional_signers);
    let mut tx = Transaction::new_unsigned(Message::new(instructions, Some(&fee_payer.pubkey())));
    tx.try_sign(&signers, svm.latest_blockhash()).unwrap();
    if let Err(err) = svm.send_transaction(tx) {
        panic!("transaction failed:\n{:#?}", err.meta.logs);
    }
}

fn send_expect_failure(
    svm: &mut LiteSVM,
    fee_payer: &Keypair,
    instructions: &[Instruction],
    additional_signers: &[&Keypair],
) -> String {
    let mut signers: Vec<&Keypair> = vec![fee_payer];
    signers.extend_from_slice(additional_signers);
    let mut tx = Transaction::new_unsigned(Message::new(instructions, Some(&fee_payer.pubkey())));
    tx.try_sign(&signers, svm.latest_blockhash()).unwrap();
    match svm.send_transaction(tx) {
        Ok(_) => panic!("expected transaction to fail, but it succeeded"),
        Err(err) => err.meta.logs.join("\n"),
    }
}

fn create_compliance_mint(svm: &mut LiteSVM, payer: &Keypair) -> Keypair {
    let mint = Keypair::new();
    let (fee_authority_elgamal, _) =
    derive_confidential_keys(payer, b"").unwrap();

    let fee_authority_pubkey: [u8; 32] =
        fee_authority_elgamal.pubkey().into();
    let ix = Instruction {
        program_id: ID,
        accounts: accounts::CreateComplianceMint {
            payer: payer.pubkey(),
            mint: mint.pubkey(),
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: instruction::CreateComplianceMint {
            decimals: DECIMALS,
            basis_points: FEE_BPS,
            maximum_fee: MAX_FEE,
            withdraw_withheld_authority_elgamal_pubkey: fee_authority_pubkey,
        }
        .data(),
    };
    send(svm, payer, &[ix], &[&mint]);
    mint
}

/// A Token-2022 account for a mint with TransferFeeConfig must include
/// TransferFeeAmount. DefaultAccountState makes it start frozen.
fn create_token_account(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Keypair {
    let token_account = Keypair::new();
    let required = ExtensionType::get_required_init_account_extensions(&[
        ExtensionType::TransferFeeConfig,
    ]);
    let space = ExtensionType::try_calculate_account_len::<TokenAccountState>(&required).unwrap();
    let rent = svm.minimum_balance_for_rent_exemption(space);
    send(
        svm,
        payer,
        &[
            solana_system_interface::instruction::create_account(
                &payer.pubkey(),
                &token_account.pubkey(),
                rent,
                space as u64,
                &TOKEN_2022_PROGRAM_ID,
            ),
            initialize_account3(
                &TOKEN_2022_PROGRAM_ID,
                &token_account.pubkey(),
                mint,
                owner,
            )
            .unwrap(),
        ],
        &[&token_account],
    );
    token_account
}

fn thaw_ix(token_account: &Pubkey, mint: &Pubkey, authority: &Pubkey) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts::ThawKycVerifiedAccount {
            token_account: *token_account,
            mint: *mint,
            freeze_authority: *authority,
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: instruction::ThawKycVerifiedAccount {}.data(),
    }
}

fn read_token_account(svm: &LiteSVM, address: &Pubkey) -> TokenAccountState {
    let account = svm.get_account(address).unwrap();
    StateWithExtensions::<TokenAccountState>::unpack(&account.data)
        .unwrap()
        .base
}

#[test]
fn compliance_mint_contains_the_four_original_extensions() {
    let (mut svm, payer) = setup();
    let mint = create_compliance_mint(&mut svm, &payer);

    let account = svm.get_account(&mint.pubkey()).unwrap();
    assert_eq!(account.owner, TOKEN_2022_PROGRAM_ID);
    let state = StateWithExtensions::<MintState>::unpack(&account.data).unwrap();
    let extensions = state.get_extension_types().unwrap();

    assert!(extensions.contains(&ExtensionType::TransferFeeConfig));
    assert!(extensions.contains(&ExtensionType::MetadataPointer));
    assert!(extensions.contains(&ExtensionType::DefaultAccountState));
    assert!(extensions.contains(&ExtensionType::MintCloseAuthority));
    assert!(extensions.contains(&ExtensionType::PermanentDelegate));
    assert!(extensions.contains(&ExtensionType::ConfidentialTransferMint));
    assert!(extensions.contains(&ExtensionType::ConfidentialTransferFeeConfig));

    let fee = state.get_extension::<TransferFeeConfig>().unwrap();
    assert_eq!(u16::from(fee.newer_transfer_fee.transfer_fee_basis_points), FEE_BPS);
    assert_eq!(u64::from(fee.newer_transfer_fee.maximum_fee), MAX_FEE);

    let close = state.get_extension::<MintCloseAuthority>().unwrap();
    assert_eq!(Option::<Pubkey>::from(close.close_authority), Some(payer.pubkey()));

    let pointer = state.get_extension::<MetadataPointer>().unwrap();
    assert_eq!(Option::<Pubkey>::from(pointer.metadata_address), Some(mint.pubkey()));

    let default_state = state.get_extension::<DefaultAccountState>().unwrap();
    assert_eq!(default_state.state, AccountState::Frozen as u8, "Default account state should be Frozen");

    
}

#[test]
fn only_the_freeze_authority_can_thaw_an_individual_kyc_account() {
    let (mut svm, payer) = setup();
    let mint = create_compliance_mint(&mut svm, &payer);
    let owner = Keypair::new();
    let token_account = create_token_account(&mut svm, &payer, &mint.pubkey(), &owner.pubkey());

    assert_eq!(read_token_account(&svm, &token_account.pubkey()).state, AccountState::Frozen);

    // A different signer must not be able to thaw the account.
    let impostor = Keypair::new();
    let logs = send_expect_failure(
        &mut svm,
        &payer,
        &[thaw_ix(&token_account.pubkey(), &mint.pubkey(), &impostor.pubkey())],
        &[&impostor],
    );
    assert!(
        logs.contains("OwnerMismatch") || logs.contains("owner does not match"),
        "unexpected failure reason:\n{logs}"
    );
    assert_eq!(read_token_account(&svm, &token_account.pubkey()).state, AccountState::Frozen);

    // The mint's freeze authority (the payer) can thaw this one account.
    send(
        &mut svm,
        &payer,
        &[thaw_ix(&token_account.pubkey(), &mint.pubkey(), &payer.pubkey())],
        &[],
    );
    assert_eq!(
        read_token_account(&svm, &token_account.pubkey()).state,
        AccountState::Initialized
    );
}

#[test]
fn fee_aware_transfer_uses_the_configured_current_epoch_fee() {
    let (mut svm, payer) = setup();
    let mint = create_compliance_mint(&mut svm, &payer);
    let source_owner = Keypair::new();
    let destination_owner = Keypair::new();
    let source = create_token_account(&mut svm, &payer, &mint.pubkey(), &source_owner.pubkey());
    let destination = create_token_account(
        &mut svm,
        &payer,
        &mint.pubkey(),
        &destination_owner.pubkey(),
    );

    // The issuer is the mint authority and freeze authority. Thaw both accounts
    // before minting/transferring because the mint defaults all accounts frozen.
    send(
        &mut svm,
        &payer,
        &[
            thaw_ix(&source.pubkey(), &mint.pubkey(), &payer.pubkey()),
            thaw_ix(&destination.pubkey(), &mint.pubkey(), &payer.pubkey()),
            mint_to(
                &TOKEN_2022_PROGRAM_ID,
                &mint.pubkey(),
                &source.pubkey(),
                &payer.pubkey(),
                &[],
                10_000,
            )
            .unwrap(),
        ],
        &[],
    );

    let transfer_ix = Instruction {
        program_id: ID,
        accounts: accounts::TransferWithCurrentEpochFee {
            source: source.pubkey(),
            mint: mint.pubkey(),
            destination: destination.pubkey(),
            authority: source_owner.pubkey(),
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
        data: instruction::TransferWithCurrentEpochFee { amount: 1_000 }.data(),
    };
    send(&mut svm, &payer, &[transfer_ix], &[&source_owner]);

    let source_state = read_token_account(&svm, &source.pubkey());
    let destination_state = read_token_account(&svm, &destination.pubkey());
    assert_eq!(source_state.amount, 9_000);
    assert_eq!(destination_state.amount, 975); // 2.5% fee on 1,000 = 25.

    let destination_data = svm.get_account(&destination.pubkey()).unwrap();
    let destination_ext =
        StateWithExtensions::<TokenAccountState>::unpack(&destination_data.data).unwrap();
    let withheld = destination_ext.get_extension::<TransferFeeAmount>().unwrap();
    assert_eq!(u64::from(withheld.withheld_amount), 25);
}

#[test]
fn compliance_mint_includes_permanent_delegate_and_confidential_extensions() {
    let (mut svm, payer) = setup();
    let mint = create_compliance_mint(&mut svm, &payer);
    let account = svm.get_account(&mint.pubkey()).unwrap();
    let state = StateWithExtensions::<MintState>::unpack(&account.data).unwrap();
    let extensions = state.get_extension_types().unwrap();

    // This is an intentional gap test for the current source snapshot. It
    // documents that requirement 5's combined re-issued mint is not yet built.
    assert!(extensions.contains(&ExtensionType::PermanentDelegate));
    assert!(extensions.contains(&ExtensionType::ConfidentialTransferMint));
    assert!(extensions.contains(&ExtensionType::ConfidentialTransferFeeConfig));
}
