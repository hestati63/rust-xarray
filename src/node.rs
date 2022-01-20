use crate::state::NodeOrState;
use crate::XaMark;

pub const CHUNK_SHIFT: usize = 6;
pub const CHUNK_SIZE: usize = 1 << CHUNK_SHIFT;
pub const CHUNK_MASK: usize = CHUNK_SIZE - 1;

#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct Mark {
    pub inner: [usize; (CHUNK_SIZE + usize::BITS as usize - 1) / usize::BITS as usize],
}

impl Mark {
    #[inline]
    pub fn set(&mut self, idx: usize) {
        let (p, ofs) = (idx / usize::BITS as usize, idx % usize::BITS as usize);
        self.inner[p] |= 1 << ofs;
    }

    #[inline]
    pub fn unset(&mut self, idx: usize) {
        let (p, ofs) = (idx / usize::BITS as usize, idx % usize::BITS as usize);
        self.inner[p] &= !(1 << ofs);
    }

    pub fn any(&mut self) -> bool {
        self.inner.iter().any(|n| *n != 0)
    }
}

pub struct Node<T> {
    pub shift: u8,
    pub offset: u8,
    pub count: u8,
    pub nr_value: u8,
    pub parent: RawEntry<T>,
    pub slots: [RawEntry<T>; CHUNK_SIZE],
    pub marks: [Mark; 3],
}

impl<T> Node<T> {
    #[inline]
    pub fn new(shift: u8, parent: &mut NodeOrState<T>) -> Option<Self> {
        if parent.is_empty() {
            Some(RawEntry::EMPTY)
        } else {
            parent.get().map(|n| RawEntry::node(n))
        }
        .map(|parent| Self {
            shift,
            offset: 0,
            count: 0,
            nr_value: 0,
            parent,
            slots: [RawEntry::EMPTY; CHUNK_SIZE],
            marks: [Mark::default(); 3],
        })
    }

    #[inline]
    pub const fn get_offset(&self, index: u64) -> u8 {
        ((index >> self.shift as u64) & CHUNK_MASK as u64) as u8
    }

    #[inline]
    pub fn entry(&mut self, index: u8) -> &mut RawEntry<T> {
        &mut self.slots[index as usize]
    }

    #[inline]
    pub fn as_raw(&self) -> RawEntry<T> {
        RawEntry::new(self as *const _ as usize | 2)
    }

    #[inline]
    pub fn mark_mut(&mut self, mark: XaMark) -> &mut Mark {
        match mark {
            XaMark::Mark0 => &mut self.marks[0],
            XaMark::Mark1 => &mut self.marks[1],
            XaMark::Mark2 => &mut self.marks[2],
        }
    }

    #[inline]
    pub fn mark(&self, mark: XaMark) -> &Mark {
        match mark {
            XaMark::Mark0 => &self.marks[0],
            XaMark::Mark1 => &self.marks[1],
            XaMark::Mark2 => &self.marks[2],
        }
    }

    pub fn find_mark(&self, start: u8, mark: XaMark) -> u8 {
        const USIZE_BITS: u8 = usize::BITS as u8;
        for (i, m) in self
            .mark(mark)
            .inner
            .iter()
            .enumerate()
            .skip((start / USIZE_BITS) as usize)
        {
            let mut m = if start / USIZE_BITS == i as u8 {
                *m & !((1 << (start % USIZE_BITS) as usize) - 1)
            } else {
                *m
            };
            if m != 0 {
                let mut n = 0;
                if m & 0xffffffff == 0 {
                    n += 32;
                    m >>= 32;
                }
                if m & 0xffff == 0 {
                    n += 16;
                    m >>= 16;
                }
                if m & 0xff == 0 {
                    n += 8;
                    m >>= 8;
                }
                if m & 0xf == 0 {
                    n += 4;
                    m >>= 4;
                }
                if m & 0x3 == 0 {
                    n += 2;
                    m >>= 2;
                }
                if m & 0x1 == 0 {
                    n += 1;
                }
                return n;
            }
        }
        CHUNK_SIZE as u8
    }

    #[inline]
    pub fn max_index(&self) -> u64 {
        ((CHUNK_SIZE as u64) << (self.shift as u64)) - 1
    }
}

#[derive(Eq)]
#[repr(transparent)]
pub struct RawEntry<T> {
    pub inner: usize,
    _t: core::marker::PhantomData<T>,
}

impl<T> core::fmt::Debug for RawEntry<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        match self.as_node_or_value() {
            Some(NodeOrValue::Node(n)) => write!(f, "Node<{:x}>", n as *const _ as usize),
            Some(NodeOrValue::Value(n)) => write!(f, "Value<{:x}>", n as *const _ as usize),
            _ => Ok(()),
        }
    }
}

impl<T> PartialEq for RawEntry<T> {
    fn eq(&self, o: &Self) -> bool {
        self.inner == o.inner
    }
}

impl<T> Clone for RawEntry<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner,
            _t: core::marker::PhantomData,
        }
    }
}

impl<T> Copy for RawEntry<T> {}

impl<T> RawEntry<T> {
    pub const EMPTY: Self = Self::new(0);

    const fn new(inner: usize) -> Self {
        Self {
            inner,
            _t: core::marker::PhantomData,
        }
    }

    pub fn value(v: &T) -> Self {
        Self::new(v as *const _ as usize | 1)
    }

    pub fn node(v: &Node<T>) -> Self {
        Self::new(v as *const _ as usize | 2)
    }

    pub fn sibling(v: u8) -> Self {
        Self::new((v as usize) << 2)
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.inner == 0
    }

    #[inline]
    pub fn is_internal(&self) -> bool {
        self.inner & 3 == 2
    }

    #[inline]
    pub fn is_value(&self) -> bool {
        self.inner & 1 == 1
    }

    #[inline]
    pub fn is_node(&self) -> bool {
        self.is_internal() && self.inner > 4096
    }

    #[inline]
    pub fn has_value(&self) -> bool {
        *self != Self::EMPTY
    }

    #[inline]
    pub fn is_sibling(&self) -> bool {
        self.is_internal() && self.inner < (((CHUNK_SIZE - 1) << 2) | 2)
    }

    #[inline]
    pub fn max_index(&mut self) -> u64 {
        if self.is_node() {
            let sz = CHUNK_SIZE << self.as_node().unwrap().shift as u64;
            let (r, _) = sz.overflowing_sub(1);
            r as u64
        } else {
            0
        }
    }

    #[inline]
    pub fn as_node<'a, 'b>(&'b self) -> Option<&'a mut Node<T>> {
        if self.is_node() {
            unsafe { ((self.inner - 2) as *mut Node<T>).as_mut() }
        } else {
            None
        }
    }

    #[inline]
    pub fn as_value<'a, 'b>(&'b self) -> Option<&'a T> {
        if self.is_value() {
            unsafe { ((self.inner - 1) as *const T).as_ref() }
        } else {
            None
        }
    }

    #[inline]
    pub fn as_sibling(&self) -> Option<u8> {
        if self.is_sibling() {
            Some((self.inner >> 2).try_into().unwrap())
        } else {
            None
        }
    }

    #[inline]
    pub fn as_node_or_value<'a, 'b, 'c>(&'c self) -> Option<NodeOrValue<'a, 'b, T>> {
        self.as_node()
            .map(NodeOrValue::Node)
            .or_else(|| self.as_value().map(NodeOrValue::Value))
    }
}

pub enum NodeOrValue<'a, 'b, T> {
    Node(&'a mut Node<T>),
    Value(&'b T),
}
