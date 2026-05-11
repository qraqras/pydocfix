/// A file-absolute UTF-8 byte range in Python source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ByteRange {
    start: usize,
    end: usize,
}

impl ByteRange {
    /// Create a byte range from inclusive `start` and exclusive `end` offsets.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Inclusive start byte offset.
    pub const fn start(self) -> usize {
        self.start
    }

    /// Exclusive end byte offset.
    pub const fn end(self) -> usize {
        self.end
    }
}
