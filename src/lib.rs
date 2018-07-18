extern crate lock_api;

use lock_api::RawRwLock;
use std::cell::UnsafeCell;
use std::collections::hash_map::HashMap;
use std::{hash, ops};

pub struct StorageMap<L, K, V, S> {
    lock: L,
    map: UnsafeCell<HashMap<K, V, S>>,
}

pub struct StorageMapGuard<'a, L: 'a + RawRwLock, V: 'a> {
    lock: &'a L,
    value: &'a V,
    exclusive: bool,
}

impl<'a, L: RawRwLock, V> ops::Deref for StorageMapGuard<'a, L, V> {
    type Target = V;
    fn deref(&self) -> &V {
        self.value
    }
}

impl<'a, L: RawRwLock, V> Drop for StorageMapGuard<'a, L, V> {
    fn drop(&mut self) {
        if self.exclusive {
            self.lock.unlock_exclusive();
        } else {
            self.lock.unlock_shared();
        }
    }
}

impl<L, K, V, S> StorageMap<L, K, V, S>
where
    L: RawRwLock,
    K: Clone + Eq + hash::Hash,
    S: hash::BuildHasher,
{
    pub fn with_hasher(hash_builder: S) -> Self {
        StorageMap {
            lock: L::INIT,
            map: UnsafeCell::new(HashMap::with_hasher(hash_builder)),
        }
    }

    pub fn get_or_create_with<'a, F: FnOnce() -> V>(
        &'a self, key: &K, create_fn: F
    ) -> StorageMapGuard<'a, L, V> {
        self.lock.lock_shared();
        // try mapping for reading first
        let map = unsafe { &*self.map.get() };
        if let Some(value) = map.get(key) {
            return StorageMapGuard {
                lock: &self.lock,
                value,
                exclusive: false,
            };
        }
        self.lock.unlock_shared();
        // now actually lock for writes
        let value = create_fn();
        self.lock.lock_exclusive();
        let map = unsafe { &mut *self.map.get() };
        StorageMapGuard {
            lock: &self.lock,
            value: &*map.entry(key.clone()).or_insert(value),
            exclusive: true,
        }
    }
}
