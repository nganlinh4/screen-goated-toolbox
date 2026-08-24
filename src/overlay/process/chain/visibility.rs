use std::sync::{Arc, Mutex};

pub(super) fn claim_result_reveal(window_shown: &Arc<Mutex<bool>>) -> bool {
    let mut shown = window_shown.lock().unwrap();
    if *shown {
        return false;
    }
    *shown = true;
    true
}

#[cfg(test)]
mod tests {
    use super::claim_result_reveal;
    use std::sync::{Arc, Mutex};

    #[test]
    fn terminal_result_claims_only_one_reveal() {
        let shown = Arc::new(Mutex::new(false));

        assert!(claim_result_reveal(&shown));
        assert!(!claim_result_reveal(&shown));
        assert!(*shown.lock().unwrap());
    }
}
