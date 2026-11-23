use super::*;
use std::cell::Cell;

fn key(width: i32, height: i32) -> IconKey {
    IconKey {
        dark: true,
        width,
        height,
    }
}

#[test]
fn mask_uses_word_aligned_rows_and_rejects_bad_dimensions() {
    assert_eq!(mask_len(16, 16), Some(32));
    assert_eq!(mask_len(20, 20), Some(80));
    assert_eq!(mask_len(24, 24), Some(96));
    assert_eq!(mask_len(28, 28), Some(112));
    assert_eq!(mask_len(0, 1), None);
    assert_eq!(mask_len(-1, 1), None);
    assert_eq!(mask_len(i32::MAX, i32::MAX), None);
}

struct FakeIcon(Rc<Cell<usize>>);
impl Drop for FakeIcon {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn cache_reuses_owned_icons_and_releases_only_its_own_handles() {
    let drops = Rc::new(Cell::new(0));
    let mut cache = Cache::default();
    assert!(cache.insert(key(24, 24), FakeIcon(drops.clone())).is_ok());
    assert!(cache.get(key(24, 24)).is_some());
    // A rejected duplicate never transfers ownership to the cache.
    assert!(cache.insert(key(24, 24), FakeIcon(drops.clone())).is_err());
    assert_eq!(drops.get(), 1);
    drop(cache);
    assert_eq!(drops.get(), 2);
}

#[test]
fn bounded_cache_retains_uncertain_delivery_until_window_retirement() {
    let drops = Rc::new(Cell::new(0));
    let cache = Rc::new(RefCell::new(Cache::default()));
    for index in 0..MAX_CACHED_ICONS {
        assert!(
            cache
                .borrow_mut()
                .insert(key(index as i32 + 1, 1), FakeIcon(drops.clone()))
                .is_ok()
        );
    }
    assert!(
        cache
            .borrow_mut()
            .insert(key(100, 1), FakeIcon(drops.clone()))
            .is_err()
    );
    assert_eq!(drops.get(), 1);
    // An in-flight delivery holds the state even if destruction removes the
    // window's owner, so handles cannot be freed while a callback is reentrant.
    let in_flight = cache.clone();
    drop(cache);
    assert_eq!(drops.get(), 1);
    drop(in_flight);
    assert_eq!(drops.get(), MAX_CACHED_ICONS + 1);
}

#[test]
fn reentrant_destruction_keeps_a_dead_tombstone_and_preserves_reused_identity() {
    let mut windows = HashMap::new();
    let old = Rc::new(WindowIcons {
        alive: Cell::new(true),
        cache: RefCell::new(Cache::default()),
    });
    windows.insert(12, old.clone());
    let retiring = begin_retirement(&windows, 12).unwrap();
    // A reentrant lookup sees the dead owner, so it cannot install new icons.
    assert!(!windows.get(&12).unwrap().alive.get());
    let replacement = Rc::new(WindowIcons::default());
    windows.insert(12, replacement.clone());
    finish_retirement(&mut windows, 12, &retiring);
    assert!(Rc::ptr_eq(windows.get(&12).unwrap(), &replacement));
}
