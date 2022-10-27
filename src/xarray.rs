use crate::{xarray_raw, RawXArray, XaMark};

pub trait OwnedPointer<T> {
    // Construct self from raw pointer.
    fn from_raw(t: *mut T) -> Self;
    // Consume and leaks self into raw pointer.
    fn into_raw(self) -> &'static T;
}

/// eXtensible Array (XArray) with Boxed element.
#[repr(transparent)]
pub struct XArray<T: 'static, V: OwnedPointer<T>> {
    inner: RawXArray<'static, T>,
    _l: core::marker::PhantomData<V>,
}

impl<'a, T: 'static, V: OwnedPointer<T>> core::ops::Deref for XArray<T, V> {
    type Target = RawXArray<'static, T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T: 'static, V: OwnedPointer<T>> core::ops::DerefMut for XArray<T, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, T: 'static, V: OwnedPointer<T>> Drop for XArray<T, V> {
    fn drop(&mut self) {
        for (_, v) in self.inner.iter() {
            let _ = V::from_raw(v as *const _ as *mut T);
        }
    }
}

impl<T: 'static, V: OwnedPointer<T>> XArray<T, V> {
    /// Create new XArrayBoxed Object.
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: RawXArray::new(),
            _l: core::marker::PhantomData,
        }
    }
    /// Insert value into the index.
    ///
    /// If the xarray does not contains the value at the index,
    /// [`None`] is returned.
    /// value is the reference of T, which outlives than self.
    #[inline]
    pub fn insert(&mut self, index: u64, value: V) -> Option<&'static T> {
        self.cursor_mut(index).insert(value)
    }

    /// Remove value at the index, returning the value at the index.
    #[inline]
    pub fn remove(&mut self, index: u64) -> Option<V> {
        self.cursor_mut(index).remove()
    }

    /// Provides a cursor with editing operations at the index.
    #[inline]
    pub fn cursor_mut(&mut self, index: u64) -> CursorMut<T, V> {
        CursorMut {
            inner: self.inner.cursor_mut(index),
            _v: core::marker::PhantomData,
        }
    }

    /// Extract range iterator starting from `start` to `end` (inclusive).
    pub fn extract_mut(&mut self, start: u64, end: u64) -> RangeMut<T, V> {
        RangeMut {
            cursor: self.cursor_mut(start),
            end,
            mark: None,
        }
    }
}

#[repr(transparent)]
pub struct CursorMut<'a, T: 'static, V: OwnedPointer<T>> {
    inner: xarray_raw::CursorMut<'static, 'a, T>,
    _v: core::marker::PhantomData<V>,
}

impl<'a, T: 'static, V: OwnedPointer<T>> core::ops::Deref for CursorMut<'a, T, V> {
    type Target = xarray_raw::CursorMut<'static, 'a, T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T: 'static, V: OwnedPointer<T>> core::ops::DerefMut for CursorMut<'a, T, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, T: 'static, V: OwnedPointer<T>> CursorMut<'a, T, V> {
    pub fn current_or_insert<F>(&mut self, f: F) -> (bool, &'static T)
    where
        F: FnOnce() -> V,
    {
        self.inner.current_or_insert(move || V::into_raw(f()))
    }

    /// Insert a new value into the xarray at the cursor.
    ///
    /// If the xarray does not contains the value at the index,
    /// [`None`] is returned.
    pub fn insert(&mut self, value: V) -> Option<&'static T> {
        self.inner.insert(V::into_raw(value))
    }

    /// Remove the current element from the xarray.
    ///
    /// If the xarray does not contains the value at the index,
    /// [`None`] is returned.
    pub fn remove(&mut self) -> Option<V> {
        self.inner
            .remove()
            .map(|n| V::from_raw(n as *const _ as *mut _))
    }
}

pub struct RangeMut<'b, T: 'static, V: OwnedPointer<T>>
where
    T: 'static,
{
    cursor: CursorMut<'b, T, V>,
    end: u64,
    mark: Option<XaMark>,
}

impl<'b, T: 'static, V: OwnedPointer<T>> RangeMut<'b, T, V> {
    pub fn filter_mark(mut self, mark: XaMark) -> Self {
        if self.mark.is_some() {
            panic!("Multiple mark cannot be filtered at once");
        }
        self.mark = Some(mark);
        self
    }

    pub fn as_cursor_mut(&mut self) -> &mut CursorMut<'b, T, V> {
        &mut self.cursor
    }
}

impl<'b, T: 'static, V: OwnedPointer<T>> core::iter::Iterator for RangeMut<'b, T, V> {
    type Item = (u64, &'static T);

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            cursor:
                CursorMut {
                    inner: xarray_raw::CursorMut { xa, xas },
                    ..
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
