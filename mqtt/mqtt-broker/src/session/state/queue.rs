use std::{
    collections::VecDeque,
    fmt::{Display, Formatter, Result as FmtResult},
    num::NonZeroUsize,
};

use mqtt3::proto;

use crate::settings::QueueFullAction;

/// `BoundedQueue` is a queue of publications with bounds by count and total payload size in bytes.
///
/// Packets will be queued until either `max_len` (max number of publications)
/// or `max_size` (max total payload size of publications)
/// is reached, and then `when_full` strategy is applied.
///
/// None for `max_len` or `max_size` means "unbounded".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoundedQueue {
    inner: VecDeque<proto::Publication>,
    max_len: Option<NonZeroUsize>,
    max_size: Option<NonZeroUsize>,
    when_full: QueueFullAction,
    current_size: usize,
}

impl BoundedQueue {
    pub fn new(
        max_len: Option<NonZeroUsize>,
        max_size: Option<NonZeroUsize>,
        when_full: QueueFullAction,
    ) -> Self {
        Self {
            inner: VecDeque::new(),
            max_len,
            max_size,
            when_full,
            current_size: 0,
        }
    }

    pub fn into_inner(self) -> VecDeque<proto::Publication> {
        self.inner
    }

    pub fn dequeue(&mut self) -> Option<proto::Publication> {
        match self.inner.pop_front() {
            Some(publication) => {
                self.current_size -= publication.payload.len();
                Some(publication)
            }
            None => None,
        }
    }

    pub fn enqueue(&mut self, publication: proto::Publication) -> Option<LimitReached> {
        if let Some(max_len) = self.max_len {
            if self.inner.len() >= max_len.get() {
                return self
                    .handle_queue_limit(publication)
                    .map(LimitReached::QueueLength);
            }
        }

        if let Some(max_size) = self.max_size {
            let pub_len = publication.payload.len();
            if self.current_size + pub_len > max_size.get() {
                return self
                    .handle_queue_limit(publication)
                    .map(LimitReached::Memory);
            }
        }

        self.current_size += publication.payload.len();
        self.inner.push_back(publication);
        None
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[cfg(test)]
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, proto::Publication> {
        self.inner.iter()
    }

    fn handle_queue_limit(
        &mut self,
        publication: proto::Publication,
    ) -> Option<proto::Publication> {
        match self.when_full {
            QueueFullAction::DropNew => Some(publication),
            QueueFullAction::DropOld => {
                let dequed = self.dequeue();
                self.current_size += publication.payload.len();
                self.inner.push_back(publication);

                dequed
            }
        }
    }
}

impl Extend<proto::Publication> for BoundedQueue {
    fn extend<T: IntoIterator<Item = proto::Publication>>(&mut self, iter: T) {
        iter.into_iter().for_each(|item| {
            let _ = self.enqueue(item);
        });
    }
}

#[derive(Debug)]
pub enum LimitReached {
    Memory(proto::Publication),
    QueueLength(proto::Publication),
}

impl LimitReached {
    pub fn publication(&self) -> &proto::Publication {
        match self {
            Self::Memory(publication) => publication,
            Self::QueueLength(publication) => publication,
        }
    }
}

impl Display for LimitReached {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Memory(_) => write!(f, "out of memory limits"),
            Self::QueueLength(_) => write!(f, "out of queue length limits"),
        }
    }
}
