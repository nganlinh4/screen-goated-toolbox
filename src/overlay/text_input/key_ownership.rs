//! Pair low-level key transitions with the destination that received the down.

const KEY_COUNT: usize = 256;
const SLOTS: usize = KEY_COUNT * 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Foreground,
    Passive(u64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    /// Packs the modifiers into a message parameter so a key handled after
    /// the hook returns still sees the modifiers that were down for it.
    pub(super) fn to_bits(self) -> isize {
        (self.ctrl as isize)
            | (self.alt as isize) << 1
            | (self.shift as isize) << 2
            | (self.win as isize) << 3
    }

    pub(super) fn from_bits(bits: isize) -> Self {
        Self {
            ctrl: bits & 1 != 0,
            alt: bits & 2 != 0,
            shift: bits & 4 != 0,
            win: bits & 8 != 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Decision {
    pub consume: bool,
    pub deliver: bool,
    pub modifiers: Modifiers,
}

pub(super) struct KeyOwnership {
    held: [Option<Owner>; SLOTS],
    preexisting: [bool; KEY_COUNT],
}

impl Default for KeyOwnership {
    fn default() -> Self {
        Self {
            held: [None; SLOTS],
            preexisting: [false; KEY_COUNT],
        }
    }
}

impl KeyOwnership {
    /// Keys already down before the hook started belong to the foreground.
    pub(super) fn seed(&mut self, is_down: impl Fn(u32) -> bool) {
        for vk in 0..KEY_COUNT {
            // Generic modifier aliases duplicate the independent left/right keys.
            if matches!(vk, 0x10..=0x12) {
                continue;
            }
            if self.held[vk * 2].is_none() && self.held[vk * 2 + 1].is_none() && is_down(vk as u32)
            {
                self.preexisting[vk] = true;
            }
        }
    }

    /// Injected and physical events follow the same ownership rules.
    pub(super) fn event(
        &mut self,
        vk: u32,
        scan: u32,
        flags: u32,
        down: bool,
        session: Option<u64>,
    ) -> Decision {
        let extended = flags & 1 != 0;
        let vk = normalize(vk, scan, extended);
        let Some(index) = usize::try_from(vk).ok().filter(|vk| *vk < KEY_COUNT) else {
            return Decision {
                consume: false,
                deliver: false,
                modifiers: self.modifiers(),
            };
        };
        let slot = index * 2 + usize::from(extended);
        let owner = if down {
            *self.held[slot].get_or_insert_with(|| {
                if self.preexisting[index] {
                    Owner::Foreground
                } else {
                    session.map_or(Owner::Foreground, Owner::Passive)
                }
            })
        } else {
            self.preexisting[index] = false;
            self.held[slot].take().unwrap_or(Owner::Foreground)
        };
        Decision {
            consume: matches!(owner, Owner::Passive(_)),
            deliver: down
                && matches!((owner, session), (Owner::Passive(owner), Some(current)) if owner == current),
            modifiers: self.modifiers(),
        }
    }

    fn modifiers(&self) -> Modifiers {
        let down = |vk: usize| {
            self.preexisting[vk] || self.held[vk * 2].is_some() || self.held[vk * 2 + 1].is_some()
        };
        Modifiers {
            shift: down(0xa0) || down(0xa1),
            ctrl: down(0xa2) || down(0xa3),
            alt: down(0xa4) || down(0xa5),
            win: down(0x5b) || down(0x5c),
        }
    }
}

fn normalize(vk: u32, scan: u32, extended: bool) -> u32 {
    match vk {
        0x10 => {
            if scan == 0x36 {
                0xa1
            } else {
                0xa0
            }
        }
        0x11 => {
            if extended {
                0xa3
            } else {
                0xa2
            }
        }
        0x12 => {
            if extended {
                0xa5
            } else {
                0xa4
            }
        }
        _ => vk,
    }
}

pub(super) fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10..=0x12 | 0x5b..=0x5c | 0xa0..=0xa5)
}

#[cfg(test)]
#[path = "key_ownership_tests.rs"]
mod tests;
