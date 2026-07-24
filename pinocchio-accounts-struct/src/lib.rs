#[doc(hidden)]
#[macro_export]
macro_rules! check_account_attr {
    ($field:ident, $program_id:expr, mut) => {
        if !$field.is_writable() {
            pinocchio_log::log!("{}: invalid mut", stringify!($field));
            return Err(pinocchio::error::ProgramError::Immutable);
        }
    };
    ($field:ident, $program_id:expr, signer) => {
        if !$field.is_signer() {
            pinocchio_log::log!("{}: invalid signer", stringify!($field));
            return Err(pinocchio::error::ProgramError::MissingRequiredSignature);
        }
    };
    ($field:ident, $program_id:expr, empty) => {
        if !$field.is_data_empty() {
            pinocchio_log::log!("{}: not empty", stringify!($field));
            return Err(pinocchio::error::ProgramError::AccountAlreadyInitialized);
        }
    };
    ($field:ident, $program_id:expr, opt_signer) => {
        if $field.address() != &pinocchio::Address::new_from_array($program_id)
            && !$field.is_signer()
        {
            pinocchio_log::log!("{}: invalid signer", stringify!($field));
            return Err(pinocchio::error::ProgramError::MissingRequiredSignature);
        }
    };
    ($field:ident, $program_id:expr, $unknown:ident) => {
        ::core::compile_error!(::core::concat!(
            "unknown account attr: ",
            ::core::stringify!($unknown)
        ));
    };
}

/// Resolves a field's reference type from its attribute list.
///
/// If any attr is `mut`, expands to `& $lt mut AccountView`; otherwise
/// `& $lt AccountView`.
#[doc(hidden)]
#[macro_export]
macro_rules! account_field_ty {
    (@ty $lt:lifetime ; mut $($rest:tt)*) => {
        & $lt mut AccountView
    };
    (@ty $lt:lifetime ; $head:ident, $($tail:ident),+) => {
        $crate::account_field_ty!(@ty $lt ; $($tail),+)
    };
    (@ty $lt:lifetime ; $head:ident) => {
        & $lt AccountView
    };
    (@ty $lt:lifetime ;) => {
        & $lt AccountView
    };
}

/// Defines an account context struct and its `from_accounts` validator.
///
/// ### Example
/// ```ignore
/// define_account_struct! {
///     pub struct AtomicSwapRepay<'info> {
///         payer: signer;
///         controller;
///         authority: signer;
///         integration: mut;
///         token_program: @pubkey(pinocchio_token::ID, pinocchio_token2022::ID);
///         reserve: @owner(SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID);
///     }
///     program_id: crate::ID
/// }
/// ```
///
/// ### Supported syntax per field:
/// ```text
/// field: [attr, ...]? [@pubkey(KEY)]? [@owner(KEY1, ...)]?;
/// ```
/// - `signer` - account must be signer
/// - `mut` - account must be writable; field type is `&mut AccountView`
/// - `empty` - account data field must be empty
/// - `opt_signer` — account is optional, but must be a signer if provided
/// - `@pubkey(KEY1, KEY2...)` — account pubkey must match one of the keys provided
/// - `@owner(KEY1, KEY2...)` — account owner must match one of the keys provided
///
/// Use `@remaining_accounts as remaining_accounts;` to capture extra accounts as `&'info mut [AccountView]`.
///
/// The generated `from_accounts` consumes accounts in order and applies all checks.
#[macro_export]
macro_rules! define_account_struct {
    (
        $vis:vis struct $name:ident < $lt:lifetime > {
            $(
                $field:ident
                $( : $( $attr:ident ),* $(,)? )?
                $( @pubkey( $( $check_pubkey:expr ),+ ) )?
                $( @owner( $( $check_owner:expr ),+ ) )?
                ;
            )*
            $( @remaining_accounts as $rem_ident:ident ; )?
        }
        program_id: $program_id:expr
    ) => {
        $vis struct $name<$lt> {
            $(
                pub $field: $crate::account_field_ty!(@ty $lt ; $($($attr),*)?),
            )*
            $( pub $rem_ident: & $lt mut [AccountView], )?
        }

        impl<$lt> $name<$lt> {
            pub fn from_accounts(
                accounts: & $lt mut [AccountView],
            ) -> Result<Self, pinocchio::error::ProgramError> {
                #![allow(unused_assignments)]
                use pinocchio::error::ProgramError;

                let mut accounts = accounts;
                $(
                    let ($field, rest) = accounts
                        .split_first_mut()
                        .ok_or(ProgramError::NotEnoughAccountKeys)?;
                    accounts = rest;

                    $(
                        $(
                            $crate::check_account_attr!($field, $program_id, $attr);
                        )*
                    )?

                    $(
                        if !( $( $field.address() == &$check_pubkey )||+ ) {
                            pinocchio_log::log!("{}: invalid key", stringify!($field));
                            return Err(ProgramError::IncorrectProgramId);
                        }
                    )?
                    $(
                    if !( $( $field.owned_by(&$check_owner) )||+ ) {
                            pinocchio_log::log!("{}: invalid owner", stringify!($field));
                            return Err(ProgramError::InvalidAccountOwner);
                        }
                    )?
                )*

                $( let $rem_ident = accounts; )?

                Ok(Self {
                    $(
                        $field,
                    )*
                    $( $rem_ident, )?
                })
            }
        }
    };
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use pinocchio::{
        account::{RuntimeAccount, NOT_BORROWED},
        error::ProgramError,
        AccountView, Address,
    };

    const PROG_ID: [u8; 32] = [1u8; 32];
    const KEY_A: [u8; 32] = [2u8; 32];
    const KEY_B: [u8; 32] = [3u8; 32];

    /// Distinguishes `&AccountView` from `&mut AccountView` without coercion
    /// hiding the difference.
    trait AccountRefKind {
        const MUTABLE: bool;
    }
    impl AccountRefKind for &AccountView {
        const MUTABLE: bool = false;
    }
    impl AccountRefKind for &mut AccountView {
        const MUTABLE: bool = true;
    }

    fn assert_immut_field<T: AccountRefKind>(_: T) {
        assert!(!T::MUTABLE);
    }
    fn assert_mut_field<T: AccountRefKind>(_: T) {
        assert!(T::MUTABLE);
    }

    /// Build a `RuntimeAccount` header + optional data in a heap buffer, then wrap
    /// it in an `AccountView`.  The caller **must** keep the returned `Vec<u8>`
    /// alive for as long as the `AccountView` is used.
    fn make_account(
        is_signer: bool,
        is_writable: bool,
        address: [u8; 32],
        owner: [u8; 32],
        data: Vec<u8>,
    ) -> (Vec<u8>, AccountView) {
        let header = core::mem::size_of::<RuntimeAccount>();
        let mut buf = vec![0u8; header + data.len()];
        let raw = buf.as_mut_ptr() as *mut RuntimeAccount;
        unsafe {
            (*raw).borrow_state = NOT_BORROWED;
            (*raw).is_signer = if is_signer { 1 } else { 0 };
            (*raw).is_writable = if is_writable { 1 } else { 0 };
            (*raw).executable = 0;
            (*raw).padding = [0; 4];
            (*raw).address = Address::new_from_array(address);
            (*raw).owner = Address::new_from_array(owner);
            (*raw).lamports = 0;
            (*raw).data_len = data.len() as u64;
            if !data.is_empty() {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    (raw as *mut u8).add(header),
                    data.len(),
                );
            }
            let view = AccountView::new_unchecked(raw);
            (buf, view)
        }
    }

    // ── NotEnoughAccountKeys ──────────────────────────────────────────────────

    #[test]
    fn not_enough_accounts_empty_slice() {
        define_account_struct! {
            struct Ctx<'info> { payer; }
            program_id: PROG_ID
        }
        assert_eq!(
            Ctx::from_accounts(&mut []).err().unwrap(),
            ProgramError::NotEnoughAccountKeys,
        );
    }

    #[test]
    fn not_enough_accounts_second_field() {
        define_account_struct! {
            struct Ctx<'info> { payer; admin; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, [0u8; 32], [0u8; 32], vec![]);
        assert_eq!(
            Ctx::from_accounts(&mut [view]).err().unwrap(),
            ProgramError::NotEnoughAccountKeys,
        );
    }

    // ── signer ────────────────────────────────────────────────────────────────

    #[test]
    fn signer_check_fails() {
        define_account_struct! {
            struct Ctx<'info> { payer: signer; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, [0u8; 32], [0u8; 32], vec![]);
        assert_eq!(
            Ctx::from_accounts(&mut [view]).err().unwrap(),
            ProgramError::MissingRequiredSignature,
        );
    }

    #[test]
    fn signer_check_passes() {
        define_account_struct! {
            struct Ctx<'info> { payer: signer; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(true, false, [0u8; 32], [0u8; 32], vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    // ── mut ───────────────────────────────────────────────────────────────────

    #[test]
    fn mut_check_fails() {
        define_account_struct! {
            struct Ctx<'info> { acct: mut; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, [0u8; 32], [0u8; 32], vec![]);
        assert_eq!(
            Ctx::from_accounts(&mut [view]).err().unwrap(),
            ProgramError::Immutable,
        );
    }

    #[test]
    fn mut_check_passes() {
        define_account_struct! {
            struct Ctx<'info> { acct: mut; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, true, [0u8; 32], [0u8; 32], vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    #[test]
    fn mut_field_try_borrow_mut() {
        define_account_struct! {
            struct Ctx<'info> { acct: mut; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, true, [0u8; 32], [0u8; 32], vec![1, 2, 3]);
        let mut accounts = [view];
        let ctx = Ctx::from_accounts(&mut accounts).unwrap();
        assert_mut_field(&mut *ctx.acct);
        let data = ctx.acct.try_borrow_mut().unwrap();
        assert_eq!(&*data, &[1, 2, 3]);
    }

    #[test]
    fn non_mut_field_is_immut_ref() {
        define_account_struct! {
            struct Ctx<'info> {
                payer: signer;
                acct: mut;
            }
            program_id: PROG_ID
        }
        let (_b0, v0) = make_account(true, false, [0u8; 32], [0u8; 32], vec![]);
        let (_b1, v1) = make_account(false, true, KEY_A, [0u8; 32], vec![]);
        let mut accounts = [v0, v1];
        let ctx = Ctx::from_accounts(&mut accounts).unwrap();
        assert_immut_field(ctx.payer);
        assert_mut_field(&mut *ctx.acct);
    }

    // ── empty ─────────────────────────────────────────────────────────────────

    #[test]
    fn empty_check_fails() {
        define_account_struct! {
            struct Ctx<'info> { acct: empty; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, [0u8; 32], [0u8; 32], vec![1, 2, 3]);
        assert_eq!(
            Ctx::from_accounts(&mut [view]).err().unwrap(),
            ProgramError::AccountAlreadyInitialized,
        );
    }

    #[test]
    fn empty_check_passes() {
        define_account_struct! {
            struct Ctx<'info> { acct: empty; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, [0u8; 32], [0u8; 32], vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    // ── @pubkey ───────────────────────────────────────────────────────────────

    #[test]
    fn pubkey_check_single_fails() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: @pubkey(Address::new_from_array(KEY_A));
            }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, KEY_B, [0u8; 32], vec![]);
        assert_eq!(
            Ctx::from_accounts(&mut [view]).err().unwrap(),
            ProgramError::IncorrectProgramId,
        );
    }

    #[test]
    fn pubkey_check_single_passes() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: @pubkey(Address::new_from_array(KEY_A));
            }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, KEY_A, [0u8; 32], vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    #[test]
    fn pubkey_check_multi_passes_second_key() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: @pubkey(Address::new_from_array(KEY_A), Address::new_from_array(KEY_B));
            }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, KEY_B, [0u8; 32], vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    // ── @owner ────────────────────────────────────────────────────────────────

    #[test]
    fn owner_check_fails() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: @owner(Address::new_from_array(KEY_A));
            }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, [0u8; 32], KEY_B, vec![]);
        assert_eq!(
            Ctx::from_accounts(&mut [view]).err().unwrap(),
            ProgramError::InvalidAccountOwner,
        );
    }

    #[test]
    fn owner_check_passes() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: @owner(Address::new_from_array(KEY_A));
            }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, [0u8; 32], KEY_A, vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    #[test]
    fn owner_check_multi_passes_second_owner() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: @owner(Address::new_from_array(KEY_A), Address::new_from_array(KEY_B));
            }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, [0u8; 32], KEY_B, vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    // ── opt_signer ────────────────────────────────────────────────────────────

    #[test]
    fn opt_signer_passes_when_address_is_program_id() {
        // address == program_id → treated as placeholder, signer not required
        define_account_struct! {
            struct Ctx<'info> { acct: opt_signer; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, PROG_ID, [0u8; 32], vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    #[test]
    fn opt_signer_fails_when_non_program_id_and_not_signer() {
        define_account_struct! {
            struct Ctx<'info> { acct: opt_signer; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, false, KEY_A, [0u8; 32], vec![]);
        assert_eq!(
            Ctx::from_accounts(&mut [view]).err().unwrap(),
            ProgramError::MissingRequiredSignature,
        );
    }

    #[test]
    fn opt_signer_passes_when_non_program_id_and_is_signer() {
        define_account_struct! {
            struct Ctx<'info> { acct: opt_signer; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(true, false, KEY_A, [0u8; 32], vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    // ── @remaining_accounts ───────────────────────────────────────────────────

    #[test]
    fn remaining_accounts_captured() {
        define_account_struct! {
            struct Ctx<'info> {
                payer;
                @remaining_accounts as remaining;
            }
            program_id: PROG_ID
        }
        let (_b0, v0) = make_account(false, false, [0u8; 32], [0u8; 32], vec![]);
        let (_b1, v1) = make_account(false, false, KEY_A, [0u8; 32], vec![]);
        let (_b2, v2) = make_account(false, false, KEY_B, [0u8; 32], vec![]);
        let mut accounts = [v0, v1, v2];
        let ctx = Ctx::from_accounts(&mut accounts).unwrap();
        assert_eq!(ctx.remaining.len(), 2);
        let _: &mut [AccountView] = ctx.remaining;
    }
}
