use std::{
    collections::HashMap,
    future::Future,
    hash::Hash,
    sync::{Arc, RwLock, Weak},
};

use tokio::sync::broadcast;

struct LoadingGuard<I, T>
where
    I: Eq + Hash + Clone + Send + Sync + 'static,
    T: Clone + Send + 'static,
{
    id: I,
    futweakref: Weak<RwLock<HashMap<I, broadcast::Sender<T>>>>,
}

impl<I, T> LoadingGuard<I, T>
where
    I: Eq + Hash + Clone + Send + Sync + 'static,
    T: Clone + Send + 'static,
{
    pub fn new(id: I, ld: &Loading<I, T>) -> Self {
        Self {
            id,
            futweakref: Arc::downgrade(&ld.futures),
        }
    }

    fn sender(&mut self) -> Option<broadcast::Sender<T>> {
        let retv = self
            .futweakref
            .upgrade()?
            .write()
            .expect("poisoned mutex")
            .remove(&self.id)
            .expect("weird race condition");
        self.futweakref = Weak::new();
        Some(retv)
    }

    pub fn resolve(mut self, val: T) {
        if let Some(sender) = self.sender() {
            _ = sender.send(val);
        }
    }
}

impl<I, T> Drop for LoadingGuard<I, T>
where
    I: Eq + Hash + Clone + Send + Sync + 'static,
    T: Clone + Send + 'static,
{
    fn drop(&mut self) {
        drop(self.sender());
    }
}

/// Loading<I, T> is a helper struct intended in cases when values of type T,
/// identified by I:
/// * need to be loaded asynchronously, and
/// * it is preferable not to repeat the loading multiple times
///
/// The `Loading` struct helps solve the latter issue by only running
/// one task at a time for each I, reusing existing loads when
/// necessary.
pub struct Loading<I, T> {
    futures: Arc<RwLock<HashMap<I, broadcast::Sender<T>>>>,
}

impl<I, T> Loading<I, T>
where
    I: Eq + Hash + Clone + Send + Sync + 'static,
    T: Clone + Send + 'static,
{
    pub fn new() -> Self {
        Self {
            futures: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// If loading isn't in progress, runs `fut` and registers it as loading `I`.
    /// Else, waits for the currently active loading to finish and returns a cloned value.
    pub async fn run<F>(&self, id: I, fut: F) -> T
    where
        F: Future<Output = T> + Send + 'static,
    {
        let running_rx = self.futures.read().unwrap().get(&id).map(|s| s.subscribe());
        if let Some(mut rrx) = running_rx {
            return rrx.recv().await.unwrap();
        }
        let (tx, mut rx) = broadcast::channel(1);
        self.futures.write().unwrap().insert(id.clone(), tx);
        let futguard = LoadingGuard::new(id, self);
        tokio::spawn(async move {
            let futguard = futguard;
            futguard.resolve(fut.await);
        });
        rx.recv().await.unwrap()
    }
}

impl<I, T> Default for Loading<I, T>
where
    I: Eq + Hash + Clone + Send + Sync + 'static,
    T: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicU32, Ordering},
        time::Duration,
    };

    use super::*;

    #[tokio::test]
    async fn test_loading() {
        let loading = Loading::new();
        let num = Arc::new(AtomicU32::new(0));
        async fn f(x: Arc<AtomicU32>) -> u32 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            x.fetch_add(1, Ordering::SeqCst)
        }

        let (r1, r2) = tokio::join!(
            loading.run("meow", f(Arc::clone(&num))),
            loading.run("meow", f(Arc::clone(&num)))
        );

        assert_eq!(r1, 0);
        assert_eq!(r2, 0);

        tokio::time::sleep(Duration::from_secs(2)).await;

        let (r1, r2) = tokio::join!(
            loading.run("meow", f(Arc::clone(&num))),
            loading.run("meow", f(Arc::clone(&num)))
        );

        assert_eq!(r1, 1);
        assert_eq!(r2, 1);
    }
}
