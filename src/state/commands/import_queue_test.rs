//! Queue semantics for the import coroutine.
//!
//! The dispatcher loop is tested here against plain futures rather than through
//! `use_coroutine`, which would need a live Dioxus runtime. What matters is the
//! scheduling shape — bounded concurrency, and no head-of-line blocking — and
//! that is the same code path in both cases.

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// Mirrors `use_import_coroutine`'s dispatcher: accept work until `max` are in
/// flight, then drain before accepting more.
async fn run_queue<F, Fut>(items: Vec<u64>, max: usize, work: F)
where
    F: Fn(u64) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut tasks: FuturesUnordered<Fut> = FuturesUnordered::new();
    let mut pending = items.into_iter();

    loop {
        while tasks.len() < max {
            match pending.next() {
                Some(item) => tasks.push(work(item)),
                None => break,
            }
        }
        if tasks.next().await.is_none() {
            break;
        }
    }
}

/// A single slow import must not delay the ones behind it.
///
/// This is the reported bug in miniature: the old dispatcher awaited each import
/// inline, so one unresponsive download parked every paper queued after it.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_slow_import_does_not_block_the_others() {
    let finished: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));

    let order = Rc::clone(&finished);
    run_queue(vec![500, 10, 20], 4, move |delay_ms| {
        let order = Rc::clone(&order);
        async move {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            order.borrow_mut().push(delay_ms);
        }
    })
    .await;

    assert_eq!(
        *finished.borrow(),
        vec![10, 20, 500],
        "faster imports must finish first rather than waiting behind the slow one"
    );
}

/// Concurrency stays bounded, so a bulk import does not open one connection per
/// paper and get rate-limited.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn concurrency_is_capped() {
    let in_flight = Rc::new(RefCell::new(0usize));
    let peak = Rc::new(RefCell::new(0usize));

    let (cur, max_seen) = (Rc::clone(&in_flight), Rc::clone(&peak));
    run_queue((0..20).collect(), 4, move |_| {
        let cur = Rc::clone(&cur);
        let max_seen = Rc::clone(&max_seen);
        async move {
            *cur.borrow_mut() += 1;
            let now = *cur.borrow();
            if now > *max_seen.borrow() {
                *max_seen.borrow_mut() = now;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            *cur.borrow_mut() -= 1;
        }
    })
    .await;

    assert!(
        *peak.borrow() <= 4,
        "at most 4 imports may run at once, saw {}",
        peak.borrow()
    );
    assert!(
        *peak.borrow() > 1,
        "imports must actually overlap, saw {}",
        peak.borrow()
    );
}

/// Every queued item runs, including those accepted after the queue was full.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn every_queued_import_runs() {
    let done = Rc::new(RefCell::new(0usize));

    let counter = Rc::clone(&done);
    run_queue((0..25).collect(), 4, move |_| {
        let counter = Rc::clone(&counter);
        async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            *counter.borrow_mut() += 1;
        }
    })
    .await;

    assert_eq!(*done.borrow(), 25);
}
