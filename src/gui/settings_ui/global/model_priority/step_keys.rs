/// Position-independent identity that travels with a row through reorders.
#[derive(Clone, Default)]
pub(super) struct StepKeys {
    next: u64,
    pub(super) keys: Vec<u64>,
}

impl StepKeys {
    pub(super) fn sync(&mut self, len: usize) {
        while self.keys.len() < len {
            self.keys.push(self.next);
            self.next += 1;
        }
        self.keys.truncate(len);
    }

    pub(super) fn at(&self, idx: usize) -> u64 {
        self.keys.get(idx).copied().unwrap_or_default()
    }

    pub(super) fn move_step(&mut self, from: usize, to: usize) {
        let key = self.keys.remove(from);
        self.keys.insert(to, key);
    }

    pub(super) fn forget(&mut self, idx: usize) {
        self.keys.remove(idx);
    }
}
