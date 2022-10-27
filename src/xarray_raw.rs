pub(crate) use super::node::{Node, NodeOrValue, RawEntry, CHUNK_MASK, CHUNK_SIZE};
pub(crate) use super::state::State;

use alloc::boxed::Box;

/// eXtensible Array (XArray).
///
/// Array abtraction of Linux kernel's radix tree.
pub struct RawXArray<'a, T>
where
    T: 'a,
{
    pub(crate) marks: usize,
    pub(crate) head: RawEntry<T>,
    _entry_lt: core::marker::PhantomData<&'a ()>,
}

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum XaMark {
    Mark0 = 0,
    Mark1 = 1,
    Mark2 = 2,
}

impl<'a, T> RawXArray<'a, T>
where
    T: 'a,
{
    /// Create new XArray Object.
    #[inline]
    pub const fn new() -> Self {
        Self {
            marks: 0,
            head: RawEntry::EMPTY,
            _entry_lt: core::marker::PhantomData,
        }
    }

    /// Determine if an array has any present entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head.is_null()
    }

    /// Inquire whether any entry in this array has a mark set.
    #[inline]
    pub fn is_marked(&self, mark: XaMark) -> bool {
        self.marks & (1 << mark as usize) != 0
    }

    /// Get value at the index.
    ///
    /// If the xarray contains the value at the index, return [`Some`].
    /// Otherwise, return [`None`].
    #[inline]
    pub fn get(&self, index: u64) -> Option<&'a T> {
        self.cursor(index).current()
    }

    /// Insert value into the index.
    ///
    /// If the xarray does not contains the value at the index,
    /// [`None`] is returned.
    /// value is the reference of T, which outlives than self.
    #[inline]
    pub fn insert<'b>(&'b mut self, index: u64, value: &'a T) -> Option<&'a T>
    where
        'a: 'b,
    {
        self.cursor_mut(index).insert(value)
    }

    /// Insert a value computed from `f` if the given `index` is not present,
    /// then returns a tuple of boolean that indicates whether new
    /// entry is created and reference that stored in the XArray each.
    #[inline]
    pub fn get_or_with<'b, F>(&'b mut self, index: u64, f: F) -> (bool, &'a T)
    where
        F: FnOnce() -> &'a T,
        'a: 'b,
    {
        self.cursor_mut(index).current_or_insert(f)
    }

    /// Remove value at the index, returning the value at the index.
    #[inline]
    pub fn remove(&mut self, index: u64) -> Option<&'a T> {
        self.cursor_mut(index).remove()
    }

    /// Provides a cursor at the index.
    #[inline]
    pub fn cursor<'b>(&'b self, index: u64) -> Cursor<'a, 'b, T> {
        Cursor {
            xa: self,
            xas: State::new(index),
        }
    }

    /// Provides a cursor with editing operations at the index.
    #[inline]
    pub fn cursor_mut<'b>(&'b mut self, index: u64) -> CursorMut<'a, 'b, T> {
        CursorMut {
            xa: self,
            xas: State::new(index),
        }
    }

    /// Extract range iterator starting from `start` to `end` (inclusive).
    pub fn extract(&self, start: u64, end: u64) -> Range<T> {
        Range {
            cursor: self.cursor(start),
            end,
            mark: None,
        }
    }

    /// Extract range iterator starting from `start` to `end` (inclusive).
    pub fn extract_mut<'b>(&'b mut self, start: u64, end: u64) -> RangeMut<'a, 'b, T> {
        RangeMut {
            cursor: self.cursor_mut(start),
            end,
            mark: None,
        }
    }

    /// Get iterator of the Xarray
    pub fn iter(&self) -> Range<T> {
        self.extract(0, u64::MAX)
    }

    /// Get mutable iterator of the Xarray
    pub fn iter_mut<'b>(&'b mut self) -> RangeMut<'a, 'b, T> {
        self.extract_mut(0, u64::MAX)
    }

    pub(crate) fn free_nodes(&mut self, mut node: &mut Node<T>) {
        let mut offset = 0;
        let raw_top = RawEntry::node(node);
        loop {
            match node.entry(offset).as_node() {
                Some(n) if node.shift > 0 => {
                    node = n;
                    offset = 0;
                    continue;
                }
                _ => (),
            }

            offset += 1;

            while offset == CHUNK_SIZE as u8 {
                let parent = node.parent;
                offset = node.offset + 1;
                node.count = 0;
                node.nr_value = 0;

                let is_node_top = node.as_raw() == raw_top;
                // drop.
                unsafe { drop(Box::from_raw(node)) };
                if is_node_top {
                    return;
                }
                node = parent.as_node().unwrap();
            }
        }
    }
}

impl<'a, T> core::fmt::Debug for RawXArray<'a, T>
where
    T: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        fn fmt_inner<T>(
            f: &mut core::fmt::Formatter<'_>,
            node: &mut Node<T>,
            d: usize,
        ) -> core::fmt::Result
        where
            T: core::fmt::Debug,
        {
            for i in 0..CHUNK_SIZE {
                match node.entry(i as u8).as_node_or_value() {
                    Some(NodeOrValue::Node(nn)) => {
                        for _ in 0..d {
                            write!(f, "  ")?;
                        }
                        writeln!(f, "#{}: Node,", i)?;
                        fmt_inner(f, nn, d + 1)?;
                    }
                    Some(NodeOrValue::Value(v)) => {
                        for _ in 0..d {
                            write!(f, "  ")?;
                        }
                        writeln!(f, "#{}: {:?},", i, v)?;
                    }
                    _ => (),
                }
            }
            Ok(())
        }
        writeln!(f, "XArray {{")?;
        if let Some(head) = self.head.as_node() {
            fmt_inner(f, head, 1)?;
        }
        writeln!(f, "}}")
    }
}

impl<'a, T> core::ops::Drop for RawXArray<'a, T>
where
    T: 'a,
{
    fn drop(&mut self) {
        if let Some(head) = self.head.as_node() {
            self.free_nodes(head);
        }
    }
}

pub struct Cursor<'a, 'b, T> {
    xa: &'b RawXArray<'a, T>,
    xas: State<'b, T>,
}

impl<'a, 'b, T> Cursor<'a, 'b, T> {
    /// Returns a reference to the element that the cursor is currently pointing
    /// to.
    ///
    /// If the underlying value is exist, return [`Some`].
    /// Otherwise, return [`None`].
    #[inline]
    pub fn current(&mut self) -> Option<&'a T> {
        // https://elixir.bootlin.com/linux/latest/source/lib/xarray.c#L1298
        let Self { xa, xas } = self;
        xas.load(xa).as_value()
    }

    /// Returns a key that the cursor is currently pointing to.
    #[inline]
    pub fn key(&mut self) -> u64 {
        self.xas.index
    }

    /// Move the cursor to next allocated value.
    #[inline]
    pub fn next_allocated(&mut self) {
        let Self { xas, xa } = self;
        xas.get_next(xa, u64::MAX);
    }
}

pub struct CursorMut<'a, 'b, T> {
    pub(crate) xa: &'b mut RawXArray<'a, T>,
    pub(crate) xas: State<'b, T>,
}

impl<'a, 'b, T> CursorMut<'a, 'b, T> {
    /// Returns a reference to the element that the cursor is currently pointing
    /// to.
    ///
    /// If the underlying value is exist, return [`Some`].
    /// Otherwise, return [`None`].
    #[inline]
    pub fn current(&mut self) -> Option<&'b T> {
        // https://elixir.bootlin.com/linux/latest/source/lib/xarray.c#L1298
        let Self { xa, xas } = self;
        xas.load(xa).as_value()
    }

    /// Set marks on the element that the cursor is currently pointing to.
    #[inline]
    pub fn mark(&mut self, marks: XaMark) {
        let Self { xa, xas } = self;
        if xas.load(xa).is_value() {
            xas.set_mark(xa, marks)
        }
    }

    /// Remove marks on the element that the cursor is currently pointing to.
    #[inline]
    pub fn unmark(&mut self, marks: XaMark) {
        let Self { xa, xas } = self;
        if xas.load(xa).is_value() {
            xas.unset_mark(xa, marks)
        }
    }

    #[inline]
    pub fn next(&mut self) {
        let Self { ref mut xas, .. } = self;
        match xas.node.get() {
            Some(node) if node.shift == 0 && xas.offset != CHUNK_MASK as u8 => {
                xas.index += 1;
                xas.offset += 1;
            }
            _ => xas.next(),
        }
    }

    #[inline]
    pub fn current_or_insert<F>(&mut self, f: F) -> (bool, &'a T)
    where
        T: 'a,
        F: FnOnce() -> &'a T,
    {
        let Self { xa, xas } = self;

        if let Some(curr) = xas.load(xa).as_value() {
            (false, curr)
        } else {
            let value = f();
            xas.store(xa, RawEntry::value(value));
            (true, value)
        }
    }

    /// Insert a new value into the xarray at the cursor.
    ///
    /// If the xarray does not contains the value at the index,
    /// [`None`] is returned.
    /// value is the reference of T, which outlives than self.
    #[inline]
    pub fn insert(&mut self, value: &'a T) -> Option<&'a T> {
        let Self { xa, xas } = self;

        if let Some(v) = xas.load(xa).as_value() {
            Some(v)
        } else {
            xas.store(xa, RawEntry::value(value));
            None
        }
    }

    /// Remove the current element from the xarray.
    ///
    /// If the xarray does not contains the value at the index,
    /// [`None`] is returned.
    /// value is the reference of T, which outlives than self.
    #[inline]
    pub fn remove(&mut self) -> Option<&'a T> {
        let Self { xa, xas } = self;

        if let Some(v) = xas.load(xa).as_value() {
            xas.store(xa, RawEntry::EMPTY);
            Some(v)
        } else {
            None
        }
    }

    /// Returns a key that the cursor is currently pointing to.
    #[inline]
    pub fn key(&mut self) -> u64 {
        self.xas.index
    }

    /// Move the cursor to next allocated value.
    #[inline]
    pub fn next_allocated(&mut self) {
        let Self { xas, xa } = self;
        xas.get_next(xa, u64::MAX);
    }
}

pub struct Range<'a, 'b, T> {
    cursor: Cursor<'a, 'b, T>,
    end: u64,
    mark: Option<XaMark>,
}

impl<'a, 'b, T> Range<'a, 'b, T> {
    #[inline]
    pub fn filter_mark(mut self, mark: XaMark) -> Self {
        if self.mark.is_some() {
            panic!("Multiple mark cannot be filtered at once");
        }
        self.mark = Some(mark);
        self
    }

    #[inline]
    pub fn as_cursor(&self) -> &Cursor<'a, 'b, T> {
        &self.cursor
    }
}

impl<'a, 'b, T> core::iter::Iterator for Range<'a, 'b, T> {
    type Item = (u64, &'b T);

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            cursor: Cursor { xa, xas },
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

pub struct RangeMut<'a, 'b, T> {
    cursor: CursorMut<'a, 'b, T>,
    end: u64,
    mark: Option<XaMark>,
}

impl<'a, 'b, T> RangeMut<'a, 'b, T> {
    #[inline]
    pub fn filter_mark(mut self, mark: XaMark) -> Self {
        if self.mark.is_some() {
            panic!("Multiple mark cannot be filtered at once");
        }
        self.mark = Some(mark);
        self
    }

    #[inline]
    pub fn as_cursor_mut(&mut self) -> &mut CursorMut<'a, 'b, T> {
        &mut self.cursor
    }
}

impl<'a, 'b, T> core::iter::Iterator for RangeMut<'a, 'b, T> {
    type Item = (u64, &'b T);

    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            cursor: CursorMut { xa, xas },
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
