//! Zero-copy account loaders for use with [`define_account_struct`].

use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

use bytemuck::Pod;
use pinocchio::account::{Ref, RefMut};
use pinocchio::error::ProgramError;
use pinocchio::{AccountView, Address};

/// Fixed-discriminator bytemuck account layout: `[discriminator | Pod body]`.
pub trait ZeroCopyAccount: Pod + Sized {
    const DISCRIMINATOR: [u8; 1];
    const DISCRIMINATOR_SIZE: usize = 1;

    fn try_from_bytes(data: &[u8]) -> Result<&Self, ProgramError> {
        if data
            .get(..Self::DISCRIMINATOR_SIZE)
            .ok_or(ProgramError::InvalidAccountData)?
            != Self::DISCRIMINATOR
        {
            return Err(ProgramError::InvalidAccountData);
        }
        bytemuck::try_from_bytes(&data[Self::DISCRIMINATOR_SIZE..])
            .map_err(|_| ProgramError::InvalidAccountData)
    }

    fn try_from_bytes_mut(data: &mut [u8]) -> Result<&mut Self, ProgramError> {
        if data
            .get(..Self::DISCRIMINATOR_SIZE)
            .ok_or(ProgramError::InvalidAccountData)?
            != Self::DISCRIMINATOR
        {
            return Err(ProgramError::InvalidAccountData);
        }
        bytemuck::try_from_bytes_mut(&mut data[Self::DISCRIMINATOR_SIZE..])
            .map_err(|_| ProgramError::InvalidAccountData)
    }
}

/// Immutable zero-copy account; data borrow is taken in `from_accounts`.
pub struct Account<'info, T: ZeroCopyAccount> {
    /// Copy of the underlying account view (for address / CPI).
    pub view: AccountView,
    data: Ref<'info, T>,
    _pt: PhantomData<&'info T>,
}

impl<'info, T: ZeroCopyAccount> Account<'info, T> {
    /// Borrow account data and validate the zero-copy layout.
    ///
    /// Copies `AccountView` first so the view remains usable alongside the data borrow.
    pub fn try_load(account: &'info AccountView) -> Result<Self, ProgramError> {
        let view = *account;
        let data = account.try_borrow()?;
        let data = Ref::try_map(data, T::try_from_bytes).map_err(|(_, e)| e)?;
        Ok(Self {
            view,
            data,
            _pt: PhantomData,
        })
    }

    #[inline]
    pub fn address(&self) -> &Address {
        self.view.address()
    }

    #[inline]
    pub fn as_view(&self) -> &AccountView {
        &self.view
    }
}

impl<T: ZeroCopyAccount> Deref for Account<'_, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

/// Mutable zero-copy account; data borrow is taken in `from_accounts`.
pub struct MutAccount<'info, T: ZeroCopyAccount> {
    /// Copy of the underlying account view (for address / CPI).
    pub view: AccountView,
    data: RefMut<'info, T>,
    _pt: PhantomData<&'info mut T>,
}

impl<'info, T: ZeroCopyAccount> MutAccount<'info, T> {
    /// Mutably borrow account data and validate the zero-copy layout.
    ///
    /// Copies `AccountView` before `try_borrow_mut` so the view remains usable.
    pub fn try_load(account: &'info mut AccountView) -> Result<Self, ProgramError> {
        let view = *account;
        let data = account.try_borrow_mut()?;
        let data = RefMut::try_map(data, T::try_from_bytes_mut).map_err(|(_, e)| e)?;
        Ok(Self {
            view,
            data,
            _pt: PhantomData,
        })
    }

    #[inline]
    pub fn address(&self) -> &Address {
        self.view.address()
    }

    #[inline]
    pub fn as_view(&self) -> &AccountView {
        &self.view
    }
}

impl<T: ZeroCopyAccount> Deref for MutAccount<'_, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: ZeroCopyAccount> DerefMut for MutAccount<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

/// Mutable raw account bytes (for header + trailing payload layouts).
pub struct MutBytes<'info> {
    pub view: AccountView,
    data: RefMut<'info, [u8]>,
}

impl<'info> MutBytes<'info> {
    pub fn try_load(account: &'info mut AccountView) -> Result<Self, ProgramError> {
        let view = *account;
        let data = account.try_borrow_mut()?;
        Ok(Self { view, data })
    }

    #[inline]
    pub fn address(&self) -> &Address {
        self.view.address()
    }

    #[inline]
    pub fn as_view(&self) -> &AccountView {
        &self.view
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}
