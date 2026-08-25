//! Death notification callback and lifecycle tests.

use aidl_compat::death::{DeathCallback, DeathRecipient};
use binder_sys::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_death_notification_dispatch() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Target server process
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let server_pid = server_ps.pid();

    // 2. Client watcher process
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let handle = mock_driver.add_handle_for_client(
        &mock_driver.get_client(client_ps.pid()).unwrap(),
        server_pid,
        0,
        0x9999,
    );

    let death_counter = Arc::new(AtomicUsize::new(0));
    let dc_clone = Arc::clone(&death_counter);

    let recipient: Arc<dyn DeathRecipient> = Arc::new(DeathCallback(move || {
        dc_clone.fetch_add(1, Ordering::SeqCst);
    }));

    let cookie = 0x12345678;
    client_ps.register_death_recipient(cookie, Arc::clone(&recipient));

    // Register death notification in client's driver state
    let mut req = Vec::with_capacity(20);
    req.extend_from_slice(&BC_REQUEST_DEATH_NOTIFICATION.to_ne_bytes());
    let hc = BinderHandleCookie {
        handle,
        padding: 0,
        cookie,
    };
    req.extend_from_slice(bytemuck::bytes_of(&hc));

    let mut bwr = binder_write_read::new();
    bwr.write_buffer = req.as_ptr() as u64;
    bwr.write_size = req.len() as u64;
    mock_driver.write_read(&mock_driver.get_client(client_ps.pid()).unwrap(), &mut bwr).unwrap();

    // Start client looper thread to process incoming death notification
    let client_ps_clone = Arc::clone(&client_ps);
    let looper_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(client_ps_clone);
        // Process pending command (BR_DEAD_BINDER)
        let _ = ts.process_pending_commands();
    });

    thread::sleep(Duration::from_millis(50));

    // Trigger process death for server
    mock_driver.trigger_process_death(server_pid);

    let _ = looper_thread.join();

    assert_eq!(death_counter.load(Ordering::SeqCst), 1, "Death recipient callback must be called");
}
