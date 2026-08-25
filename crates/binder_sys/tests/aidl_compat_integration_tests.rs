//! Full AIDL Compatibility layer integration tests over direct `binder_sys`.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result, Status, STATUS_BAD_VALUE};
use aidl_compat::stub::{Binder, RemoteBinder};
use aidl_compat::traits::{Interface, Remotable};
use binder_rt::types::TransactionCode;
use binder_rt::Parcel;
use binder_sys::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub const TEST_AIDL_DESCRIPTOR: &str = "android.os.ITestAidlService";

pub trait ITestAidlService: Interface + Send + Sync {
    fn compute(&self, a: i32, b: i32) -> Result<i32>;
    fn concatenate(&self, s1: &str, s2: &str) -> Result<String>;
    fn transfer_bytes(&self, input: &[u8]) -> Result<Vec<u8>>;
}

pub struct TestAidlServiceImpl {
    calls: AtomicU32,
}

impl Interface for TestAidlServiceImpl {
    fn as_binder(&self) -> SpIBinder {
        Binder::new(Self {
            calls: AtomicU32::new(0),
        })
    }
}

impl Remotable for TestAidlServiceImpl {
    fn get_class_descriptor() -> &'static str {
        TEST_AIDL_DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut offset = 0;
        let _ = data.read_utf16(&mut offset);

        match code {
            1 => {
                let a = data.read_i32(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let b = data.read_i32(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_i32(a * b).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            2 => {
                let s1 = data.read_utf8(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?.unwrap_or_default();
                let s2 = data.read_utf8(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?.unwrap_or_default();
                let combined = format!("{}:{}", s1, s2);
                reply.write_utf8(Some(&combined)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            3 => {
                let bytes = data.read_byte_vec(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?.unwrap_or_default();
                let mut reversed = bytes;
                reversed.reverse();
                reply.write_byte_slice(Some(&reversed)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::status::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

#[test]
fn test_aidl_service_full_integration() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // Server process
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let service_impl = Arc::new(TestAidlServiceImpl {
        calls: AtomicU32::new(0),
    });
    let cookie = 0x7777;
    server_ps.register_service_object(cookie, Binder::new_with_arc(Arc::clone(&service_impl)));

    let server_ps_clone = Arc::clone(&server_ps);
    let _server_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(server_ps_clone);
        let _ = ts.enter_looper();
    });

    thread::sleep(Duration::from_millis(50));

    // Client process
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let handle = mock_driver.add_handle_for_client(
        &mock_driver.get_client(client_ps.pid()).unwrap(),
        server_ps.pid(),
        0,
        cookie,
    );

    let transport = Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps)));
    let proxy: SpIBinder = RemoteBinder::new_with_transport(handle, 0, Some(TEST_AIDL_DESCRIPTOR), transport);

    // 1. Ping
    assert!(proxy.ping_binder().is_ok());

    // 2. Compute (11 * 7 = 77)
    let mut data1 = Parcel::new();
    data1.write_utf16(Some(TEST_AIDL_DESCRIPTOR)).unwrap();
    data1.write_i32(11).unwrap();
    data1.write_i32(7).unwrap();
    let mut reply1 = Parcel::new();
    proxy.transact(1, 0, &data1, &mut reply1).unwrap();
    let mut off1 = 0;
    assert_eq!(reply1.read_i32(&mut off1).unwrap(), 77);

    // 3. String concatenate ("Android", "WebGPU")
    let mut data2 = Parcel::new();
    data2.write_utf16(Some(TEST_AIDL_DESCRIPTOR)).unwrap();
    data2.write_utf8(Some("Android")).unwrap();
    data2.write_utf8(Some("WebGPU")).unwrap();
    let mut reply2 = Parcel::new();
    proxy.transact(2, 0, &data2, &mut reply2).unwrap();
    let mut off2 = 0;
    assert_eq!(reply2.read_utf8(&mut off2).unwrap().unwrap(), "Android:WebGPU");

    // 4. Byte array reverse
    let mut data3 = Parcel::new();
    data3.write_utf16(Some(TEST_AIDL_DESCRIPTOR)).unwrap();
    data3.write_byte_slice(Some(&[10, 20, 30, 40, 50])).unwrap();
    let mut reply3 = Parcel::new();
    proxy.transact(3, 0, &data3, &mut reply3).unwrap();
    let mut off3 = 0;
    assert_eq!(reply3.read_byte_vec(&mut off3).unwrap().unwrap(), vec![50, 40, 30, 20, 10]);

    assert_eq!(service_impl.calls.load(Ordering::SeqCst), 3);
}
