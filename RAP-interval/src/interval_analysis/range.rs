use intervals::*;
use num_traits::{Bounded, Num, Zero};

const MIN: i64 = i64::MIN;
const MAX: i64 = i64::MAX;

#[derive(PartialEq, Debug)]
pub struct Domain {}

#[derive(PartialEq, Debug)]
pub enum RangeType {
    Unknown,
    Regular,
    Empty,
}
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct Range<T>
where
    T: bounds::Bound + Num + Bounded,
{
    pub rtype: RangeType,
    pub range: Interval<UserType, UserType>,
}

#[derive(Num, Bounded, Debug, PartialEq, Eq, Hash, Copy, Clone)]
enum UserType {
    Unknown,
    i32(i32),
    Empty,
}

impl<T> Range<T>
where
    T: bounds::Bound + Num + Bounded,
{
    // Default constructor
    fn test() {}
    pub fn new(value: T) -> Self {
        Self {
            rtype: RangeType::Regular,
            range: Interval::new_unchecked(value, value),
        }
    }
    pub fn default() -> Self {
        Self::new(T::zero())
    }
    // Parameterized constructor
    pub fn with_bounds(lb: T, ub: T, rtype: RangeType) -> Self {
        Self {
            rtype,
            range: Interval::new_unchecked(lb, ub),
        }
    }

    // Getter for lower bound
    pub fn get_lower(&self) -> T {
        self.range.lower().clone()
    }

    // Getter for upper bound
    pub fn get_upper(&self) -> T {
        self.range.upper().clone()
    }

    // Setter for lower bound
    pub fn set_lower(&mut self, newl: T) {
        self.range.set_lower(newl);
    }

    // Setter for upper bound
    pub fn set_upper(&mut self, newu: T) {
        self.range.set_upper(newu);
    }

    // Check if the range type is unknown
    pub fn is_unknown(&self) -> bool {
        self.rtype == RangeType::Unknown
    }

    // Set the range type to unknown
    pub fn set_unknown(&mut self) {
        self.rtype = RangeType::Unknown;
    }

    // Check if the range type is regular
    pub fn is_regular(&self) -> bool {
        self.rtype == RangeType::Regular
    }

    // Set the range type to regular
    pub fn set_regular(&mut self) {
        self.rtype = RangeType::Regular;
    }

    // Check if the range type is empty
    pub fn is_empty(&self) -> bool {
        self.rtype == RangeType::Empty
    }

    // Set the range type to empty
    pub fn set_empty(&mut self) {
        self.rtype = RangeType::Empty;
    }

    // Check if the range is the maximum range
    pub fn is_max_range(&self) -> bool {
        self.range.lower() == T::min_value() && self.range.upper() == T::max_value()
    }

    // Print the range
    pub fn print(&self) {
        println!("Range: [{} - {}]", self.get_lower(), self.get_upper());
    }

    // Arithmetic and bitwise operations (example for addition)
    pub fn add(&self, other: &Range<T>) -> Range<T> {
        let lower = self.get_lower() + other.get_lower();
        let upper = self.get_upper() + other.get_upper();
        Range::with_bounds(lower, upper, RangeType::Regular)
    }
}

// Implement the comparison operators
impl<T> PartialEq for Range<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.get_lower() == other.get_lower()
            && self.get_upper() == other.get_upper()
            && self.rtype == other.rtype
    }
}

impl<T> Eq for Range<T> where T: Eq {}
