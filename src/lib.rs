extern crate lock_api;

use lock_api::RawRwLock;
use std::cell::UnsafeCell;
use std::collections::hash_map::HashMap;
use std::{hash, ops};

pub struct StorageMap<L, M> {
    lock: L,
    map: UnsafeCell<M>,
}

unsafe impl<L: Send, M> Send for StorageMap<L, M> {}
unsafe impl<L: Sync, M> Sync for StorageMap<L, M> {}

impl<L: RawRwLock, M: Default> Default for StorageMap<L, M> {
    fn default() -> Self {
        StorageMap {
            lock: L::INIT,
            map: UnsafeCell::new(M::default()),
        }
    }
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

pub enum PrepareResult {
    AlreadyExists,
    UnableToCreate,
    Created,
}

impl<L, K, V, S> StorageMap<L, HashMap<K, V, S>>
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

    /// The function is expected to always produce the same value given the same key.
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

    pub fn prepare_maybe<F: FnOnce() -> Option<V>>(
        &self, key: &K, create_fn: F
    ) -> PrepareResult {
        self.lock.lock_shared();
        // try mapping for reading first
        let map = unsafe { &*self.map.get() };
        let has = map.contains_key(key);
        self.lock.unlock_shared();
        if has {
            return PrepareResult::AlreadyExists;
        }
        // try creating a new value
        let value = match create_fn() {
            Some(value) => value,
            None => return PrepareResult::UnableToCreate,
        };
        // now actually lock for writes
        self.lock.lock_exclusive();
        let map = unsafe { &mut *self.map.get() };
        map.insert(key.clone(), value);
        self.lock.unlock_exclusive();
        PrepareResult::Created
    }
}
