use std::collections::BinaryHeap;
use std::sync::Mutex;
use std::time::Duration;

use futures::channel::oneshot;
use futures::{Future, FutureExt};

use crate::binding::__exocore_host_now;

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct Timestamp(pub u64);

impl std::ops::Add<Duration> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Duration) -> Self::Output {
        Timestamp(self.0 + rhs.as_nanos() as u64)
    }
}

impl From<u64> for Timestamp {
    fn from(v: u64) -> Self {
        Timestamp(v)
    }
}

impl Into<u64> for Timestamp {
    fn into(self) -> u64 {
        self.0
    }
}

pub fn now() -> Timestamp {
    unsafe { Timestamp(__exocore_host_now()) }
}

pub async fn sleep(duration: Duration) {
    let time = now() + duration;
    TIMERS.push(time).await;
}

lazy_static! {
    static ref TIMERS: Timers = Timers::new();
}

struct Timers {
    timers: Mutex<BinaryHeap<std::cmp::Reverse<Timer>>>,
}

pub(crate) fn poll_timers() {
    TIMERS.poll();
}

pub(crate) fn next_timer_time() -> Option<Timestamp> {
    TIMERS.next_timer()
}

impl Timers {
    fn new() -> Timers {
        Timers {
            timers: Mutex::new(BinaryHeap::new()),
        }
    }

    fn poll(&self) {
        let mut timers = self.timers.lock().expect("Couldn't lock timers");
        let now = now();

        loop {
            if let Some(peek) = timers.peek() {
                if peek.0.time > now {
                    return;
                }
            } else {
                return;
            }

            let timer = timers.pop().expect("Couldn't pop");
            let _ = timer.0.sender.send(());
        }
    }

    fn next_timer(&self) -> Option<Timestamp> {
        let timers = self.timers.lock().expect("Couldn't lock timers");
        timers.peek().map(|t| t.0.time)
    }

    fn push(&self, time: Timestamp) -> impl Future<Output = ()> {
        let (sender, receiver) = oneshot::channel();

        let mut timers = self.timers.lock().unwrap();
        timers.push(std::cmp::Reverse(Timer { time, sender }));

        receiver.map(|_| ())
    }
}

struct Timer {
    time: Timestamp,
    sender: oneshot::Sender<()>,
}

// Not really Eq since 2 timers could have same trigger time. We only require this for ordering.
impl PartialEq for Timer {
    fn eq(&self, other: &Self) -> bool {
        self.time.eq(&other.time)
    }
}

impl Eq for Timer {}

impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.time.cmp(&other.time))
    }
}

impl Ord for Timer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time.cmp(&other.time)
    }
}
