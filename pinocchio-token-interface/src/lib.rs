use core::ops::Deref;
use pinocchio::error::ProgramError;
use solana_account_view::{AccountView, Ref};

pub use pinocchio_token_2022::instructions;

use pinocchio_token_2022::state::Account as T22TokenAccount;

const EXTENSION_TYPE_LEN: usize = 2;
const EXTENSION_LENGTH_LEN: usize = 2;

/// SPL Token-2022 account-type byte after the 165-byte base state (`AccountType::Mint`).
const T22_ACCOUNT_TYPE_MINT: u8 = 1;
/// SPL Token-2022 account-type byte for a token holding account (`AccountType::Account`).
const T22_ACCOUNT_TYPE_TOKEN_ACCOUNT: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtensionType {
    TransferFeeConfig,
    MintCloseAuthority,
    ConfidentialTransferMint,
    PermanentDelegate,
    TransferHook,
    ConfidentialTransferFeeConfig,
    MetadataPointer,
    TokenMetadata,
    GroupPointer,
    TokenGroup,
}

impl ExtensionType {
    fn from_bytes(b: [u8; 2]) -> Option<Self> {
        match u16::from_le_bytes(b) {
            1 => Some(Self::TransferFeeConfig),
            3 => Some(Self::MintCloseAuthority),
            4 => Some(Self::ConfidentialTransferMint),
            12 => Some(Self::PermanentDelegate),
            14 => Some(Self::TransferHook),
            16 => Some(Self::ConfidentialTransferFeeConfig),
            18 => Some(Self::MetadataPointer),
            19 => Some(Self::TokenMetadata),
            20 => Some(Self::GroupPointer),
            21 => Some(Self::TokenGroup),
            _ => None,
        }
    }
}

pub struct TokenAccount<'info>(Ref<'info, T22TokenAccount>);

/// Size of multisig account for TokenAccount and Mint.
pub const MULTISIG_ACCOUNT_LENGTH: usize = 355;

/// Get the account type for a T22 account.
pub fn get_account_type(account_view: &AccountView) -> Result<u8, ProgramError> {
    let data = account_view.try_borrow()?;
    // Account type is at byte 165 (`T22TokenAccount::BASE_LEN`) for extensible T22 accounts.
    let account_type = data[T22TokenAccount::BASE_LEN];
    Ok(account_type)
}

impl<'info> TokenAccount<'info> {
    pub fn from_account_view(account_view: &'info AccountView) -> Result<Self, ProgramError> {
        if account_view.owned_by(&pinocchio_token_2022::ID) {
            let data_len = account_view.data_len();
            if data_len > T22TokenAccount::BASE_LEN {
                let account_type = get_account_type(account_view)?;
                // Token holding account must have account type 2.
                if account_type != T22_ACCOUNT_TYPE_TOKEN_ACCOUNT {
                    return Err(ProgramError::InvalidAccountData);
                }
                // Multisig accounts are not supported.
                if data_len == MULTISIG_ACCOUNT_LENGTH {
                    return Err(ProgramError::InvalidAccountData);
                }
            }
            T22TokenAccount::from_account_view(account_view)
                .map(TokenAccount)
                .map_err(|_| ProgramError::InvalidAccountData)
        } else if account_view.owned_by(&pinocchio_token::ID) {
            if account_view.data_len() != pinocchio_token::state::Account::LEN {
                return Err(ProgramError::InvalidAccountData);
            }
            // SAFETY: Legacy Token and Token-2022 token account structs share the same base layout.
            Ok(TokenAccount(Ref::map(
                account_view.try_borrow()?,
                |data| unsafe { T22TokenAccount::from_bytes_unchecked(data) },
            )))
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }
}

impl Deref for TokenAccount<'_> {
    type Target = T22TokenAccount;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Mint<'info>(Ref<'info, pinocchio_token_2022::state::Mint>);

impl<'info> Mint<'info> {
    pub fn from_account_view(account_view: &'info AccountView) -> Result<Self, ProgramError> {
        if account_view.owned_by(&pinocchio_token_2022::ID) {
            let data_len = account_view.data_len();
            // Use `T22TokenAccount::BASE_LEN` (165) as threshold — that is where the account-type
            // byte lives for all extensible T22 accounts, including Mints.
            if data_len > T22TokenAccount::BASE_LEN {
                let account_type = get_account_type(account_view)?;
                // Mint must have account type 1.
                if account_type != T22_ACCOUNT_TYPE_MINT {
                    return Err(ProgramError::InvalidAccountData);
                }
                // Multisig accounts are not supported.
                if data_len == MULTISIG_ACCOUNT_LENGTH {
                    return Err(ProgramError::InvalidAccountData);
                }
            }
            pinocchio_token_2022::state::Mint::from_account_view(account_view)
                .map(Mint)
                .map_err(|_| ProgramError::InvalidAccountData)
        } else if account_view.owned_by(&pinocchio_token::ID) {
            if account_view.data_len() != pinocchio_token::state::Mint::LEN {
                return Err(ProgramError::InvalidAccountData);
            }
            // SAFETY: Token and Token2022 Mint structs have compatible layouts.
            Ok(Mint(Ref::map(account_view.try_borrow()?, |data| unsafe {
                pinocchio_token_2022::state::Mint::from_bytes_unchecked(data)
            })))
        } else {
            Err(ProgramError::InvalidAccountData)
        }
    }
}

impl Deref for Mint<'_> {
    type Target = pinocchio_token_2022::state::Mint;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Iterate over TLV extension data and return all extension types present.
/// Works for both Token-2022 Mint and TokenAccount accounts.
pub fn get_all_extensions(acc_data_bytes: &[u8]) -> Result<Vec<ExtensionType>, ProgramError> {
    let ext_start = T22TokenAccount::BASE_LEN + 1;
    if acc_data_bytes.len() <= ext_start {
        return Ok(Vec::new());
    }
    let account_type_byte = acc_data_bytes[T22TokenAccount::BASE_LEN];
    if account_type_byte != T22_ACCOUNT_TYPE_MINT
        && account_type_byte != T22_ACCOUNT_TYPE_TOKEN_ACCOUNT
    {
        return Err(ProgramError::InvalidAccountData);
    }
    let ext_bytes = &acc_data_bytes[ext_start..];
    let mut extension_types = Vec::new();
    let mut start = 0;
    while start + EXTENSION_TYPE_LEN + EXTENSION_LENGTH_LEN <= ext_bytes.len() {
        let type_bytes: [u8; 2] = ext_bytes[start..start + EXTENSION_TYPE_LEN]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?;
        let ext_type =
            ExtensionType::from_bytes(type_bytes).ok_or(ProgramError::InvalidAccountData)?;
        let len_bytes: [u8; 2] = ext_bytes
            [start + EXTENSION_TYPE_LEN..start + EXTENSION_TYPE_LEN + EXTENSION_LENGTH_LEN]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?;
        let ext_len = u16::from_le_bytes(len_bytes) as usize;
        extension_types.push(ext_type);
        start += EXTENSION_TYPE_LEN + EXTENSION_LENGTH_LEN + ext_len;
    }
    Ok(extension_types)
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_account_view::{AccountView, RuntimeAccount, NOT_BORROWED};

    fn make_account(owner: [u8; 32], data: Vec<u8>) -> (Vec<u8>, AccountView) {
        let header = core::mem::size_of::<RuntimeAccount>();
        debug_assert_eq!(header, 88);
        let mut buf = vec![0u8; header + data.len()];
        buf[0] = NOT_BORROWED;
        buf[40..72].copy_from_slice(&owner);
        buf[80..88].copy_from_slice(&(data.len() as u64).to_le_bytes());
        if !data.is_empty() {
            buf[header..].copy_from_slice(&data);
        }
        let raw = buf.as_mut_ptr() as *mut RuntimeAccount;
        // SAFETY: buf is live, correctly laid out, and data_len matches buf length.
        let view = unsafe { AccountView::new_unchecked(raw) };
        (buf, view)
    }

    fn t22_id() -> [u8; 32] {
        // SAFETY: solana_address::Address is #[repr(transparent)] over [u8; 32].
        unsafe { core::mem::transmute(pinocchio_token_2022::ID) }
    }

    fn token_id() -> [u8; 32] {
        // SAFETY: solana_address::Address is #[repr(transparent)] over [u8; 32].
        unsafe { core::mem::transmute(pinocchio_token::ID) }
    }

    // ── TokenAccount ──────────────────────────────────────────────────────────

    #[test]
    fn token_account_t22_base_len() {
        let data = vec![0u8; T22TokenAccount::BASE_LEN];
        let (_buf, view) = make_account(t22_id(), data);
        assert!(TokenAccount::from_account_view(&view).is_ok());
    }

    #[test]
    fn token_account_t22_with_extensions() {
        let mut data = vec![0u8; T22TokenAccount::BASE_LEN + 2];
        data[T22TokenAccount::BASE_LEN] = T22_ACCOUNT_TYPE_TOKEN_ACCOUNT;
        let (_buf, view) = make_account(t22_id(), data);
        assert!(TokenAccount::from_account_view(&view).is_ok());
    }

    #[test]
    fn token_account_t22_wrong_type() {
        let mut data = vec![0u8; T22TokenAccount::BASE_LEN + 2];
        data[T22TokenAccount::BASE_LEN] = T22_ACCOUNT_TYPE_MINT; // wrong for token account
        let (_buf, view) = make_account(t22_id(), data);
        assert_eq!(
            TokenAccount::from_account_view(&view).err().unwrap(),
            ProgramError::InvalidAccountData,
        );
    }

    #[test]
    fn token_account_t22_multisig_length() {
        let mut data = vec![0u8; MULTISIG_ACCOUNT_LENGTH];
        data[T22TokenAccount::BASE_LEN] = T22_ACCOUNT_TYPE_TOKEN_ACCOUNT;
        let (_buf, view) = make_account(t22_id(), data);
        assert_eq!(
            TokenAccount::from_account_view(&view).err().unwrap(),
            ProgramError::InvalidAccountData,
        );
    }

    #[test]
    fn token_account_legacy_success() {
        let data = vec![0u8; pinocchio_token::state::Account::LEN];
        let (_buf, view) = make_account(token_id(), data);
        assert!(TokenAccount::from_account_view(&view).is_ok());
    }

    #[test]
    fn token_account_legacy_wrong_length() {
        let data = vec![0u8; 100];
        let (_buf, view) = make_account(token_id(), data);
        assert_eq!(
            TokenAccount::from_account_view(&view).err().unwrap(),
            ProgramError::InvalidAccountData,
        );
    }

    #[test]
    fn token_account_wrong_owner() {
        let data = vec![0u8; T22TokenAccount::BASE_LEN];
        let (_buf, view) = make_account([0u8; 32], data);
        assert_eq!(
            TokenAccount::from_account_view(&view).err().unwrap(),
            ProgramError::InvalidAccountData,
        );
    }

    // ── Mint ──────────────────────────────────────────────────────────────────

    #[test]
    fn mint_t22_base_len() {
        let data = vec![0u8; pinocchio_token_2022::state::Mint::BASE_LEN];
        let (_buf, view) = make_account(t22_id(), data);
        assert!(Mint::from_account_view(&view).is_ok());
    }

    #[test]
    fn mint_t22_with_extensions() {
        let mut data = vec![0u8; T22TokenAccount::BASE_LEN + 2];
        data[T22TokenAccount::BASE_LEN] = T22_ACCOUNT_TYPE_MINT;
        let (_buf, view) = make_account(t22_id(), data);
        assert!(Mint::from_account_view(&view).is_ok());
    }

    #[test]
    fn mint_t22_wrong_type() {
        let mut data = vec![0u8; T22TokenAccount::BASE_LEN + 2];
        data[T22TokenAccount::BASE_LEN] = T22_ACCOUNT_TYPE_TOKEN_ACCOUNT; // wrong for mint
        let (_buf, view) = make_account(t22_id(), data);
        assert_eq!(
            Mint::from_account_view(&view).err().unwrap(),
            ProgramError::InvalidAccountData,
        );
    }

    #[test]
    fn mint_legacy_success() {
        let data = vec![0u8; pinocchio_token::state::Mint::LEN];
        let (_buf, view) = make_account(token_id(), data);
        assert!(Mint::from_account_view(&view).is_ok());
    }

    #[test]
    fn mint_legacy_wrong_length() {
        let data = vec![0u8; 100];
        let (_buf, view) = make_account(token_id(), data);
        assert_eq!(
            Mint::from_account_view(&view).err().unwrap(),
            ProgramError::InvalidAccountData,
        );
    }

    #[test]
    fn mint_wrong_owner() {
        let data = vec![0u8; pinocchio_token_2022::state::Mint::BASE_LEN];
        let (_buf, view) = make_account([0u8; 32], data);
        assert_eq!(
            Mint::from_account_view(&view).err().unwrap(),
            ProgramError::InvalidAccountData,
        );
    }

    // ── Extensions ────────────────────────────────────────────────────────────

    pub const TEST_MINT_WITH_EXTENSIONS_SLICE: &[u8] = &[
        1, 0, 0, 0, 221, 76, 72, 108, 144, 248, 182, 240, 7, 195, 4, 239, 36, 129, 248, 5, 24, 107,
        232, 253, 95, 82, 172, 209, 2, 92, 183, 155, 159, 103, 255, 33, 133, 204, 6, 44, 35, 140,
        0, 0, 6, 1, 1, 0, 0, 0, 23, 133, 50, 97, 239, 106, 184, 83, 42, 103, 240, 83, 134, 90, 173,
        49, 41, 63, 207, 7, 207, 18, 10, 181, 185, 161, 87, 6, 84, 141, 192, 43, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        /*                  MintCloseAuthority Extension                                      */
        3, 0, 32, 0, 23, 133, 50, 97, 239, 106, 184, 83, 42, 103, 240, 83, 134, 90, 173, 49, 41, 63,
        207, 7, 207, 18, 10, 181, 185, 161, 87, 6, 84, 141, 192, 43,
        /*                  PermanentDelegate Extension                                      */
        12, 0, 32, 0, 23, 133, 50, 97, 239, 106, 184, 83, 42, 103, 240, 83, 134, 90, 173, 49, 41,
        63, 207, 7, 207, 18, 10, 181, 185, 161, 87, 6, 84, 141, 192, 43,
        /*                  TransferFeeConfig Extension                                      */
        1, 0, 108, 0, 23, 133, 50, 97, 239, 106, 184, 83, 42, 103, 240, 83, 134, 90, 173, 49, 41,
        63, 207, 7, 207, 18, 10, 181, 185, 161, 87, 6, 84, 141, 192, 43, 23, 133, 50, 97, 239, 106,
        184, 83, 42, 103, 240, 83, 134, 90, 173, 49, 41, 63, 207, 7, 207, 18, 10, 181, 185, 161,
        87, 6, 84, 141, 192, 43, 0, 0, 0, 0, 0, 0, 0, 0, 93, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 93, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        /*                  ConfidentialTransferMint Extension                                      */
        4, 0, 65, 0, 23, 133, 50, 97, 239, 106, 184, 83, 42, 103, 240, 83, 134, 90, 173, 49, 41, 63,
        207, 7, 207, 18, 10, 181, 185, 161, 87, 6, 84, 141, 192, 43, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        /*                  ConfidentialTransferFeeConfig Extension                                      */
        16, 0, 129, 0, 23, 133, 50, 97, 239, 106, 184, 83, 42, 103, 240, 83, 134, 90, 173, 49, 41,
        63, 207, 7, 207, 18, 10, 181, 185, 161, 87, 6, 84, 141, 192, 43, 28, 55, 230, 67, 59, 115,
        4, 221, 130, 115, 122, 228, 13, 155, 139, 243, 196, 159, 91, 14, 108, 73, 168, 213, 51, 40,
        179, 229, 6, 144, 28, 87, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        /*                  TransferHook Extension                                      */
        14, 0, 64, 0, 23, 133, 50, 97, 239, 106, 184, 83, 42, 103, 240, 83, 134, 90, 173, 49, 41,
        63, 207, 7, 207, 18, 10, 181, 185, 161, 87, 6, 84, 141, 192, 43, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        /*                  MetadataPointer Extension                                      */
        18, 0, 64, 0, 23, 133, 50, 97, 239, 106, 184, 83, 42, 103, 240, 83, 134, 90, 173, 49, 41,
        63, 207, 7, 207, 18, 10, 181, 185, 161, 87, 6, 84, 141, 192, 43, 23, 146, 72, 59, 108, 138,
        42, 135, 183, 71, 29, 129, 79, 149, 145, 249, 57, 92, 132, 10, 156, 227, 217, 244, 213,
        186, 125, 58, 75, 138, 116, 158,
        /*                  TokenMetadata Extension                                      */
        19, 0, 174, 0, 23, 133, 50, 97, 239, 106, 184, 83, 42, 103, 240, 83, 134, 90, 173, 49, 41,
        63, 207, 7, 207, 18, 10, 181, 185, 161, 87, 6, 84, 141, 192, 43, 23, 146, 72, 59, 108, 138,
        42, 135, 183, 71, 29, 129, 79, 149, 145, 249, 57, 92, 132, 10, 156, 227, 217, 244, 213,
        186, 125, 58, 75, 138, 116, 158, 10, 0, 0, 0, 80, 97, 121, 80, 97, 108, 32, 85, 83, 68, 5,
        0, 0, 0, 80, 89, 85, 83, 68, 79, 0, 0, 0, 104, 116, 116, 112, 115, 58, 47, 47, 116, 111,
        107, 101, 110, 45, 109, 101, 116, 97, 100, 97, 116, 97, 46, 112, 97, 120, 111, 115, 46, 99,
        111, 109, 47, 112, 121, 117, 115, 100, 95, 109, 101, 116, 97, 100, 97, 116, 97, 47, 112,
        114, 111, 100, 47, 115, 111, 108, 97, 110, 97, 47, 112, 121, 117, 115, 100, 95, 109, 101,
        116, 97, 100, 97, 116, 97, 46, 106, 115, 111, 110, 0, 0, 0, 0,
        /*                  GroupPointer Extension                                      */
        20, 0, 64, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
        2, 2, 2, 2, 2, 2, 2, 2,
        /*                  TokenGroup Extension                                      */
        21, 0, 80, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
        2, 2, 2, 2, 2, 2, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0,
    ];

    #[test]
    fn test_get_all_extensions_for_mint() {
        let extension_types = get_all_extensions(TEST_MINT_WITH_EXTENSIONS_SLICE).unwrap();
        assert_eq!(
            extension_types,
            vec![
                ExtensionType::MintCloseAuthority,
                ExtensionType::PermanentDelegate,
                ExtensionType::TransferFeeConfig,
                ExtensionType::ConfidentialTransferMint,
                ExtensionType::ConfidentialTransferFeeConfig,
                ExtensionType::TransferHook,
                ExtensionType::MetadataPointer,
                ExtensionType::TokenMetadata,
                ExtensionType::GroupPointer,
                ExtensionType::TokenGroup,
            ]
        );
    }

    #[test]
    fn test_get_all_extensions_no_extensions() {
        let data = vec![0u8; T22TokenAccount::BASE_LEN + 1];
        assert_eq!(get_all_extensions(&data).unwrap(), vec![]);
    }

    #[test]
    fn test_get_all_extensions_wrong_account_type() {
        let base = T22TokenAccount::BASE_LEN;
        let mut data = vec![0u8; base + 2];
        data[base] = 0; // Uninitialized AccountType
        assert_eq!(
            get_all_extensions(&data).err().unwrap(),
            ProgramError::InvalidAccountData,
        );
    }
}
