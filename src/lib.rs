#![no_std]

#[cfg(test)]
mod tests;

#[cfg(test)]
#[macro_use]
extern crate std;
extern crate alloc;

mod node;
mod state;
pub mod xarray;
pub mod xarray_raw;

pub use crate::xarray::{OwnedPointer, XArray};
pub use crate::xarray_raw::{RawXArray, XaMark};

use alloc::boxed::Box;

impl<T> OwnedPointer<T> for Box<T> {
    fn from_raw(t: *mut T) -> Self {
        unsafe { Box::from_raw(t) }
    }
    fn into_raw(self) -> &'static T {
        Box::leak(self)
    }
}

pub type XArrayBoxed<T> = XArray<T, Box<T>>;
