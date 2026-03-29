use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Default)]
pub struct PriorityQueue<T> {
    buckets: BTreeMap<i32, VecDeque<T>>,
}

impl<T> PriorityQueue<T> {
    pub fn new() -> Self {
        Self {
            buckets: BTreeMap::new(),
        }
    }

    pub fn push(&mut self, priority: i32, item: T) {
        self.buckets.entry(priority).or_default().push_back(item);
    }

    pub fn pop(&mut self) -> Option<T> {
        let key = *self.buckets.keys().next_back()?;
        let bucket = self.buckets.get_mut(&key)?;
        let item = bucket.pop_front();
        if bucket.is_empty() {
            self.buckets.remove(&key);
        }
        item
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    pub fn len(&self) -> usize {
        self.buckets.values().map(VecDeque::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::PriorityQueue;

    #[test]
    fn preserves_priority_order() {
        let data = [
            (3, "Clear drains"),
            (6, "drink tea"),
            (5, "Make tea"),
            (4, "Feed cat"),
            (7, "eat biscuit"),
            (2, "Tax return"),
            (1, "Solve RC tasks"),
        ];

        let mut pq = PriorityQueue::new();
        for (priority, item) in data {
            pq.push(priority, item);
        }

        let mut results = Vec::new();
        while let Some(item) = pq.pop() {
            results.push(item);
        }

        assert_eq!(
            results,
            vec![
                "eat biscuit",
                "drink tea",
                "Make tea",
                "Feed cat",
                "Clear drains",
                "Tax return",
                "Solve RC tasks",
            ]
        );
    }
}
