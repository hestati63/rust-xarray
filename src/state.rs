use crate::node::*;
use crate::RawXArray;
use crate::XaMark;
use alloc::boxed::Box;

pub enum NodeOrState<'a, T>
where
    T: 'a,
{
    Empty,
    Bound,
    Restart,
    Node(&'a mut Node<T>),
}

impl<'a, T> NodeOrState<'a, T>
where
    T: 'a,
{
    #[inline]
    pub(crate) fn get(&self) -> Option<&'a mut Node<T>> {
        if let Self::Node(node) = self {
            unsafe { (*node as *const Node<T> as *mut Node<T>).as_mut() }
        } else {
            None
        }
    }
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
    #[inline]
    pub(crate) fn is_restart(&self) -> bool {
        matches!(self, Self::Restart)
    }
    #[inline]
    pub(crate) fn is_bound(&self) -> bool {
        matches!(self, Self::Bound)
    }
}

pub struct State<'a, T>
where
    T: 'a,
{
    pub index: u64,
    pub shift: u8,
    pub sibs: u8,
    pub offset: u8,
    pub node: NodeOrState<'a, T>,
}

impl<'c, T> State<'c, T>
where
    T: 'c,
{
    #[inline]
    pub fn new(index: u64) -> Self {
        State {
            index,
            shift: 0,
            sibs: 0,
            offset: 0,
            node: NodeOrState::Restart,
        }
    }

    pub fn load(&mut self, xa: &RawXArray<T>) -> RawEntry<T> {
        let mut entry = self
            .node
            .get()
            .map(|node| *node.entry(self.offset))
            .unwrap_or_else(|| match xa.head.as_node_or_value() {
                Some(NodeOrValue::Node(node))
                    if self.index >> node.shift as u64 > CHUNK_MASK as u64 =>
                {
                    self.node = NodeOrState::Bound;
                    RawEntry::EMPTY
                }
                Some(NodeOrValue::Value(_)) if self.index != 0 => {
                    self.node = NodeOrState::Bound;
                    RawEntry::EMPTY
                }
                _ => {
                    self.node = NodeOrState::Empty;
                    xa.head
                }
            });
        while let Some(node) = entry.as_node() {
            if self.shift > node.shift {
                entry = node.as_raw();
                break;
            }
            entry = self.descend(node);
            if self.node.get().unwrap().shift == 0 {
                break;
            }
        }
        entry
    }

    pub fn set_mark(&mut self, xa: &mut RawXArray<T>, mark: XaMark) {
        let mut node = self.node.get();
        let mut offset = self.offset;
        while let Some(n) = node {
            n.mark_mut(mark).set(offset as usize);
            offset = n.offset;
            node = n.parent.as_node();
        }
        xa.marks |= 1 << mark as usize;
    }

    pub fn unset_mark(&mut self, xa: &mut RawXArray<T>, mark: XaMark) {
        let mut node = self.node.get();
        let mut offset = self.offset;
        while let Some(n) = node {
            n.mark_mut(mark).unset(offset as usize);
            if n.mark_mut(mark).any() {
                return;
            }
            offset = n.offset;
            node = n.parent.as_node();
        }
        xa.marks &= !(1 << mark as usize);
    }

    pub fn store(&mut self, xa: &mut RawXArray<T>, mut entry: RawEntry<T>) -> RawEntry<T> {
        // https://elixir.bootlin.com/linux/latest/source/lib/xarray.c#L769
        let mut count = 0;
        let mut values = 0;
        let (mut first, is_value) = if entry.has_value() {
            (self.create(xa, !entry.is_node()), entry.is_value())
        } else {
            (self.load(xa), false)
        };
        if self.node.is_bound() || self.node.is_restart() {
            return first;
        }

        if matches!(self.node.get(), Some(node) if self.shift < node.shift) {
            self.sibs = 0;
        }

        if first == entry && self.sibs == 0 {
            return first;
        }

        let mut next = first;
        let mut offset = self.offset;
        let max = self.offset + self.sibs;
        let mut slot_info = if let Some(node) = self.node.get() {
            if self.sibs != 0 {
                // xas_squash_marks.
                todo!()
            }
            Some((node, offset))
        } else {
            None
        };

        loop {
            if let Some((slot_node, ofs)) = slot_info {
                *slot_node.entry(ofs) = entry;
                slot_info = Some((slot_node, ofs + 1));
            } else {
                xa.head = entry;
            }

            let next_has_value = next.has_value();
            match (next.as_node(), self.node.get()) {
                (Some(next), node) if node.as_ref().map(|n| n.shift != 0).unwrap_or(true) => {
                    xa.free_nodes(next);
                }
                _ => (),
            }
            if self.node.get().is_none() {
                break;
            }
            count += (!next_has_value as i32) - (!entry.has_value() as i32);
            values += (!first.is_value() as i32) - (!is_value as i32);
            if entry.has_value() {
                if offset == max {
                    break;
                }
                if !entry.is_sibling() {
                    entry = RawEntry::sibling(self.offset)
                }
            } else if offset == CHUNK_MASK as u8 {
                break;
            }
            offset += 1;
            next = *self.node.get().unwrap().entry(offset);
            if !next.is_sibling() {
                if !entry.has_value() && offset > max {
                    break;
                }
                first = next;
            }
        }
        self.update_node(xa, self.node.get(), count, values);
        first
    }

    fn create(&mut self, xa: &mut RawXArray<T>, allow_root: bool) -> RawEntry<T> {
        // https://elixir.bootlin.com/linux/latest/source/lib/xarray.c#L635
        let order = self.shift;
        let (mut slot, mut entry, mut shift) = if let Some(node) = self.node.get() {
            let offset = self.offset;
            let shift = node.shift;
            let entry = *node.entry(offset);
            let slot = node.entry(offset);
            (slot, entry, shift)
        } else {
            self.node = NodeOrState::Empty;
            if let Some(mut shift) = self.expand(xa, xa.head) {
                if shift == 0 && !allow_root {
                    shift = CHUNK_SHIFT as u8;
                }
                let en = xa.head;
                (&mut xa.head, en, shift)
            } else {
                return RawEntry::EMPTY;
            }
        };

        while shift > order {
            shift -= CHUNK_SHIFT as u8;
            let node = match entry.as_node_or_value() {
                Some(NodeOrValue::Node(en)) => en,
                Some(NodeOrValue::Value(_)) => break,
                None => {
                    if let Some(en) = self.alloc(shift) {
                        *slot = RawEntry::node(en);
                        en
                    } else {
                        break;
                    }
                }
            };
            entry = self.descend(node);
            slot = self.node.get().unwrap().entry(self.offset);
        }
        entry
    }

    fn max(&mut self) -> u64 {
        let mut max = self.index;
        let mask = self.size() - 1;
        if self.shift > 0 || self.sibs > 0 {
            max |= mask;
            if mask == max {
                max += 1;
            }
        }
        max
    }

    fn size(&mut self) -> u64 {
        (self.sibs as u64 + 1) << self.shift as u64
    }

    fn expand(&mut self, xa: &mut RawXArray<T>, mut head: RawEntry<T>) -> Option<u8> {
        let max = self.max();
        let mut shift = 0;
        let mut node = None;

        match head.as_node_or_value() {
            Some(NodeOrValue::Node(n)) => {
                shift = n.shift + CHUNK_SHIFT as u8;
                node = Some(n);
            }
            Some(_) => (),
            None => {
                if max == 0 {
                    return Some(0);
                }
                while (max >> shift) as usize >= CHUNK_SIZE {
                    shift += CHUNK_SHIFT as u8;
                }
                return Some(shift + CHUNK_SHIFT as u8);
            }
        }

        while max > head.max_index() {
            node = self.alloc(shift);
            if let Some(node) = node.as_mut() {
                node.count = 1;
                if head.is_value() {
                    node.nr_value = 1;
                }
                *node.entry(0) = head;

                for m in [XaMark::Mark0, XaMark::Mark1, XaMark::Mark2] {
                    if xa.is_marked(m) {
                        node.mark_mut(m).set(0);
                    }
                }

                if let Some(head) = head.as_node() {
                    head.offset = 0;
                    head.parent = RawEntry::node(node);
                }
                head = RawEntry::node(node);
                xa.head = head;
                shift += CHUNK_SHIFT as u8;
            } else {
                return None;
            }
        }
        self.node = NodeOrState::Node(node.unwrap());
        Some(shift)
    }

    fn alloc<'a, 'b>(&'a mut self, shift: u8) -> Option<&'b mut Node<T>> {
        Node::new(shift, &mut self.node)
            .map(|b| Box::leak(Box::new(b)))
            .map(|mut node| {
                if let Some(p) = self.node.get() {
                    node.offset = self.offset;
                    p.count += 1;
                }
                node
            })
    }

    fn update_node(
        &mut self,
        xa: &mut RawXArray<T>,
        node: Option<&mut Node<T>>,
        count: i32,
        values: i32,
    ) {
        if !(count == 0 && values == 0) {
            if let Some(node) = node {
                node.count = node.count.overflowing_add(count as u8).0;
                node.nr_value = node.nr_value.overflowing_add(values as u8).0;
                // xas_update
                if count < 0 {
                    self.delete_node(xa)
                }
            }
        }
    }

    fn delete_node(&mut self, xa: &mut RawXArray<T>) {
        let mut node = self.node.get().unwrap();
        while node.count == 0 {
            let boxed_node = unsafe { Box::from_raw(node) };
            self.offset = boxed_node.offset;

            if let Some(p) = boxed_node.parent.as_node() {
                *p.entry(self.offset) = RawEntry::EMPTY;
                p.count -= 1;
                self.node = NodeOrState::Node(p);
                node = self.node.get().unwrap();
            } else {
                xa.head = RawEntry::EMPTY;
                self.node = NodeOrState::Bound;
                return;
            }
        }
        if node.parent.is_null() {
            self.shrink(xa)
        }
    }

    fn shrink(&mut self, xa: &mut RawXArray<T>) {
        let mut node = self.node.get().unwrap();
        while node.count == 1 {
            let raw_entry = *node.entry(0);
            let entry = match raw_entry.as_node_or_value() {
                _ if !raw_entry.has_value() => break,
                Some(NodeOrValue::Node(node)) => {
                    if node.shift != 0 {
                        break;
                    }
                    Some(node)
                }
                _ => None,
            };

            self.node = NodeOrState::Bound;
            xa.head = raw_entry;

            unsafe { drop(Box::from_raw(node)) };

            if let Some(node_) = entry {
                node = node_;
                node.parent = RawEntry::EMPTY;
            } else {
                break;
            }
        }
    }

    fn descend(&mut self, node: &'c mut Node<T>) -> RawEntry<T> {
        let mut offset = node.get_offset(self.index);
        let mut entry = *node.entry(offset);

        if let Some(ofs) = entry.as_sibling() {
            offset = ofs;
            entry = *node.entry(offset);
        }
        self.node = NodeOrState::Node(node);
        self.offset = offset;
        entry
    }

    pub fn next(&mut self) {
        if !self.node.is_restart() {
            self.index += 1;
        }
        if self.node.is_empty() {
            self.node = NodeOrState::Bound;
            return;
        }
        if let Some(mut node) = self.node.get() {
            if self.offset != node.get_offset(self.index) {
                self.offset += 1;
            }

            while self.offset == CHUNK_SIZE as u8 {
                self.offset = node.offset + 1;
                if let Some(n) = node.parent.as_node() {
                    self.node = NodeOrState::Node(n);
                    node = self.node.get().unwrap();
                } else {
                    self.node = NodeOrState::Bound;
                    return;
                }
            }

            loop {
                let entry = *self.node.get().unwrap().entry(self.offset);
                if let Some(node) = entry.as_node() {
                    self.offset = node.get_offset(self.index);
                    self.node = NodeOrState::Node(node);
                } else {
                    break;
                }
            }
        }
    }

    fn move_index(&mut self, offset: u8) {
        let shift = self.node.get().unwrap().shift;
        self.index &= (!(CHUNK_MASK as u64)) << shift;
        self.index = self.index.overflowing_add((offset as u64) << shift).0;
    }

    pub fn find(&mut self, xa: &RawXArray<T>, end: u64) -> Option<RawEntry<T>> {
        if self.node.is_bound() {
            return None;
        }
        if self.index > end {
            self.node = NodeOrState::Bound;
            return None;
        }
        if self.node.is_empty() {
            self.index = 1;
            self.node = NodeOrState::Bound;
            return None;
        } else if self.node.is_restart() {
            let entry = self.load(xa);
            if entry.is_value() {
                return Some(entry);
            } else if !entry.is_node() {
                return None;
            }
        } else if let Some(node) = self.node.get() {
            if node.shift == 0 && node.offset != (self.index as usize & CHUNK_MASK) as u8 {
                self.offset = ((self.index as usize - 1) & CHUNK_MASK) as u8 + 1;
            }
        }

        self.offset += 1;
        self.move_index(self.offset);

        while self.node.get().is_some() && self.index < end {
            let node = self.node.get().unwrap();
            if self.offset == CHUNK_SIZE as u8 {
                self.offset = node.offset + 1;
                self.node = if let Some(node) = node.parent.as_node() {
                    NodeOrState::Node(node)
                } else {
                    NodeOrState::Empty
                };
                continue;
            }

            let entry = *node.entry(self.offset);
            if let Some(node) = entry.as_node() {
                self.node = NodeOrState::Node(node);
                self.offset = 0;
                continue;
            }

            if entry.is_value() && !entry.is_sibling() {
                return Some(entry);
            }

            self.offset += 1;
            self.move_index(self.offset);
        }

        if self.node.is_empty() {
            self.node = NodeOrState::Bound;
        }
        None
    }

    pub fn find_marked(
        &mut self,
        xa: &RawXArray<T>,
        end: u64,
        mark: XaMark,
    ) -> Option<RawEntry<T>> {
        if self.index > end {
            self.node = NodeOrState::Restart;
            return None;
        }
        let mut advance = if self.node.is_empty() {
            self.index = 1;
            self.node = NodeOrState::Bound;
            return None;
        } else if self.node.get().is_none() {
            self.node = NodeOrState::Empty;
            if self.index > xa.head.as_node().map(|n| n.max_index()).unwrap_or(0) {
                self.node = NodeOrState::Bound;
                return None;
            }
            if let Some(node) = xa.head.as_node() {
                self.offset = (self.index >> node.shift as u64).try_into().unwrap();
                self.node = NodeOrState::Node(node);
            } else {
                if xa.is_marked(mark) {
                    return Some(xa.head);
                }
                self.index = 1;
                self.node = NodeOrState::Bound;
                return None;
            }
            false
        } else {
            true
        };

        while self.index <= end {
            let node = self.node.get().unwrap();
            if self.offset == CHUNK_SIZE as u8 {
                self.offset = node.offset + 1;
                self.node = if let Some(node) = node.parent.as_node() {
                    NodeOrState::Node(node)
                } else {
                    NodeOrState::Empty
                };

                if matches!(self.node, NodeOrState::Empty) {
                    break;
                }
                advance = false;
                continue;
            }

            if !advance {
                if let Some(sib) = node.entry(self.offset).as_sibling() {
                    self.offset = sib;
                    self.move_index(self.offset);
                }
            }

            let offset = self
                .node
                .get()
                .unwrap()
                .find_mark(self.offset + advance as u8, mark);
            if offset > self.offset {
                advance = false;
                self.move_index(offset);
                if self.index > end {
                    self.node = NodeOrState::Restart;
                    return None;
                }
                self.offset = offset;
                if offset == CHUNK_SIZE as u8 {
                    continue;
                }
            }

            let entry = node.entry(self.offset);
            if let Some(node) = entry.as_node() {
                self.offset = node.get_offset(self.index);
                self.node = NodeOrState::Node(node);
            } else {
                return Some(*entry);
            }
        }
        self.node = if self.index > end {
            NodeOrState::Restart
        } else {
            NodeOrState::Bound
        };
        None
    }

    pub fn get_next(&mut self, xa: &RawXArray<T>, end: u64) -> Option<RawEntry<T>> {
        match self.node.get() {
            _ if self.offset != (self.index as usize & CHUNK_MASK) as u8 => self.find(xa, end),
            None => self.find(xa, end),
            Some(node) if node.shift > 0 => self.find(xa, end),
            Some(node) => loop {
                if self.index >= end || self.offset == CHUNK_MASK as u8 {
                    break self.find(xa, end);
                } else {
                    let entry = node.entry(self.offset + 1);
                    if entry.is_internal() {
                        break self.find(xa, end);
                    } else {
                        self.index += 1;
                        self.offset += 1;
                        if !entry.is_null() {
                            break Some(*entry);
                        }
                    }
                }
            },
        }
    }

    pub fn get_next_marked(
        &mut self,
        xa: &RawXArray<T>,
        mark: XaMark,
        end: u64,
    ) -> Option<RawEntry<T>> {
        match self.node.get() {
            None => self.find_marked(xa, end, mark),
            Some(node) if node.shift > 0 => self.find_marked(xa, end, mark),
            Some(node) => {
                let offset = node.find_mark(self.offset + 1, mark);
                self.offset = offset;
                self.index = (self.index & !CHUNK_MASK as u64) + offset as u64;
                if self.index > end {
                    None
                } else if offset == CHUNK_SIZE as u8 {
                    self.find_marked(xa, end, mark)
                } else {
                    let entry = node.entry(offset);
                    if entry.is_null() {
                        self.find_marked(xa, end, mark)
                    } else {
                        Some(*entry)
                    }
                }
            }
        }
    }
}
