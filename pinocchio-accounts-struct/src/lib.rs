//! Macro for declaratively defining and validating account context structs
//! in Pinocchio programs, with optional eager zero-copy account loading.

mod account;

pub use account::{Account, MutAccount, MutBytes, ZeroCopyAccount};

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
    ($field:ident, $program_id:expr, bytes) => {};
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

/// Resolves a field's type from attrs + optional `@account(T)` / `bytes`.
#[doc(hidden)]
#[macro_export]
macro_rules! account_field_ty {
    // Typed + mut anywhere in attrs
    (@ty $lt:lifetime ; @account($ty:ty) ; mut $($rest:tt)*) => {
        $crate::MutAccount<$lt, $ty>
    };
    (@ty $lt:lifetime ; @account($ty:ty) ; $head:ident $($rest:tt)*) => {
        $crate::account_field_ty!(@ty $lt ; @account($ty) ; $($rest)*)
    };
    (@ty $lt:lifetime ; @account($ty:ty) ;) => {
        $crate::Account<$lt, $ty>
    };

    // Raw bytes (requires mut)
    (@ty $lt:lifetime ; @bytes ; mut $($rest:tt)*) => {
        $crate::MutBytes<$lt>
    };
    (@ty $lt:lifetime ; @bytes ; $head:ident $($rest:tt)*) => {
        $crate::account_field_ty!(@ty $lt ; @bytes ; $($rest)*)
    };
    (@ty $lt:lifetime ; @bytes ;) => {
        ::core::compile_error!("`bytes` account fields require the `mut` attribute")
    };

    // Untyped
    (@ty $lt:lifetime ; @none ; mut $($rest:tt)*) => {
        & $lt mut AccountView
    };
    (@ty $lt:lifetime ; @none ; $head:ident $($rest:tt)*) => {
        $crate::account_field_ty!(@ty $lt ; @none ; $($rest)*)
    };
    (@ty $lt:lifetime ; @none ;) => {
        & $lt AccountView
    };
}

/// Wraps a checked account reference into the field storage type.
#[doc(hidden)]
#[macro_export]
macro_rules! wrap_account_field {
    (@wrap $field:ident ; @account($ty:ty) ; mut $($rest:tt)*) => {
        let $field = $crate::MutAccount::<$ty>::try_load($field)?;
    };
    (@wrap $field:ident ; @account($ty:ty) ; $head:ident $($rest:tt)*) => {
        $crate::wrap_account_field!(@wrap $field ; @account($ty) ; $($rest)*);
    };
    (@wrap $field:ident ; @account($ty:ty) ;) => {
        let $field = $crate::Account::<$ty>::try_load($field)?;
    };

    (@wrap $field:ident ; @bytes ; mut $($rest:tt)*) => {
        let $field = $crate::MutBytes::try_load($field)?;
    };
    (@wrap $field:ident ; @bytes ; $head:ident $($rest:tt)*) => {
        $crate::wrap_account_field!(@wrap $field ; @bytes ; $($rest)*);
    };
    (@wrap $field:ident ; @bytes ;) => {
        ::core::compile_error!("`bytes` account fields require the `mut` attribute")
    };

    (@wrap $field:ident ; @none ; mut $($rest:tt)*) => {};
    (@wrap $field:ident ; @none ; $head:ident $($rest:tt)*) => {
        $crate::wrap_account_field!(@wrap $field ; @none ; $($rest)*);
    };
    (@wrap $field:ident ; @none ;) => {
        let $field: &AccountView = $field;
    };
}

/// Detect `bytes` in attrs without literal/`$ident` arm ambiguity.
#[doc(hidden)]
#[macro_export]
macro_rules! attrs_kind {
    (@scan $head:ident $($rest:ident)* ; $($all:ident)*) => {
        $crate::attrs_kind!(@is_bytes $head ; $($rest)* ; $($all)*)
    };
    (@scan ; $($all:ident)*) => {
        @none
    };
    (@is_bytes bytes ; $($rest:ident)* ; $($all:ident)*) => {
        @bytes
    };
    (@is_bytes $other:ident ; $($rest:ident)* ; $($all:ident)*) => {
        $crate::attrs_kind!(@scan $($rest)* ; $($all)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! account_field_ty_dispatch {
    ($lt:lifetime ; $($attrs:ident)* ; @account($ty:ty)) => {
        $crate::account_field_ty!(@ty $lt ; @account($ty) ; $($attrs)*)
    };
    ($lt:lifetime ; $($attrs:ident)* ;) => {
        $crate::account_field_ty_from_attrs!($lt ; $($attrs)*)
    };
}

/// Resolve untyped / bytes field type from attrs.
#[doc(hidden)]
#[macro_export]
macro_rules! account_field_ty_from_attrs {
    ($lt:lifetime ; $($attrs:ident)*) => {
        $crate::account_field_ty_from_attrs!(@scan $lt ; $($attrs)* ; $($attrs)*)
    };
    (@scan $lt:lifetime ; $head:ident $($rest:ident)* ; $($all:ident)*) => {
        $crate::account_field_ty_from_attrs!(@is_bytes $lt ; $head ; $($rest)* ; $($all)*)
    };
    (@scan $lt:lifetime ; ; $($all:ident)*) => {
        $crate::account_field_ty!(@ty $lt ; @none ; $($all)*)
    };
    (@is_bytes $lt:lifetime ; bytes ; $($rest:ident)* ; $($all:ident)*) => {
        $crate::account_field_ty!(@ty $lt ; @bytes ; $($all)*)
    };
    (@is_bytes $lt:lifetime ; $other:ident ; $($rest:ident)* ; $($all:ident)*) => {
        $crate::account_field_ty_from_attrs!(@scan $lt ; $($rest)* ; $($all)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! wrap_account_field_dispatch {
    ($field:ident ; $($attrs:ident)* ; @account($ty:ty)) => {
        $crate::wrap_account_field!(@wrap $field ; @account($ty) ; $($attrs)*);
    };
    ($field:ident ; $($attrs:ident)* ;) => {
        $crate::wrap_account_field_from_attrs!($field ; $($attrs)*);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! wrap_account_field_from_attrs {
    ($field:ident ; $($attrs:ident)*) => {
        $crate::wrap_account_field_from_attrs!(@scan $field ; $($attrs)* ; $($attrs)*);
    };
    (@scan $field:ident ; $head:ident $($rest:ident)* ; $($all:ident)*) => {
        $crate::wrap_account_field_from_attrs!(@is_bytes $field ; $head ; $($rest)* ; $($all)*);
    };
    (@scan $field:ident ; ; $($all:ident)*) => {
        $crate::wrap_account_field!(@wrap $field ; @none ; $($all)*);
    };
    (@is_bytes $field:ident ; bytes ; $($rest:ident)* ; $($all:ident)*) => {
        $crate::wrap_account_field!(@wrap $field ; @bytes ; $($all)*);
    };
    (@is_bytes $field:ident ; $other:ident ; $($rest:ident)* ; $($all:ident)*) => {
        $crate::wrap_account_field_from_attrs!(@scan $field ; $($rest)* ; $($all)*);
    };
}

/// Defines an account context struct and its `from_accounts` validator.
///
/// ### Example
/// ```ignore
/// define_account_struct! {
///     pub struct AcceptAdmin<'info> {
///         pending_admin: signer;
///         global_config: mut, @account(GlobalConfig)
///             @pubkey(GLOBAL_CONFIG_PDA) @owner(PROGRAM_ID);
///     }
///     program_id: crate::ID
/// }
/// ```
///
/// ### Supported syntax per field:
/// ```text
/// field: [attr, ...]? [@account(Type)]? [@pubkey(KEY)]? [@owner(KEY1, ...)]?;
/// ```
/// - `signer` — account must be signer
/// - `mut` — writable; with `@account(T)` loads [`MutAccount`], else `&mut AccountView`
/// - `empty` — account data must be empty
/// - `bytes` — with `mut`, loads [`MutBytes`] (raw data borrow)
/// - `opt_signer` — optional account; must be signer if not the program id
/// - `@account(Type)` — eager zero-copy load into [`Account`] / [`MutAccount`]
/// - `@pubkey` / `@owner` — key checks
///
/// `@remaining_accounts as name;` captures extras as `&mut [AccountView]`.
///
/// `from_accounts` takes `&mut [AccountView]` so typed mut accounts can be borrowed
/// during parsing.
#[macro_export]
macro_rules! define_account_struct {
    (
        $vis:vis struct $name:ident < $lt:lifetime > {
            $(
                $field:ident
                $( : $( $attr:ident ),* $(,)? )?
                $( @account( $account_ty:ty ) )?
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
                pub $field: $crate::account_field_ty_dispatch!(
                    $lt ; $($($attr)*)? ; $(@account($account_ty))?
                ),
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

                    $crate::wrap_account_field_dispatch!(
                        $field ; $($($attr)*)? ; $(@account($account_ty))?
                    );
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
    use bytemuck::{Pod, Zeroable};
    use pinocchio::{
        account::{RuntimeAccount, NOT_BORROWED},
        error::ProgramError,
        AccountView, Address,
    };

    use crate::ZeroCopyAccount;

    const PROG_ID: [u8; 32] = [1u8; 32];
    const KEY_A: [u8; 32] = [2u8; 32];
    const KEY_B: [u8; 32] = [3u8; 32];

    // Bags account layouts are packed (align 1) so a 1-byte discriminator is safe.
    #[repr(C, packed)]
    #[derive(Clone, Copy, Pod, Zeroable)]
    struct DemoState {
        value: u64,
    }

    impl ZeroCopyAccount for DemoState {
        const DISCRIMINATOR: [u8; 1] = [7];
    }

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
    fn signer_check_passes() {
        define_account_struct! {
            struct Ctx<'info> { payer: signer; }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(true, false, [0u8; 32], [0u8; 32], vec![]);
        assert!(Ctx::from_accounts(&mut [view]).is_ok());
    }

    #[test]
    fn mut_field_is_mut_ref() {
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

    #[test]
    fn typed_mut_account_loads_and_mutates() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: mut, @account(DemoState);
            }
            program_id: PROG_ID
        }
        let mut data = vec![7u8];
        data.extend_from_slice(&42u64.to_le_bytes());
        let (_buf, view) = make_account(false, true, KEY_A, PROG_ID, data);
        let mut accounts = [view];
        let mut ctx = Ctx::from_accounts(&mut accounts).unwrap();
        assert_eq!({ ctx.acct.value }, 42);
        ctx.acct.value = 99;
        assert_eq!(ctx.acct.address(), &Address::new_from_array(KEY_A));
    }

    #[test]
    fn typed_immut_account_loads() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: @account(DemoState);
            }
            program_id: PROG_ID
        }
        let mut data = vec![7u8];
        data.extend_from_slice(&7u64.to_le_bytes());
        let (_buf, view) = make_account(false, false, KEY_A, PROG_ID, data);
        let mut accounts = [view];
        let ctx = Ctx::from_accounts(&mut accounts).unwrap();
        assert_eq!({ ctx.acct.value }, 7);
    }

    #[test]
    fn bytes_mut_loads() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: mut, bytes;
            }
            program_id: PROG_ID
        }
        let (_buf, view) = make_account(false, true, KEY_A, PROG_ID, vec![1, 2, 3]);
        let mut accounts = [view];
        let mut ctx = Ctx::from_accounts(&mut accounts).unwrap();
        assert_eq!(ctx.acct.data(), &[1, 2, 3]);
        ctx.acct.data_mut()[0] = 9;
        assert_eq!(ctx.acct.data()[0], 9);
    }

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

    #[test]
    fn owner_and_typed_together() {
        define_account_struct! {
            struct Ctx<'info> {
                acct: mut, @account(DemoState) @owner(Address::new_from_array(PROG_ID));
            }
            program_id: PROG_ID
        }
        let mut data = vec![7u8];
        data.extend_from_slice(&1u64.to_le_bytes());
        let (_buf, view) = make_account(false, true, KEY_A, PROG_ID, data);
        let mut accounts = [view];
        let ctx = Ctx::from_accounts(&mut accounts).unwrap();
        assert_eq!({ ctx.acct.value }, 1);
    }
}
