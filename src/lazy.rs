use std::cell::UnsafeCell;
use std::sync::Once;

pub struct Lazy<T> {
    init: Once,
    hold: UnsafeCell<Option<T>>,
}

unsafe impl<T: Send> Send for Lazy<T> {}
unsafe impl<T: Sync> Sync for Lazy<T> {}

impl<T> Lazy<T> {
    pub fn new() -> Self {
        Self {
            init: Once::new(),
            hold: UnsafeCell::new(None),
        }
    }

    pub fn get<F>(&self, f: F) -> &T
    where
        F: FnOnce() -> T,
    {
        self.init.call_once(|| unsafe {
            // SAFETY: This is safe because only one thread can access the `hold` because of the `Once`.
            *self.hold.get() = Some(f());
        });

        // SAFETY: This is safe because the value has already been initialized in the `Once` block.
        let hold = unsafe { &*self.hold.get() };
        hold.as_ref().expect("Lazy value was not initialized")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_build_once_and_only_once_the_value() {
        let lazy = Lazy::<u8>::new();

        lazy.get(|| 5_u8);
        lazy.get(|| unreachable!());
        let value = lazy.get(|| unreachable!());

        assert_eq!(value, &5_u8);
    }
}
