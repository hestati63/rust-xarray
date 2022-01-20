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

pub use xarray::{XArray, XaMark};
pub use xarray_boxed::XArrayBoxed;
