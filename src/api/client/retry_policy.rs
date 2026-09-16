//! Retry ownership for synchronous provider calls made inside a model chain.
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

thread_local! {
    static CHAIN_OWNS_RETRIES: Cell<bool> = const { Cell::new(false) };
}

pub(crate) struct ChainRetryGuard {
    previous: bool,
    _same_thread: PhantomData<Rc<()>>,
}

impl ChainRetryGuard {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            previous: CHAIN_OWNS_RETRIES.with(|value| value.replace(value.get() || enabled)),
            _same_thread: PhantomData,
        }
    }
}

impl Drop for ChainRetryGuard {
    fn drop(&mut self) {
        CHAIN_OWNS_RETRIES.with(|value| value.set(self.previous));
    }
}

pub(crate) fn provider_retries_allowed() -> bool {
    CHAIN_OWNS_RETRIES.with(|value| !value.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_ownership_is_nested_and_thread_scoped() {
        assert!(provider_retries_allowed());
        {
            let _chain = ChainRetryGuard::new(true);
            assert!(!provider_retries_allowed());
            {
                let _nested = ChainRetryGuard::new(false);
                assert!(!provider_retries_allowed());
            }
            assert!(!provider_retries_allowed());
            assert!(std::thread::spawn(provider_retries_allowed).join().unwrap());
        }
        assert!(provider_retries_allowed());
    }
}
