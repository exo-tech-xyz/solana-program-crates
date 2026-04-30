use solana_sdk::pubkey::Pubkey;

pub const KAMINO_LEND_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
pub const KAMINO_FARMS_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr");

pub fn derive_vanilla_obligation_address(
    obligation_id: u8,
    authority: &Pubkey,
    market: &Pubkey,
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            &0_u8.to_le_bytes(),
            &obligation_id.to_le_bytes(),
            authority.as_ref(),
            market.as_ref(),
            Pubkey::default().as_ref(),
            Pubkey::default().as_ref(),
        ],
        &KAMINO_LEND_PROGRAM_ID,
    )
    .0
}

pub fn derive_reserve_liquidity_supply(market: &Pubkey, reserve_liquidity_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"reserve_liq_supply",
            market.as_ref(),
            reserve_liquidity_mint.as_ref(),
        ],
        &KAMINO_LEND_PROGRAM_ID,
    )
    .0
}

pub fn derive_reserve_collateral_mint(market: &Pubkey, reserve_liquidity_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"reserve_coll_mint",
            market.as_ref(),
            reserve_liquidity_mint.as_ref(),
        ],
        &KAMINO_LEND_PROGRAM_ID,
    )
    .0
}

pub fn derive_reserve_collateral_supply(
    market: &Pubkey,
    reserve_liquidity_mint: &Pubkey,
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"reserve_coll_supply",
            market.as_ref(),
            reserve_liquidity_mint.as_ref(),
        ],
        &KAMINO_LEND_PROGRAM_ID,
    )
    .0
}

pub fn derive_market_authority_address(market: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lma", market.as_ref()], &KAMINO_LEND_PROGRAM_ID)
}

pub fn derive_obligation_farm_address(reserve_farm: &Pubkey, obligation: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"user", reserve_farm.as_ref(), obligation.as_ref()],
        &KAMINO_FARMS_PROGRAM_ID,
    )
    .0
}

pub fn derive_user_metadata_address(user: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"user_meta", user.as_ref()], &KAMINO_LEND_PROGRAM_ID)
}

pub fn derive_rewards_vault(farm_state: &Pubkey, rewards_vault_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"rvault", farm_state.as_ref(), rewards_vault_mint.as_ref()],
        &KAMINO_FARMS_PROGRAM_ID,
    )
    .0
}

pub fn derive_rewards_treasury_vault(
    global_config: &Pubkey,
    rewards_vault_mint: &Pubkey,
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"tvault",
            global_config.as_ref(),
            rewards_vault_mint.as_ref(),
        ],
        &KAMINO_FARMS_PROGRAM_ID,
    )
    .0
}

pub fn derive_farm_vaults_authority(farm_state: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"authority", farm_state.as_ref()],
        &KAMINO_FARMS_PROGRAM_ID,
    )
}

pub fn derive_kfarms_treasury_vault_authority(global_config: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"authority", global_config.as_ref()],
        &KAMINO_FARMS_PROGRAM_ID,
    )
}
