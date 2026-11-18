//! Generation-checked storage behind RHI handles (shared by backends).

/// Values addressed by `(index, generation)`; reusing a slot bumps its generation so old handles go stale.
#[derive(Debug)]
pub struct Slots<T> {
    items: Vec<(u32, Option<T>)>,
    free: Vec<u32>,
}

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            free: Vec::new(),
        }
    }
}

impl<T> Slots<T> {
    /// Stores `value`, returning its `(index, generation)`.
    pub fn insert(&mut self, value: T) -> (u32, u32) {
        match self.free.pop() {
            Some(index) => {
                let slot = &mut self.items[index as usize];
                slot.0 = slot.0.wrapping_add(1);
                slot.1 = Some(value);
                (index, slot.0)
            }
            None => {
                self.items.push((0, Some(value)));
                (u32::try_from(self.items.len() - 1).unwrap_or(u32::MAX), 0)
            }
        }
    }

    /// The value, if the handle is current.
    pub fn get(&self, index: u32, generation: u32) -> Option<&T> {
        match self.items.get(index as usize) {
            Some((g, Some(v))) if *g == generation => Some(v),
            _ => None,
        }
    }

    /// Removes the value, if the handle is current.
    pub fn remove(&mut self, index: u32, generation: u32) -> Option<T> {
        let slot = self
            .items
            .get_mut(index as usize)
            .filter(|(g, v)| *g == generation && v.is_some())?;
        self.free.push(index);
        slot.1.take()
    }

    /// Live values in index order.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.items.iter().filter_map(|(_, v)| v.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::Slots;

    #[test]
    fn reused_slots_invalidate_old_handles() {
        let mut slots = Slots::default();
        let (i, g) = slots.insert("a");
        assert_eq!(slots.get(i, g), Some(&"a"));
        assert_eq!(slots.remove(i, g), Some("a"));
        assert_eq!(slots.get(i, g), None);
        let (j, h) = slots.insert("b");
        assert_eq!((j, h), (i, g + 1));
        assert_eq!(
            slots.remove(i, g),
            None,
            "stale handle cannot remove the new value"
        );
        assert_eq!(slots.values().collect::<Vec<_>>(), [&"b"]);
    }
}
