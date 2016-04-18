//! Main memory implementation.

extern crate bit_vec;

use std::ops;
use std::sync::mpsc;
use std::thread;

use bus;

pub const BLOCK_SIZE: usize = 32;
pub const MAIN_MEMORY_SIZE: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Address(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Block(pub usize);

impl Block {
    pub fn for_addr(addr: Address) -> Block {
        Block(addr.0 / BLOCK_SIZE)
    }

    pub fn address_range(&self) -> ops::Range<usize> {
        ops::Range {
            start: self.0 * BLOCK_SIZE,
            end: (self.0 + 1) * BLOCK_SIZE,
        }
    }
}

/// The main memory.
pub struct MainMemory {
    to_bus: mpsc::Sender<bus::BusMessage>,
    from_bus: mpsc::Receiver<bus::BusMessage>,
    modified: bit_vec::BitVec,
    data: [u8; MAIN_MEMORY_SIZE]
}

impl MainMemory {
    /// Create the main memory in its own thread.
    pub fn spawn(bus: mpsc::Sender<bus::BusMessage>) -> mpsc::Sender<bus::BusMessage> {
        let (send, recv) = mpsc::channel();

        thread::spawn(move || {
            let memory = Box::new(MainMemory {
                to_bus: bus,
                from_bus: recv,
                modified: bit_vec::BitVec::from_elem(MAIN_MEMORY_SIZE / BLOCK_SIZE, false),
                data: [0; MAIN_MEMORY_SIZE],
            });

            memory.run();
        });

        send
    }

    /// Run the main loop of the main memory thread. Serves up responses to
    /// requests to read and write memory.
    pub fn run(mut self) {
        for msg in self.from_bus {
            // Simulate how main memory is an order of magnitude slower than
            // cache with a 100,000 ns sleep.
            thread::sleep(::std::time::Duration::new(0, 100_000));

            match msg {
                bus::BusMessage::ReadRequest { who, block } => {
                    let data = if self.modified.get(block.0).unwrap_or(false) {
                        None
                    } else {
                        let mut data = [0 as u8; BLOCK_SIZE];
                        data.clone_from_slice(&self.data[block.address_range()]);
                        Some(data)
                    };

                    self.to_bus.send(bus::BusMessage::ReadResponse {
                        who: who,
                        block: block,
                        data: data,
                    }).expect("Error sending to bus from main memory");
                },

                bus::BusMessage::ReadExclusiveRequest { who, block } => {
                    let data = if self.modified.get(block.0).unwrap_or(false) {
                        None
                    } else {
                        self.modified.set(block.0, true);
                        let mut data = [0 as u8; BLOCK_SIZE];
                        data.clone_from_slice(&self.data[block.address_range()]);
                        Some(data)
                    };

                    self.to_bus.send(bus::BusMessage::ReadExclusiveResponse {
                        who: who,
                        block: block,
                        data: data,
                    }).expect("Error sending to bus from main memory");
                },

                bus::BusMessage::WriteRequest { block, data } => {
                    self.modified.set(block.0, false);
                    self.data[block.address_range()].clone_from_slice(&data);
                },

                // Ignored.
                bus::BusMessage::ReadResponse { who: _, block: _, data: _ } => { },
                bus::BusMessage::ReadExclusiveResponse { who: _, block: _, data: _ } => { },
            }
        }
    }
}
