#[deny(missing_docs)]

use std::sync::mpsc;

pub mod benchmark;
pub mod bus;
pub mod main_memory;
pub mod memory_cache;

/// Spawn main memory and caches, tie them together with the bus, and then run
/// the benchmark.
pub fn main() {
    let (to_bus, from_bus) = mpsc::channel();

    let mut outgoing = Vec::with_capacity(memory_cache::NUMBER_OF_CACHES + 1);
    outgoing.push(main_memory::MainMemory::spawn(to_bus.clone()));

    let mut handles = Vec::with_capacity(memory_cache::NUMBER_OF_CACHES);

    for id in 0..memory_cache::NUMBER_OF_CACHES {
        let id = id as memory_cache::MemoryCacheId;

        let (send, handle) = memory_cache::MemoryCache::spawn(id, to_bus.clone(), move |cache| {
            benchmark::benchmark(cache);
        });

        handles.push(handle);
        outgoing.push(send);
    }

    bus::Bus::spawn(from_bus, outgoing);

    for handle in handles {
        handle.join().expect("Could not join thread");
    }
}
