use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

type ShardedLock<V> = Arc<RwLock<Option<V>>>;

pub struct Cache<K, V> {
    values: Arc<Mutex<HashMap<K, ShardedLock<V>>>>,
}

impl<K, V> Cache<K, V>
where
    K: 'static + Send + Sync,
    V: 'static + Send + Sync,
{
    pub async fn get_or_try_init<F, R, E>(&self, key: K, f: F) -> Result<V, E>
    where
        K: Hash + Eq + Clone,
        F: FnOnce() -> R,
        R: Future<Output = Result<V, E>>,
        V: Clone,
    {
        let arc_to_calculate: Arc<RwLock<Option<V>>> = Arc::new(RwLock::new(None));

        let values_clone = self.values.clone();
        let arc_to_calculate_clone = arc_to_calculate.clone();
        let key_clone = key.clone();
        let spawn = tokio::task::spawn(async move {
            let mut guard = values_clone.lock().await;
            if guard.contains_key(&key_clone) {
                return guard
                    .get(&key_clone)
                    .unwrap()
                    .read()
                    .await
                    .as_ref()
                    .cloned();
            }
            guard.insert(key_clone, arc_to_calculate_clone);
            None
        })
        .await;

        match spawn.unwrap() {
            None => {}
            Some(value) => return Ok(value),
        }

        let mutex = self.values.clone().lock().await.get(&key).unwrap().clone();
        let mut lock = mutex.write_owned().await;

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
            values: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;

    use futures::future::join;
    use rand::Rng;
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
                    sleep(Duration::from_millis(random())).await;
                    calculations_first.fetch_add(1, Ordering::Relaxed);
                    sleep(Duration::from_millis(random())).await;
                    ok(1_u8)
                })
                .await
        });

        let second = spawn(async move {
            cache_second
                .get_or_try_init("key".to_owned(), || async move {
                    sleep(Duration::from_millis(random())).await;
                    calculations_second.fetch_add(1, Ordering::Relaxed);
                    sleep(Duration::from_millis(random())).await;
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
                    sleep(Duration::from_millis(random())).await;
                    calculations_first.fetch_add(1, Ordering::Relaxed);
                    sleep(Duration::from_millis(random())).await;
                    Err(())
                })
                .await
        });

        let second = spawn(async move {
            cache_second
                .get_or_try_init("key".to_owned(), || async move {
                    sleep(Duration::from_millis(random())).await;
                    calculations_second.fetch_add(1, Ordering::Relaxed);
                    sleep(Duration::from_millis(random())).await;
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
    fn random() -> u64 {
        rand::thread_rng().gen_range(100..200)
    }
}
