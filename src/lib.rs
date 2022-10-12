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
pub mod xarray_boxed;

pub use crate::xarray::{XArray, XaMark};
pub use crate::xarray_boxed::XArrayBoxed;
