#[derive(Debug)]
pub enum HeadBodyTail<T> {
    Only(T),
    Head(T),
    Body(T),
    Tail(T),
}

#[derive(Default)]
pub struct HeadBodyTailIter<T, I: Iterator<Item = T>> {
    next_item: Option<T>,
    first: bool,
    inner: I,
}

impl<T, I: Iterator<Item = T>> HeadBodyTailIter<T, I> {
    pub fn new(mut inner: I) -> Self {
        let next_item = inner.next();
        Self {
            inner,
            first: true,
            next_item,
        }
    }
}

impl<T, I: Iterator<Item = T>> Iterator for HeadBodyTailIter<T, I> {
    type Item = HeadBodyTail<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let current_item = self.next_item.take();
        if current_item.is_none() {
            return None;
        }
        self.next_item = self.inner.next();
        let result = match (self.next_item.is_some(), self.first) {
            (true, true) => current_item.map(HeadBodyTail::Head),
            (false, true) => current_item.map(HeadBodyTail::Only),
            (true, false) => current_item.map(HeadBodyTail::Body),
            (false, false) => current_item.map(HeadBodyTail::Tail),
        };
        self.first = false;
        result
    }
}
