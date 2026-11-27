//! The page table: one `u32` per virtual page (physical page + 1, flags, age), toroidally
//! addressed so a scrolling clipmap level never moves an entry. Mirrored in `shaders/vsm.slang`.

/// Texels per page side.
pub const PAGE_TEXELS: u32 = 128;
/// Pages per virtual map side.
pub const PAGES_PER_SIDE: u32 = 128;
/// Texels per virtual map side.
pub const VIRTUAL_TEXELS: u32 = PAGE_TEXELS * PAGES_PER_SIDE;

bitflags::bitflags! {
    /// A page's state (entry bits 16–23).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PageFlags: u8 {
        /// Some pixel needs it this frame.
        const REQUESTED = 1;
        /// Holds current depth.
        const VALID = 1 << 1;
        /// Must be redrawn.
        const DIRTY = 1 << 2;
        /// Has a dynamic-layer page (4.7b).
        const DYNAMIC = 1 << 3;
        /// Something under it changed this frame (4.7b): redrawn if requested, else released.
        const INVALIDATED = 1 << 4;
    }
}

/// One page-table entry: bits 0–15 the physical page + 1 (0: none), 16–23 flags, 24–31 age.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageEntry(u32);

impl PageEntry {
    /// No page, no flags (all zero, so a cleared table is empty).
    pub const EMPTY: Self = Self(0);

    /// An entry; `physical` is below `u16::MAX`.
    pub fn new(physical: Option<u16>, flags: PageFlags, age: u8) -> Self {
        debug_assert!(physical != Some(u16::MAX), "physical page out of range");
        let page = physical.map_or(0, |p| u32::from(p) + 1);
        Self(page | u32::from(flags.bits()) << 16 | u32::from(age) << 24)
    }

    /// From its bits.
    pub fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Its bits.
    pub fn bits(self) -> u32 {
        self.0
    }

    /// The physical page.
    pub fn physical(self) -> Option<u16> {
        ((self.0 & 0xFFFF) as u16).checked_sub(1)
    }

    /// The flags.
    pub fn flags(self) -> PageFlags {
        PageFlags::from_bits_truncate((self.0 >> 16) as u8)
    }

    /// Frames since last requested.
    pub fn age(self) -> u8 {
        (self.0 >> 24) as u8
    }
}

/// The table index of world page `page` (in that level's page units) of level `level`.
pub fn table_slot(level: u32, page: [i32; 2]) -> u32 {
    let side = PAGES_PER_SIDE as i32;
    let [x, y] = page.map(|p| p.rem_euclid(side) as u32);
    level * PAGES_PER_SIDE * PAGES_PER_SIDE + y * PAGES_PER_SIDE + x
}

/// The world page slot `slot` (x, y in 0..128) holds while its level's origin is `origin`: the one
/// world page of the level's 128 × 128 that maps to it.
pub fn slot_page(origin: [i32; 2], slot: [u32; 2]) -> [i32; 2] {
    let side = PAGES_PER_SIDE as i32;
    [0, 1].map(|i| origin[i] + (slot[i] as i32 - origin[i]).rem_euclid(side))
}
