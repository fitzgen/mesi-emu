//! Memory cache implementation.

extern crate lru_time_cache;
use self::lru_time_cache::LruCache;

use std::mem;
use std::sync::mpsc;
use std::thread;

use bus;
use main_memory;

/// The number of blocks a cache can hold.
pub const CACHE_SIZE: usize = main_memory::BLOCK_SIZE;

/// The number of caches to simulate.
pub const NUMBER_OF_CACHES: usize = 8;

/// The current MESI state of a cache line.
///
/// Descriptions of individual states quoted [from
/// Wikipedia](https://en.wikipedia.org/wiki/MESI_protocol).
///
/// <pre>
///   M E S I
/// M ✗ ✗ ✗ ✓
/// E ✗ ✗ ✗ ✓
/// S ✗ ✗ ✓ ✓
/// I ✓ ✓ ✓ ✓
/// </pre>
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MesiState {
    /// "The cache line is present only in the current cache, and is dirty; it
    /// has been modified from the value in main memory. The cache is required
    /// to write the data back to main memory at some time in the future, before
    /// permitting any other read of the (no longer valid) main memory
    /// state. The write-back changes the line to the Shared state."
    Modified,

    /// "The cache line is present only in the current cache, but is clean; it
    /// matches main memory. It may be changed to the Shared state at any time,
    /// in response to a read request. Alternatively, it may be changed to the
    /// Modified state when writing to it."
    Exclusive,

    /// "Indicates that this cache line may be stored in other caches of the
    /// machine and is clean; it matches the main memory. The line may be
    /// discarded (changed to the Invalid state) at any time."
    Shared,

    /// "Indicates that this cache line is invalid (unused)."
    Invalid,
}

/// The id of a memory cache.
pub type MemoryCacheId = u8;

/// A cache line is a block of data and its associated MESI state.
#[derive(Clone, Copy)]
pub struct CacheLine {
    state: MesiState,
    data: [u8; main_memory::BLOCK_SIZE],
}

impl CacheLine {
    /// Read a byte from the data in this cache line.
    pub fn read_byte(&self, addr: main_memory::Address) -> u8 {
        assert!(self.state != MesiState::Invalid);
        self.data[addr.0 % main_memory::BLOCK_SIZE]
    }

    /// Write a byte to the data in this cache line.
    pub fn write_byte(&mut self, addr: main_memory::Address, val: u8) {
        assert!(self.state == MesiState::Modified);
        self.data[addr.0 % main_memory::BLOCK_SIZE] = val;
    }
}

/// A memory cache.
pub struct MemoryCache {
    /// This cache's unique id.
    pub id: MemoryCacheId,
    miss_count: f64,
    total_count: f64,
    to_bus: mpsc::Sender<bus::BusMessage>,
    from_bus: mpsc::Receiver<bus::BusMessage>,
    cached_lines: LruCache<main_memory::Block, Box<CacheLine>>,
}

impl MemoryCache {
    /// Spawn a MemoryCache thread that uses `accessor` to simulate data access
    /// patterns.
    pub fn spawn<F>(id: MemoryCacheId,
                    bus: mpsc::Sender<bus::BusMessage>,
                    accessor: F)
                    -> (mpsc::Sender<bus::BusMessage>, thread::JoinHandle<()>)
        where F: 'static + Send + FnOnce(MemoryCache)
    {
        let (send, recv) = mpsc::channel();

        let th = thread::Builder::new().name(format!("Memory cache {}", id));
        let handle = th.spawn(move || {
            accessor(MemoryCache {
                id: id,
                miss_count: 0.0,
                total_count: 0.0,
                to_bus: bus,
                from_bus: recv,
                cached_lines: LruCache::with_capacity(CACHE_SIZE),
            });
        });

        (send, handle.expect("Error spawning thread"))
    }

    /// Return the percent of reads and writes that have missed the cache.
    pub fn miss_percent(&self) -> f64 {
        assert!(self.miss_count <= self.total_count);
        (self.miss_count / self.total_count) * 100.0
    }

    /// Reset the statistics recording miss percents.
    pub fn reset_stats(&mut self) {
        self.miss_count = 0.0;
        self.total_count = 0.0;
    }

    /// Empty the cache and write back any modified cache lines that might be
    /// stored.
    pub fn empty(&mut self) {
        self.flush();
        mem::replace(&mut self.cached_lines, LruCache::with_capacity(CACHE_SIZE));
    }

    /// Flush the cache. Writes each `MesiState::Modified` cache line back to
    /// main memory.
    pub fn flush(&mut self) {
        let modified = self.cached_lines.retrieve_all().into_iter()
            .filter(|&(_, ref c)| c.state == MesiState::Modified);

        for (block, cache_line) in modified {
            self.to_bus.send(bus::BusMessage::WriteRequest {
                block: block,
                data: cache_line.data,
            }).expect("Error sending to bus from memory cache");

            self.cached_lines.remove(&block);
        }
    }

    /// Flush the cache if adding a new cache line would drop another cache line
    /// from the cache.
    fn maybe_flush(&mut self) {
        if self.cached_lines.len() == CACHE_SIZE {
            self.flush();
        }
    }

    fn handle_bus_message(&mut self, msg: &bus::BusMessage) {
        match *msg {
            // Snoop on other caches' requests.

            bus::BusMessage::ReadRequest { who, block }
            if who != self.id => {
                if let Some(cache_line) = self.cached_lines.get_mut(&block) {
                    cache_line.state = match cache_line.state {
                        MesiState::Invalid => MesiState::Invalid,
                        MesiState::Exclusive | MesiState::Shared => {
                            self.to_bus.send(bus::BusMessage::ReadResponse {
                                who: who,
                                from: bus::ResponseSender::Cache,
                                block: block,
                                data: Some(cache_line.data),
                            }).expect("Error sending to bus from memory cache");
                            MesiState::Shared
                        },
                        MesiState::Modified => {
                            self.to_bus.send(bus::BusMessage::WriteRequest {
                                block: block,
                                data: cache_line.data,
                            }).expect("Error sending to bus from memory cache");

                            self.to_bus.send(bus::BusMessage::ReadResponse {
                                who: who,
                                from: bus::ResponseSender::Cache,
                                block: block,
                                data: Some(cache_line.data),
                            }).expect("Error sending to bus from memory cache");

                            MesiState::Shared
                        },
                    }
                }
            },

            bus::BusMessage::ReadExclusiveRequest { who, block }
            if who != self.id => {
                if let Some(cache_line) = self.cached_lines.get_mut(&block) {
                    if cache_line.state == MesiState::Modified {
                        self.to_bus.send(bus::BusMessage::WriteRequest {
                            block: block,
                            data: cache_line.data,
                        }).expect("Error sending to bus from memory cache");
                    }

                    cache_line.state = MesiState::Invalid;
                }
            },

            // Handle responses to our own requests.

            bus::BusMessage::ReadResponse { who, from, block, data }
            if who == self.id && data.is_some() => {
                if let Some(cached) = self.cached_lines.get_mut(&block) {
                    if cached.state != MesiState::Invalid {
                        // We already got a response from a snooping cache.
                        assert!(from == bus::ResponseSender::MainMemory);
                        return;
                    }
                }

                self.maybe_flush();
                self.cached_lines.insert(block, Box::new(CacheLine {
                    state: match from {
                        bus::ResponseSender::MainMemory => MesiState::Exclusive,
                        bus::ResponseSender::Cache => MesiState::Shared,
                    },
                    data: data.unwrap(),
                }));
            },

            bus::BusMessage::ReadExclusiveResponse { who, block, data }
            if who == self.id && data.is_some() => {
                self.maybe_flush();
                self.cached_lines.insert(block, Box::new(CacheLine {
                    state: MesiState::Modified,
                    data: data.unwrap(),
                }));
            },

            // Snoop when other caches start reading cache lines that we have
            // marked exclusive and set our local copy's state to shared.
            bus::BusMessage::ReadResponse { who, from: _, block, data }
            if who != self.id && data.is_some() => {
                if let Some(cache_line) = self.cached_lines.get_mut(&block) {
                    if cache_line.state == MesiState::Exclusive {
                        cache_line.state = MesiState::Shared;
                    }
                }
            },

            // Ignore our own requests.
            bus::BusMessage::ReadRequest { who, block: _ } |
            bus::BusMessage::ReadExclusiveRequest { who, block: _ } => {
                assert!(who == self.id);
            },

            // Ignore responses that aren't meant for us.
            bus::BusMessage::ReadResponse { who, from: _, block: _, data } |
            bus::BusMessage::ReadExclusiveResponse { who, block: _, data } => {
                assert!(who != self.id || data.is_none());
            },

            // Ignore writes, they are only for main memory.
            bus::BusMessage::WriteRequest { block: _, data: _ } => { },
        }
    }

    /// Handle the backlog of unprocessed bus messages.
    fn snoop_backlog(&mut self) {
        while let Ok(msg) = self.from_bus.try_recv() {
            self.handle_bus_message(&msg);
        }
    }

    /// Keep snooping bus messages until `when` returns true.
    fn snoop_until<F>(&mut self, when: F) where F: Fn(&bus::BusMessage) -> bool {
        loop {
            let msg = self.from_bus.recv().expect("Error receiving bus message");

            self.handle_bus_message(&msg);

            if when(&msg) {
                return;
            }
        }
    }

    /// Read the byte at the given address.
    pub fn read(&mut self, addr: main_memory::Address) -> u8 {
        self.total_count += 1.0;
        self.snoop_backlog();

        let target_block = main_memory::Block::for_addr(addr);

        if let Some(cache_line) = self.cached_lines.get(&target_block) {
            if cache_line.state != MesiState::Invalid {
                return cache_line.read_byte(addr);
            }
        }

        self.miss_count += 1.0;

        loop {
            self.to_bus.send(bus::BusMessage::ReadRequest {
                who: self.id,
                block: target_block,
            }).expect("Error sending to bus from memory cache");

            let self_id = self.id;
            self.snoop_until(|msg| match *msg {
                bus::BusMessage::ReadResponse { who, from: _, block, data: _ } => {
                    who == self_id && block == target_block
                },
                _ => false
            });

            if let Some(cache_line) = self.cached_lines.get(&target_block) {
                if cache_line.state != MesiState::Invalid {
                    return cache_line.read_byte(addr);
                }
            }

            // If we didn't get the cache line successfully, then another cache
            // must have it in the Modified state. They will have snooped our
            // read request and issued a write to main memory in response, so
            // keep retrying the read request.
        }
    }

    /// Write the `value` to the given address.
    pub fn write(&mut self, address: main_memory::Address, value: u8) {
        self.total_count += 1.0;
        self.snoop_backlog();

        let target_block = main_memory::Block::for_addr(address);

        if let Some(cache_line) = self.cached_lines.get_mut(&target_block) {
            match cache_line.state {
                MesiState::Modified | MesiState::Exclusive => {
                    cache_line.state = MesiState::Modified;
                    cache_line.write_byte(address, value);
                    return;
                },
                MesiState::Shared => {
                    // TODO FITZGEN: actually invalidate everyone else's copy of
                    // this cache line, verify that it invalidated alright, and
                    // then continue as now. If invalidation fails, then we need
                    // to consider that a cache miss and continue with the main
                    // memory logic.

                    self.to_bus.send(bus::BusMessage::ReadExclusiveRequest {
                        who: self.id,
                        block: target_block,
                    }).expect("Error sending to bus from memory cache");

                    cache_line.state = MesiState::Modified;
                    cache_line.write_byte(address, value);
                    return;
                },
                MesiState::Invalid => { },
            }
        }

        self.miss_count += 1.0;

        loop {
            self.to_bus.send(bus::BusMessage::ReadExclusiveRequest {
                who: self.id,
                block: target_block,
            }).expect("Error sending message to bus from memory cache");

            let self_id = self.id;
            self.snoop_until(|msg| match *msg {
                bus::BusMessage::ReadExclusiveResponse { who, block, data: _ } => {
                    who == self_id && block == target_block
                },
                _ => false
            });

            if let Some(cache_line) = self.cached_lines.get_mut(&target_block) {
                if cache_line.state == MesiState::Modified {
                    cache_line.write_byte(address, value);
                    return;
                }
            }

            // If we didn't get the cache line successfully, then another cache
            // must have it in the Modified state. They will have snooped our
            // read for exclusive access request and issued a write to main
            // memory in response, so keep retrying the request.
        }
    }
}
