#[cfg(any(test, feature = "tests_utils"))]
use std::time::Duration;
use std::time::Instant;

pub struct Clock {
    source: Source,
}

impl Clock {
    pub fn new() -> Clock {
        Clock {
            source: Source::System,
        }
    }

    #[cfg(any(test, feature = "tests_utils"))]
    pub fn new_mocked() -> Clock {
        Clock {
            source: Source::Mocked(std::sync::RwLock::new(Instant::now())),
        }
    }

    pub fn instant(&self) -> Instant {
        match &self.source {
            Source::System => Instant::now(),
            #[cfg(any(test, feature = "tests_utils"))]
            Source::Mocked(time) => {
                let mocked_instant = time.read().expect("Couldn't acquire read lock");
                *mocked_instant
            }
        }
    }

    #[cfg(any(test, feature = "tests_utils"))]
    pub fn set_instant(&self, current_time: Instant) {
        if let Source::Mocked(mocked_instant) = &self.source {
            let mut mocked_instant = mocked_instant.write().expect("Couldn't acquire write lock");
            *mocked_instant = current_time;
        } else {
            panic!("Called set_time, but clock source is system");
        }
    }

    #[cfg(any(test, feature = "tests_utils"))]
    pub fn add_instant_duration(&self, duration: Duration) {
        if let Source::Mocked(mocked_instant) = &self.source {
            let mut mocked_instant = mocked_instant.write().expect("Couldn't acquire write lock");
            *mocked_instant += duration;
        } else {
            panic!("Called set_time, but clock source is system");
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Clock::new()
    }
}

enum Source {
    System,
    #[cfg(any(test, feature = "tests_utils"))]
    Mocked(std::sync::RwLock<Instant>),
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_non_mocked_clock() {
        let now = Instant::now();

        let clock = Clock::new();
        let instant1 = clock.instant();
        assert!(instant1 > now);

        let instant2 = clock.instant();
        assert!(instant2 > instant1);
    }

    #[test]
    fn test_mocked_clock() {
        let mocked_clock = Clock::new_mocked();
        assert_eq!(mocked_clock.instant(), mocked_clock.instant());

        let new_instant = Instant::now() - Duration::from_secs(1);
        mocked_clock.set_instant(new_instant);

        assert_eq!(mocked_clock.instant(), new_instant);

        let dur_2secs = Duration::from_secs(2);
        mocked_clock.add_instant_duration(dur_2secs);
        assert_eq!(mocked_clock.instant(), new_instant + dur_2secs);
    }

    #[test]
    fn test_thread_safety() {
        let now = Instant::now();

        let mocked_clock = Arc::new(Clock::new_mocked());

        let thread_clock = Arc::clone(&mocked_clock);
        thread::spawn(move || {
            thread_clock.set_instant(now);
        })
        .join()
        .unwrap();

        assert_eq!(mocked_clock.instant(), now);
    }
}
