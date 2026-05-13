#![allow(dead_code)]

use std::error::Error;

use litesvm::LiteSVM;
use solana_sdk::{
    account::Account, clock::Clock, pubkey::Pubkey, signature::Keypair, signer::Signer,
    transaction::Transaction,
};
use exo_spl_token_litesvm::{setup_token_account, setup_token_mint};

use crate::{
    instructions::{
        create_refresh_kamino_obligation_instruction, create_refresh_kamino_reserve_instruction,
    },
    math_utils::Fraction,
    pda::{
        derive_farm_vaults_authority, derive_kfarms_treasury_vault_authority,
        derive_market_authority_address, derive_reserve_collateral_mint,
        derive_reserve_collateral_supply, derive_reserve_liquidity_supply,
        derive_rewards_treasury_vault, derive_rewards_vault, derive_user_metadata_address,
        KAMINO_FARMS_PROGRAM_ID, KAMINO_LEND_PROGRAM_ID,
    },
    state::{
        kfarms::{FarmState, GlobalConfig, RewardInfo, UserState},
        klend::{KaminoReserve, LastUpdate, LendingMarket, Obligation, UserMetadata},
    },
};

pub struct KaminoFarmsContext {
    pub global_config: Pubkey,
}

pub struct KaminoReserveContext {
    pub kamino_reserve_pk: Pubkey,
    pub liquidity_supply_vault: Pubkey,
    pub reserve_collateral_mint: Pubkey,
    pub reserve_collateral_supply: Pubkey,
    pub reserve_farm_collateral: Pubkey,
    pub reserve_farm_debt: Pubkey,
}

pub struct KaminoTestContext {
    pub lending_market: Pubkey,
    pub reserve_context: KaminoReserveContext,
    pub farms_context: KaminoFarmsContext,
    /// `(user_pubkey, user_metadata_pda)`
    pub referrer_metadata: (Pubkey, Pubkey),
}

/// Sets up all accounts required by the Kamino integration for testing.
///
/// `liquidity_collateral_ratio_bps`: exchange rate between liquidity and collateral in basis
/// points. 10_000 = 1:1 (no pre-existing supply), 1_000 = collateral worth 10x liquidity.
pub fn setup_kamino_state(
    svm: &mut LiteSVM,
    liquidity_mint: &Pubkey,
    liquidity_mint_token_program: &Pubkey,
    reward_mint: &Pubkey,
    reward_mint_token_program: &Pubkey,
    liquidity_collateral_ratio_bps: u64,
    reserve_has_farms: bool,
) -> KaminoTestContext {
    let lending_market_pk = Pubkey::new_unique();
    let mut market = LendingMarket::default();
    let (_lending_market_authority, market_auth_bump) =
        derive_market_authority_address(&lending_market_pk);
    market.bump_seed = market_auth_bump as u64;
    market.price_refresh_trigger_to_max_age_pct = 1;
    svm.set_account(
        lending_market_pk,
        Account {
            lamports: u64::MAX,
            data: [
                LendingMarket::DISCRIMINATOR.to_vec(),
                bytemuck::bytes_of(&market).to_vec(),
            ]
            .concat(),
            owner: KAMINO_LEND_PROGRAM_ID,
            executable: false,
            rent_epoch: u64::MAX,
        },
    )
    .unwrap();

    let global_config_pk = Pubkey::new_unique();
    let mut global_config = GlobalConfig::default();
    let treasury_vault = derive_rewards_treasury_vault(&global_config_pk, reward_mint);
    let (treasury_vault_authority, treasury_vault_authority_bump) =
        derive_kfarms_treasury_vault_authority(&global_config_pk);
    setup_token_account(
        svm,
        &treasury_vault,
        reward_mint,
        &treasury_vault_authority,
        0,
        reward_mint_token_program,
        None,
    );
    global_config.treasury_vaults_authority = treasury_vault_authority;
    global_config.treasury_vaults_authority_bump = treasury_vault_authority_bump as u64;
    svm.set_account(
        global_config_pk,
        Account {
            lamports: u64::MAX,
            data: [
                GlobalConfig::DISCRIMINATOR.to_vec(),
                bytemuck::bytes_of(&global_config).to_vec(),
            ]
            .concat(),
            owner: KAMINO_FARMS_PROGRAM_ID,
            executable: false,
            rent_epoch: u64::MAX,
        },
    )
    .unwrap();

    let reserve_context = setup_reserve(
        svm,
        &global_config_pk,
        liquidity_mint,
        liquidity_mint_token_program,
        reward_mint,
        reward_mint_token_program,
        &lending_market_pk,
        liquidity_collateral_ratio_bps,
        reserve_has_farms,
    );

    let farms_context = KaminoFarmsContext {
        global_config: global_config_pk,
    };

    let referrer_metadata = setup_user_metadata(svm);

    KaminoTestContext {
        lending_market: lending_market_pk,
        reserve_context,
        farms_context,
        referrer_metadata,
    }
}

fn setup_reserve(
    svm: &mut LiteSVM,
    global_config_pk: &Pubkey,
    liquidity_mint: &Pubkey,
    liquidity_mint_token_program: &Pubkey,
    reward_mint: &Pubkey,
    reward_mint_token_program: &Pubkey,
    lending_market_pk: &Pubkey,
    liquidity_collateral_ratio_bps: u64,
    has_farms: bool,
) -> KaminoReserveContext {
    let (lending_market_authority, _market_auth_bump) =
        derive_market_authority_address(lending_market_pk);

    let (reserve_farm_collateral, reserve_farm_debt) = if has_farms {
        let reserve_farm_collateral = Pubkey::new_unique();
        let mut farm_collateral = FarmState::default();
        farm_collateral.global_config = *global_config_pk;
        farm_collateral.delegate_authority = lending_market_authority;
        farm_collateral.scope_oracle_price_id = u64::MAX;
        farm_collateral.num_reward_tokens = 1;

        let reward_vault = derive_rewards_vault(&reserve_farm_collateral, reward_mint);
        let (farm_vault_authority, farm_vault_authority_bump) =
            derive_farm_vaults_authority(&reserve_farm_collateral);
        farm_collateral.farm_vaults_authority = farm_vault_authority;
        farm_collateral.farm_vaults_authority_bump = farm_vault_authority_bump as u64;

        setup_token_account(
            svm,
            &reward_vault,
            reward_mint,
            &farm_vault_authority,
            u64::MAX,
            reward_mint_token_program,
            None,
        );

        let mut reward_info = RewardInfo::default();
        reward_info.token.decimals = 6;
        reward_info.token.mint = *reward_mint;
        reward_info.token.token_program = *reward_mint_token_program;
        reward_info.rewards_available = u64::MAX;
        reward_info.rewards_vault = reward_vault;
        reward_info.rewards_issued_unclaimed = u64::MAX;
        farm_collateral.reward_infos[0] = reward_info;

        svm.set_account(
            reserve_farm_collateral,
            Account {
                lamports: u64::MAX,
                data: [
                    FarmState::DISCRIMINATOR.to_vec(),
                    bytemuck::bytes_of(&farm_collateral).to_vec(),
                ]
                .concat(),
                owner: KAMINO_FARMS_PROGRAM_ID,
                executable: false,
                rent_epoch: u64::MAX,
            },
        )
        .unwrap();

        let reserve_farm_debt = Pubkey::new_unique();
        let mut farm_debt = FarmState::default();
        farm_debt.global_config = *global_config_pk;
        farm_debt.delegate_authority = lending_market_authority;
        farm_debt.scope_oracle_price_id = u64::MAX;
        svm.set_account(
            reserve_farm_debt,
            Account {
                lamports: u64::MAX,
                data: [
                    FarmState::DISCRIMINATOR.to_vec(),
                    bytemuck::bytes_of(&farm_debt).to_vec(),
                ]
                .concat(),
                owner: KAMINO_FARMS_PROGRAM_ID,
                executable: false,
                rent_epoch: u64::MAX,
            },
        )
        .unwrap();

        (reserve_farm_collateral, reserve_farm_debt)
    } else {
        (Pubkey::default(), Pubkey::default())
    };

    let kamino_reserve_pk = Pubkey::new_unique();
    let mut kamino_reserve = KaminoReserve::default();
    kamino_reserve.lending_market = *lending_market_pk;
    kamino_reserve.liquidity.mint_pubkey = *liquidity_mint;
    kamino_reserve.liquidity.mint_decimals = 6;

    // Set last_update slot to current - 1 so the reserve is fresh on the first tx.
    let clock = svm.get_sysvar::<Clock>();
    let safe_slot = clock.slot.saturating_sub(1);
    let mut last_update_bytes = Vec::new();
    last_update_bytes.extend_from_slice(&safe_slot.to_le_bytes());
    last_update_bytes.extend_from_slice(&0u8.to_le_bytes()); // stale = false
    last_update_bytes.extend_from_slice(&0u8.to_le_bytes()); // price_status = 0
    last_update_bytes.extend_from_slice(&[0u8; 6]); // placeholder padding
    let last_update: LastUpdate =
        *bytemuck::try_from_bytes(&last_update_bytes).expect("LastUpdate from bytes");
    kamino_reserve.last_update = last_update;

    kamino_reserve.liquidity.market_price_sf = Fraction::ONE.to_bits();
    kamino_reserve.liquidity.token_program = *liquidity_mint_token_program;
    kamino_reserve.farm_collateral = reserve_farm_collateral;
    kamino_reserve.farm_debt = reserve_farm_debt;
    kamino_reserve.version = 1;
    kamino_reserve.config.token_info.max_age_price_seconds = u64::MAX;
    kamino_reserve.config.deposit_limit = u64::MAX;

    let mut liquidity_supply = 0;
    if liquidity_collateral_ratio_bps < 10_000 {
        kamino_reserve.collateral.mint_total_supply = 1_000_000;
        kamino_reserve.liquidity.available_amount =
            (kamino_reserve.collateral.mint_total_supply as u128)
                .saturating_mul(10_000)
                .saturating_div(liquidity_collateral_ratio_bps as u128) as u64;
        liquidity_supply = kamino_reserve.liquidity.available_amount;
    }

    let liquidity_supply_vault = derive_reserve_liquidity_supply(lending_market_pk, liquidity_mint);
    setup_token_account(
        svm,
        &liquidity_supply_vault,
        liquidity_mint,
        &lending_market_authority,
        liquidity_supply,
        liquidity_mint_token_program,
        None,
    );

    let reserve_collateral_mint = derive_reserve_collateral_mint(lending_market_pk, liquidity_mint);
    // Collateral mints are always classic SPL token, not token-2022.
    setup_token_mint(
        svm,
        &reserve_collateral_mint,
        6,
        &lending_market_authority,
        &spl_token::ID,
    );

    let reserve_collateral_supply =
        derive_reserve_collateral_supply(lending_market_pk, liquidity_mint);
    setup_token_account(
        svm,
        &reserve_collateral_supply,
        &reserve_collateral_mint,
        &lending_market_authority,
        0,
        &spl_token::ID,
        None,
    );

    kamino_reserve.liquidity.supply_vault = liquidity_supply_vault;
    kamino_reserve.collateral.mint_pubkey = reserve_collateral_mint;
    kamino_reserve.collateral.supply_vault = reserve_collateral_supply;
    svm.set_account(
        kamino_reserve_pk,
        Account {
            lamports: u64::MAX,
            data: [
                KaminoReserve::DISCRIMINATOR.to_vec(),
                bytemuck::bytes_of(&kamino_reserve).to_vec(),
            ]
            .concat(),
            owner: KAMINO_LEND_PROGRAM_ID,
            executable: false,
            rent_epoch: u64::MAX,
        },
    )
    .unwrap();

    KaminoReserveContext {
        kamino_reserve_pk,
        liquidity_supply_vault,
        reserve_collateral_mint,
        reserve_collateral_supply,
        reserve_farm_collateral,
        reserve_farm_debt,
    }
}

fn setup_user_metadata(svm: &mut LiteSVM) -> (Pubkey, Pubkey) {
    let user_pk = Pubkey::new_unique();
    let (metadata_pda, metadata_bump) = derive_user_metadata_address(&user_pk);

    let mut metadata = UserMetadata::default();
    metadata.bump = metadata_bump as u64;
    metadata.owner = user_pk;
    svm.set_account(
        metadata_pda,
        Account {
            lamports: u64::MAX,
            data: [
                UserMetadata::DISCRIMINATOR.to_vec(),
                bytemuck::bytes_of(&metadata).to_vec(),
            ]
            .concat(),
            owner: KAMINO_LEND_PROGRAM_ID,
            executable: false,
            rent_epoch: u64::MAX,
        },
    )
    .unwrap();

    (user_pk, metadata_pda)
}

/// Sets up additional reserves in the same lending market for the given liquidity mints.
pub fn setup_additional_reserves(
    svm: &mut LiteSVM,
    global_config_pk: &Pubkey,
    lending_market_pk: &Pubkey,
    reward_mint_and_program: (&Pubkey, &Pubkey),
    liquidity_mints_and_programs: Vec<(&Pubkey, &Pubkey)>,
) -> Vec<KaminoReserveContext> {
    liquidity_mints_and_programs
        .into_iter()
        .map(|(liquidity_mint, liquidity_mint_program)| {
            setup_reserve(
                svm,
                global_config_pk,
                liquidity_mint,
                liquidity_mint_program,
                reward_mint_and_program.0,
                reward_mint_and_program.1,
                lending_market_pk,
                10_000,
                true,
            )
        })
        .collect()
}

/// Refreshes a Kamino reserve via an on-chain transaction.
pub fn refresh_kamino_reserve(
    svm: &mut LiteSVM,
    payer: &Keypair,
    reserve: &Pubkey,
    market: &Pubkey,
    scope_prices: &Pubkey,
) -> Result<(), Box<dyn Error>> {
    let ix = create_refresh_kamino_reserve_instruction(reserve, market, scope_prices);
    let result = svm.send_transaction(Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    ));
    if let Err(ref e) = result {
        println!("{:#?}", e.meta.logs);
    }
    assert!(result.is_ok(), "refresh_kamino_reserve failed");
    Ok(())
}

/// Refreshes a Kamino obligation via an on-chain transaction.
pub fn refresh_kamino_obligation(
    svm: &mut LiteSVM,
    payer: &Keypair,
    market: &Pubkey,
    obligation: &Pubkey,
    reserves: Vec<&Pubkey>,
) -> Result<(), Box<dyn Error>> {
    let ix = create_refresh_kamino_obligation_instruction(market, obligation, reserves);
    let result = svm.send_transaction(Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    ));
    if let Err(ref e) = result {
        println!("{:#?}", e.meta.logs);
    }
    assert!(result.is_ok(), "refresh_kamino_obligation failed");
    Ok(())
}

/// Fetches and deserializes a `KaminoReserve` from the SVM.
pub fn fetch_kamino_reserve(
    svm: &LiteSVM,
    kamino_reserve_pk: &Pubkey,
) -> Result<KaminoReserve, Box<dyn Error>> {
    let acc = svm
        .get_account(kamino_reserve_pk)
        .expect("kamino reserve account not found");
    Ok(KaminoReserve::try_from(&acc.data)?.clone())
}

/// Fetches and deserializes a Kamino `Obligation` from the SVM.
pub fn fetch_kamino_obligation(
    svm: &LiteSVM,
    kamino_obligation_pk: &Pubkey,
) -> Result<Obligation, Box<dyn Error>> {
    let acc = svm
        .get_account(kamino_obligation_pk)
        .expect("kamino obligation account not found");
    Ok(Obligation::try_from(&acc.data)?.clone())
}

/// Accrues interest on a `KaminoReserve` by directly mutating its `available_amount`.
///
/// `interest_bps`: basis points of interest to apply (e.g. 100 = 1%).
pub fn kamino_reserve_accrue_interest(
    svm: &mut LiteSVM,
    kamino_reserve_pk: &Pubkey,
    interest_bps: u64,
) -> Result<(), Box<dyn Error>> {
    let mut acc = svm
        .get_account(kamino_reserve_pk)
        .expect("kamino reserve account not found");
    let reserve = bytemuck::try_from_bytes_mut::<KaminoReserve>(&mut acc.data[8..]).unwrap();
    reserve.liquidity.available_amount = reserve
        .liquidity
        .available_amount
        .saturating_mul(10_000 + interest_bps)
        .saturating_div(10_000);
    svm.set_account(*kamino_reserve_pk, acc)
        .expect("failed to set kamino reserve");
    Ok(())
}

/// Directly sets `rewards_issued_unclaimed` on a farm `UserState` for the given reward token.
pub fn set_obligation_farm_rewards_issued_unclaimed(
    svm: &mut LiteSVM,
    obligation_farm: &Pubkey,
    reward_mint: &Pubkey,
    token_program: &Pubkey,
    amount: u64,
) -> Result<(), Box<dyn Error>> {
    let user_state_acc = svm
        .get_account(obligation_farm)
        .expect("obligation farm account not found");
    let mut user_state = UserState::try_from(&user_state_acc.data)?.clone();

    let reserve_farm_acc = svm
        .get_account(&user_state.farm_state)
        .expect("reserve farm account not found");
    let reserve_farm = FarmState::try_from(&reserve_farm_acc.data)?;

    let (reward_index, _) = reserve_farm
        .find_reward_index_and_rewards_available(reward_mint, token_program)
        .expect("reward token not found in farm");

    user_state.rewards_issued_unclaimed[reward_index as usize] = amount;

    svm.set_account(
        *obligation_farm,
        Account {
            data: [
                UserState::DISCRIMINATOR.to_vec(),
                bytemuck::bytes_of(&user_state).to_vec(),
            ]
            .concat(),
            ..user_state_acc
        },
    )?;

    Ok(())
}
