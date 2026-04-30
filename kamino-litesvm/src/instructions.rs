use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

use crate::{discriminator::anchor_discriminator, pda::KAMINO_LEND_PROGRAM_ID};

pub fn create_refresh_kamino_reserve_instruction(
    reserve: &Pubkey,
    market: &Pubkey,
    scope_prices: &Pubkey,
) -> Instruction {
    // pyth, switchboard price, and switchboard twap oracles are unused in tests;
    // pass the program ID as a no-op placeholder (matching the client impl).
    Instruction {
        program_id: KAMINO_LEND_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*reserve, false),
            AccountMeta::new_readonly(*market, false),
            AccountMeta::new_readonly(KAMINO_LEND_PROGRAM_ID, false), // pyth
            AccountMeta::new_readonly(KAMINO_LEND_PROGRAM_ID, false), // switchboard price
            AccountMeta::new_readonly(KAMINO_LEND_PROGRAM_ID, false), // switchboard twap
            AccountMeta::new_readonly(*scope_prices, false),
        ],
        data: anchor_discriminator("global", "refresh_reserve").to_vec(),
    }
}

pub fn create_refresh_kamino_obligation_instruction(
    market: &Pubkey,
    obligation: &Pubkey,
    reserves: Vec<&Pubkey>,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(*market, false),
        AccountMeta::new(*obligation, false),
    ];
    accounts.extend(reserves.iter().map(|r| AccountMeta::new(**r, false)));

    Instruction {
        program_id: KAMINO_LEND_PROGRAM_ID,
        accounts,
        data: anchor_discriminator("global", "refresh_obligation").to_vec(),
    }
}
