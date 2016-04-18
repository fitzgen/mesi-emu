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
    ReadRequest {
        who: memory_cache::MemoryCacheId,
        block: main_memory::Block,
    },
    ReadResponse {
        who: memory_cache::MemoryCacheId,
        block: main_memory::Block,
        data: Option<[u8; main_memory::BLOCK_SIZE]>,
    },

    ReadExclusiveRequest {
        who: memory_cache::MemoryCacheId,
        block: main_memory::Block,
    },
    ReadExclusiveResponse {
        who: memory_cache::MemoryCacheId,
        block: main_memory::Block,
        data: Option<[u8; main_memory::BLOCK_SIZE]>,
    },

    WriteRequest {
        block: main_memory::Block,
        data: [u8; main_memory::BLOCK_SIZE],
    },
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
