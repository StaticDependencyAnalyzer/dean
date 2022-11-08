use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;

use tokio::sync::Mutex;

pub struct Cache<K, V> {
    values: Mutex<HashMap<K, Arc<Mutex<Option<V>>>>>,
}

impl<K, V> Cache<K, V> {
    pub async fn get_or_try_init<F, R, E>(&self, key: K, f: F) -> Result<V, E>
    where
        K: Hash + Eq + Clone,
        F: FnOnce() -> R,
        R: Future<Output = Result<V, E>>,
        V: Clone,
    {
        let mut arc_to_calculate: Option<Arc<Mutex<Option<V>>>> = None;

        {
            let mut guard = self.values.lock().await;
            if guard.contains_key(&key) {
                match guard.get(&key).unwrap().lock().await.as_ref() {
                    None => {}
                    Some(value) => return Ok(value.clone()),
                };
            }
            arc_to_calculate.replace(Arc::new(Mutex::new(None)));
            guard.insert(key.clone(), arc_to_calculate.as_ref().unwrap().clone());
        }

        let mutex = arc_to_calculate.as_ref().unwrap().clone();
        let mut lock = mutex.lock().await;

        if lock.is_some() {
            return Ok(lock.as_ref().unwrap().clone());
        }

        let result = f().await;
        match result {
            Ok(value) => {
                lock.replace(value);
                Ok(lock.as_ref().unwrap().clone())
            }
            Err(e) => Err(e),
        }
    }
}

impl<K, V> Cache<K, V> {
    pub fn new() -> Self {
        Self {
            values: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;

    use futures::future::join;
    use tokio::spawn;
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn it_builds_once_and_only_once_the_value() {
        let cache = Cache::<String, u8>::new();

        let value = cache
            .get_or_try_init("key".to_owned(), || async { ok(1_u8) })
            .await;

        assert_eq!(value.unwrap(), 1);
    }

    #[tokio::test]
    async fn when_multiple_values_are_calculated_it_calculates_only_one() {
        let cache = Arc::new(Cache::<String, u8>::new());
        let cache_first = cache.clone();
        let cache_second = cache.clone();

        let calculations = Arc::new(std::sync::atomic::AtomicI32::new(0));
        let calculations_first = calculations.clone();
        let calculations_second = calculations.clone();

        let first = spawn(async move {
            cache_first
                .get_or_try_init("key".to_owned(), || async move {
                    sleep(Duration::from_millis(100)).await;
                    calculations_first.fetch_add(1, Ordering::Relaxed);
                    sleep(Duration::from_millis(100)).await;
                    ok(1_u8)
                })
                .await
        });

        let second = spawn(async move {
            cache_second
                .get_or_try_init("key".to_owned(), || async move {
                    sleep(Duration::from_millis(100)).await;
                    calculations_second.fetch_add(1, Ordering::Relaxed);
                    sleep(Duration::from_millis(100)).await;
                    ok(1_u8)
                })
                .await
        });

        let (first, second) = join(first, second).await;

        assert_eq!(calculations.fetch_add(0, Ordering::Relaxed), 1);
        assert_eq!(first.unwrap().unwrap(), 1);
        assert_eq!(second.unwrap().unwrap(), 1);
    }

    #[tokio::test]
    async fn when_multiple_values_are_calculated_at_the_same_time_but_error_nothing_is_saved() {
        let cache = Arc::new(Cache::<String, u8>::new());
        let cache_first = cache.clone();
        let cache_second = cache.clone();

        let calculations = Arc::new(std::sync::atomic::AtomicI32::new(0));
        let calculations_first = calculations.clone();
        let calculations_second = calculations.clone();

        let first = spawn(async move {
            cache_first
                .get_or_try_init("key".to_owned(), || async move {
                    sleep(Duration::from_millis(100)).await;
                    calculations_first.fetch_add(1, Ordering::Relaxed);
                    sleep(Duration::from_millis(100)).await;
                    Err(())
                })
                .await
        });

        let second = spawn(async move {
            cache_second
                .get_or_try_init("key".to_owned(), || async move {
                    sleep(Duration::from_millis(100)).await;
                    calculations_second.fetch_add(1, Ordering::Relaxed);
                    sleep(Duration::from_millis(100)).await;
                    Err(())
                })
                .await
        });

        let (first, second) = join(first, second).await;

        assert_eq!(calculations.fetch_add(0, Ordering::Relaxed), 2);
        assert!(first.unwrap().is_err());
        assert!(second.unwrap().is_err());
    }

    #[allow(clippy::unnecessary_wraps)]
    fn ok<T>(val: T) -> Result<T, ()> {
        Ok(val)
    }
}
