extern crate chrono;

extern crate rand;
use self::rand::distributions::IndependentSample;

use std::mem;
use std::sync::atomic;

use main_memory;
use memory_cache;

static EPOCH: atomic::AtomicUsize = atomic::ATOMIC_USIZE_INIT;

fn synchronize_phase(cache: &mut memory_cache::MemoryCache, timer: &mut chrono::DateTime<chrono::UTC>,
                     phase: &mut usize, phase_name: &str) {
    assert!(*phase > 0);

    cache.flush();

    let start_of_this_phase = (*phase - 1) * memory_cache::NUMBER_OF_CACHES;
    let start_of_next_phase = *phase * memory_cache::NUMBER_OF_CACHES;
    let epoch = EPOCH.load(atomic::Ordering::SeqCst);
    assert!(start_of_this_phase <= epoch && epoch < start_of_next_phase,
            "epoch in correct phase: {} <= {} < {}", start_of_this_phase, epoch, start_of_next_phase);

    if EPOCH.fetch_add(1, atomic::Ordering::SeqCst) != start_of_next_phase - 1 {
        loop {
            let epoch = EPOCH.load(atomic::Ordering::SeqCst);
            if epoch >= start_of_next_phase {
                break;
            }
        }
    }

    if cache.id == 0 {
        let now = chrono::UTC::now();
        println!("{}:\n\t{} ms\n\t{:.*} % cache miss\n", phase_name,
                 (now - *timer).num_milliseconds(),
                 3, cache.miss_percent());
        cache.reset_stats();
        mem::replace(timer, now);
    }

    // Continue on to the next phase!
    cache.empty();
    *phase += 1;
}

// TODO FITZGEN
pub fn benchmark(mut cache: memory_cache::MemoryCache) {
    let mut timer = chrono::UTC::now();
    let mut phase = 1;
    let id = cache.id;

    // Read every byte in memory sequentially.

    for i in 0..main_memory::MAIN_MEMORY_SIZE {
        cache.read(main_memory::Address(i));
    }

    synchronize_phase(&mut cache, &mut timer, &mut phase, "Sequential Read");

    // Write to every byte in memory sequentially.

    for i in 0..main_memory::MAIN_MEMORY_SIZE {
        cache.write(main_memory::Address(i), id);
    }

    synchronize_phase(&mut cache, &mut timer, &mut phase, "Sequential Write");

    // // Read MAIN_MEMORY_SIZE random bytes.

    // let memory_range = rand::distributions::Range::new(0, main_memory::MAIN_MEMORY_SIZE);
    // let mut rng = rand::thread_rng();

    // for _ in 0..main_memory::MAIN_MEMORY_SIZE {
    //     let addr = main_memory::Address(memory_range.ind_sample(&mut rng));
    //     cache.read(addr);
    // }

    // synchronize_phase(&mut cache, &mut timer, &mut phase, "Random Read");

    // // Write MAIN_MEMORY_SIZE random bytes.

    // for _ in 0..main_memory::MAIN_MEMORY_SIZE {
    //     let addr = main_memory::Address(memory_range.ind_sample(&mut rng));
    //     cache.write(addr, id);
    // }

    // synchronize_phase(&mut cache, &mut timer, &mut phase, "Random Write");

    // Read a thread-unique chunk of bytes sequentially and repeatedly, for a
    // total of MAIN_MEMORY_SIZE reads.

    let chunk_size = memory_cache::CACHE_SIZE * main_memory::BLOCK_SIZE;
    let unique_chunk_offset = id as usize * chunk_size;

    for i in 0..main_memory::MAIN_MEMORY_SIZE {
        let addr = main_memory::Address(unique_chunk_offset + (i % chunk_size));
        cache.read(addr);
    }

    synchronize_phase(&mut cache, &mut timer, &mut phase, "Thread-Unique Chunk Read");

    // Write a thread-unique chunk of bytes sequentially and repeatedly, for a
    // total of MAIN_MEMORY_SIZE writes.

    for i in 0..main_memory::MAIN_MEMORY_SIZE {
        let addr = main_memory::Address(unique_chunk_offset + (i % chunk_size));
        cache.write(addr, id);
    }

    synchronize_phase(&mut cache, &mut timer, &mut phase, "Thread-Unique Chunk Write");

    // Read the same chunk of bytes across all threads, sequentially and
    // repeatedly, for a total of MAIN_MEMORY_SIZE reads.

    for i in 0..main_memory::MAIN_MEMORY_SIZE {
        let addr = main_memory::Address(i % chunk_size);
        cache.read(addr);
    }

    synchronize_phase(&mut cache, &mut timer, &mut phase, "Shared Chunk Read");

    // Write the same chunk of bytes across all threads, sequentially and
    // repeatedly, for a total of MAIN_MEMORY_SIZE writes.

    for i in 0..main_memory::MAIN_MEMORY_SIZE {
        let addr = main_memory::Address(i % chunk_size);
        cache.write(addr, id);
    }

    synchronize_phase(&mut cache, &mut timer, &mut phase, "Shared Chunk Write");

    // Write the same chunk of bytes across all threads, sequentially and
    // repeatedly, for a total of MAIN_MEMORY_SIZE writes.

    for i in 0..main_memory::MAIN_MEMORY_SIZE {
        let addr = main_memory::Address((i * id as usize) % chunk_size);
        cache.write(addr, id);
    }

    synchronize_phase(&mut cache, &mut timer, &mut phase, "False-Sharing Chunk Write");

    // // Read the same address across all cache threads.

    // let addr = main_memory::Address(0);
    // for _ in 0..main_memory::MAIN_MEMORY_SIZE {
    //     cache.read(addr);
    // }

    // synchronize_phase(&mut cache, &mut timer, &mut phase, "Address(0) Read");

    // // Write the same address across all cache threads.

    // let addr = main_memory::Address(0);
    // for _ in 0..main_memory::MAIN_MEMORY_SIZE {
    //     cache.write(addr, id);
    // }

    // synchronize_phase(&mut cache, &mut timer, &mut phase, "Address(0) Write");

    // // Read different addresses on the same cache line.

    // let addr = main_memory::Address(id as usize);
    // for _ in 0..main_memory::MAIN_MEMORY_SIZE {
    //     cache.read(addr);
    // }

    // synchronize_phase(&mut cache, &mut timer, &mut phase, "Address(id) Read");

    // // Write different addresses on the same cache line.

    // let addr = main_memory::Address(id as usize);
    // for _ in 0..main_memory::MAIN_MEMORY_SIZE {
    //     cache.write(addr, id);
    // }

    // synchronize_phase(&mut cache, &mut timer, &mut phase, "Address(id) Write");

    // // Read on different cache lines.

    // let addr = main_memory::Address(id as usize * 100);
    // for _ in 0..main_memory::MAIN_MEMORY_SIZE {
    //     cache.read(addr);
    // }

    // synchronize_phase(&mut cache, &mut timer, &mut phase, "Address(id * BLOCK_SIZE) Read");

    // // Write on different cache lines.

    // let addr = main_memory::Address(id as usize * 100);
    // for _ in 0..main_memory::MAIN_MEMORY_SIZE {
    //     cache.write(addr, id);
    // }

    // synchronize_phase(&mut cache, &mut timer, &mut phase, "Address(id * BLOCK_SIZE) Write");
}
