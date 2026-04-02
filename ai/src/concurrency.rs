use std::sync::{Arc, OnceLock};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

static LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();

pub fn init(max_concurrent: usize) {
    LIMITER.set(Arc::new(Semaphore::new(max_concurrent))).ok();
}

pub async fn acquire() -> OwnedSemaphorePermit {
    let sem = LIMITER.get_or_init(|| Arc::new(Semaphore::new(10)));
    Arc::clone(sem).acquire_owned().await.unwrap()
}
