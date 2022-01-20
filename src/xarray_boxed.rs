use crate::{xarray, XArray, XaMark};
use alloc::boxed::Box;

/// eXtensible Array (XArray) with Boxed element.
#[repr(transparent)]
pub struct XArrayBoxed<T>
where
    T: 'static,
{
    inner: XArray<'static, T>,
}

impl<T> core::ops::Deref for XArrayBoxed<T>
where
    T: 'static,
{
    type Target = XArray<'static, T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> core::ops::DerefMut for XArrayBoxed<T>
where
    T: 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> core::ops::Drop for XArrayBoxed<T>
where
    T: 'static,
{
    fn drop(&mut self) {
        for (_, v) in self.inner.iter() {
            unsafe {
                Box::from_raw(v as *const _ as *mut T);
            }
        }
    }
}

impl<T> XArrayBoxed<T>
where
    T: 'static,
{
    /// Create new XArrayBoxed Object.
    #[inline]
    pub const fn new() -> Self {
        Self {
            inner: XArray::new(),
        }
    }
    /// Insert value into the index.
    ///
    /// If the xarray does not contains the value at the index,
    /// [`None`] is returned.
    /// value is the reference of T, which outlives than self.
    #[inline]
    pub fn insert(&mut self, index: u64, value: T) -> Option<&'static T> {
        self.cursor_mut(index).insert(value)
    }

    /// Remove value at the index, returning the value at the index.
    #[inline]
    pub fn remove(&mut self, index: u64) -> Option<T> {
        self.cursor_mut(index).remove()
    }

    /// Provides a cursor with editing operations at the index.
    #[inline]
    pub fn cursor_mut(&mut self, index: u64) -> CursorMut<T> {
        CursorMut {
            inner: self.inner.cursor_mut(index),
        }
    }

    /// Extract range iterator starting from `start` to `end` (inclusive).
    pub fn extract_mut(&mut self, start: u64, end: u64) -> RangeMut<T> {
        RangeMut {
            cursor: self.cursor_mut(start),
            end,
            mark: None,
        }
    }
}

#[repr(transparent)]
pub struct CursorMut<'a, T>
where
    T: 'static,
{
    inner: xarray::CursorMut<'static, 'a, T>,
}

impl<'a, T> core::ops::Deref for CursorMut<'a, T>
where
    T: 'static,
{
    type Target = xarray::CursorMut<'static, 'a, T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> core::ops::DerefMut for CursorMut<'a, T>
where
    T: 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, T> CursorMut<'a, T>
where
    T: 'static,
{
    pub fn current_or_insert<F>(&mut self, f: F) -> (bool, &'static T)
    where
        F: FnOnce() -> T,
    {
        self.inner
            .current_or_insert(move || Box::leak(Box::new(f())))
    }

    /// Insert a new value into the xarray at the cursor.
    ///
    /// If the xarray does not contains the value at the index,
    /// [`None`] is returned.
    pub fn insert(&mut self, value: T) -> Option<&'static T> {
        self.inner.insert(Box::leak(Box::new(value)))
    }

    /// Remove the current element from the xarray.
    ///
    /// If the xarray does not contains the value at the index,
    /// [`None`] is returned.
    pub fn remove(&mut self) -> Option<T> {
        unsafe {
            self.inner
                .remove()
                .map(|n| *Box::from_raw(n as *const _ as *mut _))
        }
    }
}

pub struct RangeMut<'b, T>
where
    T: 'static,
{
    cursor: CursorMut<'b, T>,
    end: u64,
    mark: Option<XaMark>,
}

impl<'b, T> RangeMut<'b, T>
where
    T: 'static,
{
    pub fn filter_mark(mut self, mark: XaMark) -> Self {
        if self.mark.is_some() {
            panic!("Multiple mark cannot be filtered at once");
        }
        self.mark = Some(mark);
        self
    }

    pub fn as_cursor_mut(&mut self) -> &mut CursorMut<'b, T> {
        &mut self.cursor
    }
}

impl<'b, T> core::iter::Iterator for RangeMut<'b, T> {
    type Item = (u64, &'static T);

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            cursor: CursorMut {
                inner: xarray::CursorMut { xa, xas },
            },
            end,
            mark,
        } = self;

        if xas.index > *end {
            return None;
        }

        if let Some(mark) = *mark {
            xas.get_next_marked(xa, mark, *end)
        } else {
            xas.get_next(xa, *end)
        }
        .map(|n| (xas.index, n.as_value().unwrap()))
    }
}
