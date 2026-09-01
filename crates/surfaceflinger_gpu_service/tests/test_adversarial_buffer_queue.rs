//! Adversarial stress test suite for GraphicBufferQueue in `surfaceflinger_gpu_service`.
//!
//! Tests:
//! 1. 16-slot buffer exhaustion and `NoFreeSlots` error state.
//! 2. Dynamic texture reallocation churn and resolution resizing.
//! 3. Out-of-order buffer queueing and latest texture view acquisition.
//! 4. 8-thread concurrent producer mutation stress.
//! 5. Error handling on invalid slot index, un-dequeued slot queuing, and buffer size mismatch.
//! 6. AIDL IPC transaction serialization across `IGraphicBufferProducer` codes.

use aidl_compat::{Parcel, PING_TRANSACTION};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use surfaceflinger_gpu_service::{
    igraphicbufferproducer_codes, BufferQueueError, GraphicBufferProducerService,
};

/// Helper initializing a headless WGPU device and queue for testing.
async fn create_test_wgpu() -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("Failed to find suitable WGPU adapter");

    let mut required_features = wgpu::Features::empty();
    if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        required_features |= wgpu::Features::TIMESTAMP_QUERY;
    }

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Adversarial BufferQueue Test Device"),
                required_features,
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
        .await
        .expect("Failed to create WGPU test device");

    (Arc::new(device), Arc::new(queue))
}

#[test]
fn test_16_slot_buffer_exhaustion_and_reclamation() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let producer = GraphicBufferProducerService::new(2001, device, queue);

        producer.connect().unwrap();

        // 1. Dequeue all 16 slots
        let mut slots = Vec::new();
        for _ in 0..16 {
            let slot = producer.dequeue_buffer(64, 64, 1).unwrap();
            assert!(slot >= 0 && slot < 16);
            assert!(!slots.contains(&slot));
            slots.push(slot);
        }

        // 2. 17th dequeue must fail with NoFreeSlots
        let err = producer.dequeue_buffer(64, 64, 1).unwrap_err();
        assert_eq!(err, BufferQueueError::NoFreeSlots);

        // 3. Cancel slot 7 -> slot 7 becomes free
        producer.cancel_buffer(slots[7]);
        let reclaimed = producer.dequeue_buffer(64, 64, 1).unwrap();
        assert_eq!(reclaimed, slots[7]);

        // 4. Queue slot 7 -> acquire view
        producer.queue_buffer_color(reclaimed, [255, 0, 0, 255], 64, 64).unwrap();
        let view = producer.acquire_latest_texture_view();
        assert!(view.is_some());

        // 5. Release/cancel remaining slots
        for &s in &slots {
            if s != reclaimed {
                producer.cancel_buffer(s);
            }
        }
    });
}

#[test]
fn test_buffer_reallocation_churn_and_dimension_resizing() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let producer = GraphicBufferProducerService::new(2002, device, queue);
        producer.connect().unwrap();

        let slot = producer.dequeue_buffer(32, 32, 1).unwrap();
        producer.queue_buffer_color(slot, [0, 255, 0, 255], 32, 32).unwrap();
        assert!(producer.acquire_latest_texture_view().is_some());

        // Re-dequeue at 512x512
        let slot2 = producer.dequeue_buffer(512, 512, 1).unwrap();
        let large_data = vec![200u8; 512 * 512 * 4];
        producer.queue_buffer_data(slot2, &large_data, 512, 512).unwrap();
        assert!(producer.acquire_latest_texture_view().is_some());

        // Shrink to 8x8
        let slot3 = producer.dequeue_buffer(8, 8, 1).unwrap();
        producer.queue_buffer_color(slot3, [0, 0, 255, 255], 8, 8).unwrap();
        assert!(producer.acquire_latest_texture_view().is_some());
    });
}

#[test]
fn test_out_of_order_queueing_and_acquisition() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let producer = GraphicBufferProducerService::new(2003, device, queue);
        producer.connect().unwrap();

        let s0 = producer.dequeue_buffer(64, 64, 1).unwrap();
        let s1 = producer.dequeue_buffer(64, 64, 1).unwrap();
        let s2 = producer.dequeue_buffer(64, 64, 1).unwrap();

        // Queue out of order: s2, then s0, then s1
        producer.queue_buffer_color(s2, [10, 20, 30, 255], 64, 64).unwrap();
        producer.queue_buffer_color(s0, [40, 50, 60, 255], 64, 64).unwrap();
        producer.queue_buffer_color(s1, [70, 80, 90, 255], 64, 64).unwrap();

        // Latest acquisition should be s1
        let view = producer.acquire_latest_texture_view();
        assert!(view.is_some());
        let data = producer.get_latest_buffer_data();
        assert!(data.is_some());
        let (buf, w, h) = data.unwrap();
        assert_eq!(w, 64);
        assert_eq!(h, 64);
        assert_eq!(buf[0], 70);
        assert_eq!(buf[1], 80);
        assert_eq!(buf[2], 90);
    });
}

#[test]
fn test_concurrent_multithreaded_producer_stress() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let producer = Arc::new(GraphicBufferProducerService::new(2004, device, queue));
        producer.connect().unwrap();

        let num_threads = 8;
        let iterations = 100;
        let error_count = Arc::new(AtomicUsize::new(0));
        let stop_flag = Arc::new(AtomicBool::new(false));

        let mut handles = Vec::new();
        for t in 0..num_threads {
            let prod = Arc::clone(&producer);
            let errs = Arc::clone(&error_count);

            let h = thread::spawn(move || {
                for i in 0..iterations {
                    match prod.dequeue_buffer(32, 32, 1) {
                        Ok(slot) => {
                            if i % 3 == 0 {
                                prod.cancel_buffer(slot);
                            } else {
                                let color = [t as u8 * 30, i as u8, 128, 255];
                                if let Err(_) = prod.queue_buffer_color(slot, color, 32, 32) {
                                    errs.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }
                        Err(BufferQueueError::NoFreeSlots) => {
                            // Contention under load is expected, brief sleep and retry
                            thread::sleep(Duration::from_micros(200));
                        }
                        Err(_) => {
                            errs.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }
            });
            handles.push(h);
        }

        // Consumer thread acquiring textures
        let prod_consumer = Arc::clone(&producer);
        let stop_consumer = Arc::clone(&stop_flag);
        let consumer_h = thread::spawn(move || {
            let mut acquired = 0;
            while !stop_consumer.load(Ordering::Relaxed) {
                if let Some(_) = prod_consumer.acquire_latest_texture_view() {
                    acquired += 1;
                }
                thread::sleep(Duration::from_micros(500));
            }
            acquired
        });

        for h in handles {
            h.join().unwrap();
        }

        stop_flag.store(true, Ordering::Relaxed);
        let total_acquired = consumer_h.join().unwrap();
        println!("Concurrent test finished. Total textures acquired: {}", total_acquired);
        assert_eq!(error_count.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn test_error_resilience_and_invalid_operations() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let producer = GraphicBufferProducerService::new(2005, device, queue);

        // Invalid slot index
        let err_neg = producer.queue_buffer_color(-1, [0, 0, 0, 255], 32, 32).unwrap_err();
        assert_eq!(err_neg, BufferQueueError::InvalidSlot(-1));

        let err_overflow = producer.queue_buffer_color(999, [0, 0, 0, 255], 32, 32).unwrap_err();
        assert_eq!(err_overflow, BufferQueueError::InvalidSlot(999));

        // Queueing non-dequeued slot
        let err_in_use = producer.queue_buffer_color(0, [0, 0, 0, 255], 32, 32).unwrap_err();
        assert_eq!(err_in_use, BufferQueueError::SlotInUse(0));

        // Dimension mismatch on queue_buffer_data
        let slot = producer.dequeue_buffer(64, 64, 1).unwrap();
        let short_data = vec![0u8; 10]; // Requires 64 * 64 * 4 = 16384 bytes
        let err_dim = producer.queue_buffer_data(slot, &short_data, 64, 64).unwrap_err();
        assert_eq!(err_dim, BufferQueueError::DimensionMismatch);

        producer.cancel_buffer(slot);
    });
}

#[test]
fn test_aidl_ipc_producer_transact() {
    use aidl_compat::IBinder;

    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let producer = GraphicBufferProducerService::new(2006, device, queue);

        // PING
        let mut ping_reply = Parcel::new();
        assert!(producer.transact(PING_TRANSACTION, 0, &Parcel::new(), &mut ping_reply).is_ok());

        // CONNECT (9)
        let mut conn_reply = Parcel::new();
        producer.transact(igraphicbufferproducer_codes::CONNECT, 0, &Parcel::new(), &mut conn_reply).unwrap();
        let mut off = 0;
        assert!(conn_reply.read_status(&mut off).unwrap().is_ok());

        // SET_BUFFER_COUNT (2)
        let mut sbc_data = Parcel::new();
        sbc_data.write_i32(4).unwrap();
        let mut sbc_reply = Parcel::new();
        producer.transact(igraphicbufferproducer_codes::SET_BUFFER_COUNT, 0, &sbc_data, &mut sbc_reply).unwrap();
        off = 0;
        assert!(sbc_reply.read_status(&mut off).unwrap().is_ok());

        // ALLOCATE_BUFFERS (12)
        let mut alloc_data = Parcel::new();
        alloc_data.write_u32(128).unwrap();
        alloc_data.write_u32(128).unwrap();
        alloc_data.write_u32(1).unwrap();
        let mut alloc_reply = Parcel::new();
        producer.transact(igraphicbufferproducer_codes::ALLOCATE_BUFFERS, 0, &alloc_data, &mut alloc_reply).unwrap();
        off = 0;
        assert!(alloc_reply.read_status(&mut off).unwrap().is_ok());

        // DEQUEUE_BUFFER (3)
        let mut deq_data = Parcel::new();
        deq_data.write_u32(128).unwrap();
        deq_data.write_u32(128).unwrap();
        deq_data.write_u32(1).unwrap();
        let mut deq_reply = Parcel::new();
        producer.transact(igraphicbufferproducer_codes::DEQUEUE_BUFFER, 0, &deq_data, &mut deq_reply).unwrap();
        off = 0;
        assert!(deq_reply.read_status(&mut off).unwrap().is_ok());
        let slot = deq_reply.read_i32(&mut off).unwrap();
        assert!(slot >= 0);

        // QUEUE_BUFFER (6)
        let mut q_data = Parcel::new();
        q_data.write_i32(slot).unwrap();
        q_data.write_u32(128).unwrap();
        q_data.write_u32(128).unwrap();
        let pixels = vec![255u8; 128 * 128 * 4];
        q_data.write_byte_slice(Some(&pixels)).unwrap();
        let mut q_reply = Parcel::new();
        producer.transact(igraphicbufferproducer_codes::QUEUE_BUFFER, 0, &q_data, &mut q_reply).unwrap();
        off = 0;
        assert!(q_reply.read_status(&mut off).unwrap().is_ok());

        // DISCONNECT (10)
        let mut disc_reply = Parcel::new();
        producer.transact(igraphicbufferproducer_codes::DISCONNECT, 0, &Parcel::new(), &mut disc_reply).unwrap();
        off = 0;
        assert!(disc_reply.read_status(&mut off).unwrap().is_ok());
    });
}
