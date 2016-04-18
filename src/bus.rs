//! The bus connects each memory cache to each other and main memory. It
//! forwards messages sent from one of these actors to all the others.

use std::sync::mpsc;
use std::thread;

use main_memory;
use memory_cache;

#[inline(always)]
fn ignore<T>(_: T) { }

/// The various types of messages we can send on the bus.
#[derive(Clone, Copy, Debug)]
pub enum BusMessage {
    /// A request to read a block from main memory.
    ReadRequest {
        /// Which memory cache is requesting the read.
        who: memory_cache::MemoryCacheId,
        /// Which block of memory.
        block: main_memory::Block,
    },

    /// The response to a `ReadRequest`.
    ReadResponse {
        /// Which memory cache the response is for.
        who: memory_cache::MemoryCacheId,
        /// Who sent the response.
        from: ResponseSender,
        /// Which block of memory.
        block: main_memory::Block,
        /// The block's data. If `None`, the data is unavailable due to another
        /// cache holding it exclusively for writing.
        data: Option<[u8; main_memory::BLOCK_SIZE]>,
    },

    /// A request to exclusively read a block from main memory, with intent to
    /// modify and write it back.
    ReadExclusiveRequest {
        /// Which memory cache is requesting the exclusive read.
        who: memory_cache::MemoryCacheId,
        /// Which block of memory.
        block: main_memory::Block,
    },

    /// The response to a `ReadExclusiveRequest`.
    ReadExclusiveResponse {
        /// Which memory cache the response is for.
        who: memory_cache::MemoryCacheId,
        /// Which block of memory.
        block: main_memory::Block,
        /// The block's data. If `None`, the data is unavailable due to another
        /// cache holding it exclusively for writing.
        data: Option<[u8; main_memory::BLOCK_SIZE]>,
    },

    /// A request to write a block back to main memory.
    WriteRequest {
        /// Which block of memory.
        block: main_memory::Block,
        /// The data to be written to the block.
        data: [u8; main_memory::BLOCK_SIZE],
    },
}

/// Who sent a response.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResponseSender {
    /// The response was sent by a snooping memory cache.
    Cache,
    /// The response was sent by main memory.
    MainMemory,
}

/// The bus that connects the memory caches to main memory and each other.
pub struct Bus {
    incoming: mpsc::Receiver<BusMessage>,
    outgoing: Vec<mpsc::Sender<BusMessage>>,
}

impl Bus {
    /// Create the bus, in its own thread.
    pub fn spawn(incoming: mpsc::Receiver<BusMessage>, outgoing: Vec<mpsc::Sender<BusMessage>>)
    {
        let bus = Bus {
            incoming: incoming,
            outgoing: outgoing,
        };

        thread::spawn(move || bus.run());
    }

    /// Run the bus' main loop, which forwards messages to each memory cache and
    /// main memory.
    pub fn run(mut self) {
        for msg in self.incoming {
            for out in &mut self.outgoing {
                ignore(out.send(msg.clone()));
            }
        }
    }
}
