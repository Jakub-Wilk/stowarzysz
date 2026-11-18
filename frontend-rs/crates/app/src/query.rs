//! Server state without a cache (the React app's `staleTime: 0, gcTime: 0` rule): a query
//! refetches when it mounts, when one of its topics is bumped (SSE events, see `realtime`), when
//! the app comes back to the foreground, and after `ready`/`resync` on the event stream.

use std::future::Future;

use api::ApiError;
use dioxus::prelude::*;

/// What a query depends on; SSE events bump the topics they touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Polls,
    Pacts,
    Ledger,
    Santa,
    People,
    Me,
    Birthdays,
}

impl Topic {
    pub const ALL: [Self; 7] = [
        Self::Polls,
        Self::Pacts,
        Self::Ledger,
        Self::Santa,
        Self::People,
        Self::Me,
        Self::Birthdays,
    ];

    fn index(self) -> usize {
        self as usize
    }
}

/// One generation counter per topic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefetchBus {
    generations: Signal<[u64; Topic::ALL.len()]>,
}

impl RefetchBus {
    pub fn bump(mut self, topics: &[Topic]) {
        let mut generations = self.generations.write();
        for topic in topics {
            generations[topic.index()] += 1;
        }
    }

    pub fn bump_all(self) {
        self.bump(&Topic::ALL);
    }

    /// Read (and so subscribe to) the topics' generations.
    pub fn track(self, topics: &[Topic]) -> u64 {
        let generations = self.generations.read();
        topics.iter().map(|t| generations[t.index()]).sum()
    }
}

pub fn use_refetch_bus_provider() -> RefetchBus {
    use_context_provider(|| RefetchBus {
        generations: Signal::new([0; Topic::ALL.len()]),
    })
}

pub fn use_refetch_bus() -> RefetchBus {
    use_context()
}

/// `use_resource` that also reruns whenever one of `topics` is bumped.
pub fn use_query<T, F, Fut>(topics: &'static [Topic], mut fetch: F) -> Resource<Result<T, ApiError>>
where
    T: 'static,
    F: FnMut() -> Fut + 'static,
    Fut: Future<Output = Result<T, ApiError>> + 'static,
{
    let bus = use_refetch_bus();
    use_resource(move || {
        bus.track(topics);
        fetch()
    })
}
