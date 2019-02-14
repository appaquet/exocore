use std;
use std::cmp::Ordering;
use std::ops::{Deref, DerefMut, Range};


#[derive(Clone, PartialEq, Eq, Hash)]
struct OrdRange<T>(Range<T>);

impl<T> Deref for OrdRange<T> {
    type Target = Range<T>;

    fn deref(&self) -> &Range<T> {
        &self.0
    }
}

impl<T> DerefMut for OrdRange<T> {
    fn deref_mut(&mut self) -> &mut Range<T> {
        &mut self.0
    }
}

impl<T: Ord + Copy + Eq> OrdRange<T> {
    #[inline]
    pub fn is_before(&self, other: &OrdRange<T>) -> bool {
        self.end <= other.start
    }

    #[inline]
    pub fn is_right_before(&self, other: &OrdRange<T>) -> bool {
        self.end == other.start
    }

    #[inline]
    pub fn is_after(&self, other: &OrdRange<T>) -> bool {
        other.end <= self.start
    }

    #[inline]
    pub fn is_right_after(&self, other: &OrdRange<T>) -> bool {
        other.end == self.start
    }
}

impl<T: Ord + Copy + Eq> PartialOrd for OrdRange<T> {
    fn partial_cmp(&self, other: &OrdRange<T>) -> Option<Ordering> {
        if self.start == other.start && self.end == other.end {
            Some(Ordering::Equal)
        } else if self.is_before(other) {
            Some(Ordering::Less)
        } else if self.is_after(other) {
            Some(Ordering::Greater)
        } else {
            None
        }
    }
}

impl<'a, T: Ord + Copy + Eq> Ord for OrdRange<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        if self == other {
            Ordering::Equal
        } else if self.is_before(other) {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }
}

pub fn are_continuous<'a, I, T: 'a + Ord + Copy + Eq>(iter: I) -> bool
where
    I: Iterator<Item = &'a Range<T>>,
{
    get_gaps(iter).is_empty()
}

pub fn get_gaps<'a, I, T: 'a + Ord + Copy + Eq>(iter: I) -> Vec<Range<T>>
where
    I: Iterator<Item = &'a Range<T>>,
{

    // FIXME : We need OrdRange at this point - find a way to convert this properly
    // error[E0277]: a collection of type `std::vec::Vec<&range::OrdRange<T>>` cannot be built from an iterator over elements of type `&'a std::ops::Range<T>`
    let mut sorted: Vec<OrdRange<T>> = iter.collect();
    sorted.sort();

    let mut gaps = Vec::new();
    for i in 1..sorted.len() {
        let left = sorted[i - 1];
        let right = sorted[i];
        if left.end != right.start {
            gaps.push(Range { start: left.end, end: right.start });
        }
    }

    gaps
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_find_gaps() {
        let a = Range { start: 10, end: 20 };
        let b = Range { start: 30, end: 40 };
        let c = Range { start: 40, end: 50 };
        let d = Range { start: 80, end: 100 };

        let ranges = vec![b, a, d, c];
        let gaps = get_gaps(ranges.iter());
        assert_eq!(gaps, vec![Range { start: 20, end: 30 }, Range { start: 50, end: 80 }]);
        assert!(!are_continuous(ranges.iter()));

        let ranges = vec![b, c];
        assert!(are_continuous(ranges.iter()));
    }
}
