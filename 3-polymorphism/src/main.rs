use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;

trait Storage<K, V> {
    fn set(&mut self, key: K, val: V);
    fn get(&self, key: &K) -> Option<&V>;
    fn remove(&mut self, key: &K) -> Option<V>;
}

struct User {
    id: u64,
    email: Cow<'static, str>,
    activated: bool,
}

struct HashMapStorage<K, V> {
    inner: HashMap<K, V>,
}

impl<K, V> HashMapStorage<K, V>
where
    K: Eq + Hash,
{
    fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

impl<K, V> Storage<K, V> for HashMapStorage<K, V>
where
    K: Eq + Hash,
{
    fn set(&mut self, key: K, val: V) {
        self.inner.insert(key, val);
    }

    fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        self.inner.remove(key)
    }
}

struct UserRepositoryStatic<S>
where
    S: Storage<u64, User>,
{
    storage: S,
}

impl<S> UserRepositoryStatic<S>
where
    S: Storage<u64, User>,
{
    fn new(storage: S) -> Self {
        Self { storage }
    }

    fn get(&self, id: u64) -> Option<&User> {
        self.storage.get(&id)
    }

    fn add(&mut self, user: User) {
        let id = user.id;
        self.storage.set(id, user);
    }

    fn update(&mut self, user: User) -> bool {
        let id = user.id;
        let existed = self.storage.get(&id).is_some();
        self.storage.set(id, user);
        existed
    }

    fn remove(&mut self, id: u64) -> Option<User> {
        self.storage.remove(&id)
    }
}

struct UserRepositoryDynamic {
    storage: Box<dyn Storage<u64, User>>,
}

impl UserRepositoryDynamic {
    fn new(storage: Box<dyn Storage<u64, User>>) -> Self {
        Self { storage }
    }

    fn get(&self, id: u64) -> Option<&User> {
        self.storage.get(&id)
    }

    fn add(&mut self, user: User) {
        let id = user.id;
        self.storage.set(id, user);
    }

    fn update(&mut self, user: User) -> bool {
        let id = user.id;
        let existed = self.storage.get(&id).is_some();
        self.storage.set(id, user);
        existed
    }

    fn remove(&mut self, id: u64) -> Option<User> {
        self.storage.remove(&id)
    }
}

fn main() {
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user(id: u64, email: &str, activated: bool) -> User {
        User {
            id,
            email: Cow::Owned(email.to_string()),
            activated,
        }
    }

    #[test]
    fn static_repo_crud() {
        let storage = HashMapStorage::<u64, User>::new();
        let mut repo = UserRepositoryStatic::new(storage);

        let user1 = make_user(1, "u1@example.com", false);
        repo.add(user1);

        let got = repo.get(1).expect("user should exist");
        assert_eq!(got.id, 1);
        assert_eq!(got.email, Cow::from("u1@example.com"));
        assert!(!got.activated);

        let user1_updated = make_user(1, "u1@example.com", true);
        let existed = repo.update(user1_updated);
        assert!(existed);
        let got = repo.get(1).expect("user should still exist");
        assert!(got.activated);

        let removed = repo.remove(1).expect("user should be removed");
        assert_eq!(removed.id, 1);
        assert!(repo.get(1).is_none());

        let user2 = make_user(2, "u2@example.com", false);
        let existed = repo.update(user2);
        assert!(!existed);
        assert!(repo.get(2).is_some());
    }

    #[test]
    fn dynamic_repo_crud() {
        let storage = HashMapStorage::<u64, User>::new();
        let mut repo = UserRepositoryDynamic::new(Box::new(storage));

        let user1 = make_user(1, "u1@example.com", false);
        repo.add(user1);

        let got = repo.get(1).expect("user should exist");
        assert_eq!(got.id, 1);
        assert_eq!(got.email, Cow::from("u1@example.com"));
        assert!(!got.activated);

        let user1_updated = make_user(1, "u1@example.com", true);
        let existed = repo.update(user1_updated);
        assert!(existed);
        let got = repo.get(1).expect("user should still exist");
        assert!(got.activated);

        let removed = repo.remove(1).expect("user should be removed");
        assert_eq!(removed.id, 1);
        assert!(repo.get(1).is_none());

        let user2 = make_user(2, "u2@example.com", false);
        let existed = repo.update(user2);
        assert!(!existed);
        assert!(repo.get(2).is_some());
    }
}
