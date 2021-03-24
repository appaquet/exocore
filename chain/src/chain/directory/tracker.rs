use std::{
    collections::HashMap,
    slice::from_raw_parts,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    usize,
};

type SegmentId = usize;

/// Tracks opened segments in order to attempt to limit the number of concurrent
/// opened segments and prevent going over operating system virtual memory
/// limits.
///
/// We track all opened (read or write) segments. These segments increment an
/// access counter every time they are accessed. Every time a segment needs to
/// be opened, the tracker is called (`open_read` or `open_write`) to track the
/// new segment. A read segment gives us a strong reference to its mmap file.
/// The segment itself only holds a weak reference, which renders the segment
/// tracker the sole owner (except for momentary accesses).
///
/// When opening a new segment, if too many segments are open, we sort the
/// opened segments by access counts since last check, sort them, and then
/// close segments that have been accessed the least.
#[derive(Clone)]
pub struct SegmentTracker {
    inner: Arc<Mutex<Inner>>,
}

impl SegmentTracker {
    pub fn new(max_open: usize) -> SegmentTracker {
        SegmentTracker {
            inner: Arc::new(Mutex::new(Inner {
                next_id: 0,
                max_open,
                opened: HashMap::new(),
            })),
        }
    }

    pub fn register(&self) -> RegisteredSegment {
        let mut inner = self.inner.lock().unwrap();

        let id = inner.next_id;
        inner.next_id += 1;

        RegisteredSegment {
            id,
            access_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn open_write(&self, segment: &RegisteredSegment) {
        let mut inner = self.inner.lock().unwrap();
        inner.opened.insert(
            segment.id,
            TrackedSegment {
                access_count_live: segment.access_count.clone(),
                access_count_last: 0,
                state: OpenState::Write,
            },
        );

        if inner.opened.len() > inner.max_open {
            inner.close_least_used(segment.id);
        } else {
            inner.save_access_count();
        }
    }

    pub fn open_read(&self, segment: &RegisteredSegment, mmap: Arc<memmap2::Mmap>) {
        let mut inner = self.inner.lock().unwrap();
        inner.opened.insert(
            segment.id,
            TrackedSegment {
                access_count_live: segment.access_count.clone(),
                access_count_last: 0,
                state: OpenState::Read(mmap),
            },
        );

        if inner.opened.len() > inner.max_open {
            inner.close_least_used(segment.id);
        } else {
            inner.save_access_count();
        }
    }

    pub fn close(&self, segment: &RegisteredSegment) {
        let mut inner = self.inner.lock().unwrap();
        inner.opened.remove(&segment.id);
    }
}

#[derive(Default)]
struct Inner {
    next_id: usize,
    max_open: usize,
    opened: HashMap<SegmentId, TrackedSegment>,
}

impl Inner {
    fn close_least_used(&mut self, last_segment: SegmentId) {
        struct SegmentStats {
            index: SegmentId,
            access_count: usize,
            write: bool,
        }

        let mut segment_stats = Vec::new();
        for (index, segment) in &mut self.opened {
            segment_stats.push(SegmentStats {
                index: *index,
                access_count: segment.delta_access_count(),
                write: segment.state.is_write(),
            })
        }

        segment_stats.sort_by_key(|seg| seg.access_count);

        let to_close = segment_stats.len() - self.max_open;
        let mut closed = 0;
        for segment in segment_stats {
            if segment.index == last_segment || segment.write {
                continue;
            }

            self.opened.remove(&segment.index);
            closed += 1;
            if closed >= to_close {
                break;
            }
        }
    }

    fn save_access_count(&mut self) {
        for segment in self.opened.values_mut() {
            segment.access_count_last = segment.access_count_live.load(Ordering::Relaxed);
        }
    }
}

struct TrackedSegment {
    access_count_live: Arc<AtomicUsize>,
    access_count_last: usize,
    state: OpenState,
}

enum OpenState {
    Read(Arc<memmap2::Mmap>),
    Write,
}

impl OpenState {
    fn is_write(&self) -> bool {
        matches!(self, &OpenState::Write)
    }
}

impl TrackedSegment {
    fn delta_access_count(&mut self) -> usize {
        let access_count_live = self.access_count_live.load(Ordering::Relaxed);
        let access_count_last = self.access_count_last;

        self.access_count_last = access_count_live;

        if access_count_live >= access_count_last {
            access_count_live - access_count_last
        } else {
            // counter rolled over
            (usize::MAX - access_count_last) + access_count_live
        }
    }
}

#[derive(Clone)]
pub struct RegisteredSegment {
    id: SegmentId,
    access_count: Arc<AtomicUsize>,
}
impl RegisteredSegment {
    pub fn access(&self) {
        self.access_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn simple_case() {
        let tracker = SegmentTracker::new(2);

        let segment1 = tracker.register();
        let segment2 = tracker.register();
        let segment3 = tracker.register();

        let segment2_file = tempfile::tempfile().unwrap();
        let segment3_file = tempfile::tempfile().unwrap();

        tracker.open_write(&segment1);
        tracker.open_read(&segment2, mmap(&segment2_file));
        tracker.open_read(&segment3, mmap(&segment3_file));

        { 
            // should have dropped segment 2 since 1 is write and segment 3 is latest added
            let inner = tracker.inner.lock().unwrap();
            assert_eq!(inner.opened.len(), 2);
            assert!(inner.opened.contains_key(&segment1.id));
            assert!(inner.opened.contains_key(&segment3.id));
        }
    }

    #[test]
    fn sort_access_count() {
        let tracker = SegmentTracker::new(2);

        let segment1 = tracker.register();
        let segment2 = tracker.register();
        let segment3 = tracker.register();

        let segment1_file = tempfile::tempfile().unwrap();
        let segment2_file = tempfile::tempfile().unwrap();
        let segment3_file = tempfile::tempfile().unwrap();

        tracker.open_read(&segment1, mmap(&segment1_file));
        tracker.open_read(&segment2, mmap(&segment2_file));

        segment1.access(); 

        tracker.open_read(&segment3, mmap(&segment3_file));

        { 
            // should drop segment 2 since segment 1 got access more, and segment 3 just got added
            let inner = tracker.inner.lock().unwrap();
            assert_eq!(inner.opened.len(), 2);
            assert!(inner.opened.contains_key(&segment1.id));
            assert!(inner.opened.contains_key(&segment3.id));
        }
    }

    #[test]
    fn force_close() {
        let tracker = SegmentTracker::new(2);

        let segment1 = tracker.register();
        tracker.open_write(&segment1);
        tracker.close(&segment1);

        { 
            let inner = tracker.inner.lock().unwrap();
            assert_eq!(inner.opened.len(), 0);
        }
    }

    fn mmap(file: &File) -> Arc<memmap2::Mmap> {
        unsafe { Arc::new(memmap2::MmapOptions::new().len(1).map(file).unwrap())}
    }
}
