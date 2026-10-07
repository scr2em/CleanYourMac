mod common;
use common::*;
use cym_core::ffi::*;
use serde_json::{json, Value};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    sync::mpsc,
    time::Duration,
};

unsafe fn call(engine: *const CymEngine, request: Value) -> Value {
    let request = CString::new(request.to_string()).unwrap();
    let response = cym_call(engine, request.as_ptr());
    let value = serde_json::from_str(CStr::from_ptr(response).to_str().unwrap()).unwrap();
    cym_string_free(response);
    value
}
extern "C" fn collect(context: *mut c_void, event: *const c_char) {
    let sender = unsafe { &*(context as *const mpsc::Sender<Value>) };
    let event = unsafe { CStr::from_ptr(event) }.to_str().unwrap();
    sender.send(serde_json::from_str(event).unwrap()).unwrap();
}

#[test]
fn c_abi_round_trips_requests_scans_and_errors() {
    let f = Fixture::new();
    f.write("app/package.json", "{}");
    f.write("app/node_modules/x/index.js", "x");
    let config = CString::new(json!({"journalPath": f.at("history.json")}).to_string()).unwrap();
    unsafe {
        assert_eq!(cym_abi_version(), ABI_VERSION);
        let engine = cym_engine_new(config.as_ptr());
        assert!(!engine.is_null());
        let modules = call(engine, json!({"method": "modules"}));
        assert_eq!(modules["ok"], true);
        assert_eq!(modules["result"].as_array().unwrap().len(), 14);
        assert_eq!(modules["result"][0]["usesRoots"], true);
        let unknown = call(engine, json!({"method": "nope"}));
        assert_eq!(unknown["ok"], false);
        let invalid = call(engine, json!({"method": "pathContains", "params": {}}));
        assert_eq!(invalid["ok"], false);
        assert_eq!(call(engine, json!({"method": "history"}))["result"], json!([]));

        let (sender, receiver) = mpsc::channel::<Value>();
        let request = CString::new(
            json!({"modules": ["node"], "context": {"roots": [f.path()]}}).to_string(),
        )
        .unwrap();
        let scan = cym_scan_start(
            engine,
            request.as_ptr(),
            collect,
            &sender as *const _ as *mut c_void,
        );
        assert!(!scan.is_null());
        // The engine may be released while the scan still runs.
        cym_engine_free(engine);
        let mut events = vec![];
        loop {
            let event = receiver.recv_timeout(Duration::from_secs(20)).unwrap();
            let done = event["type"] == "finished";
            events.push(event);
            if done {
                break;
            }
        }
        cym_scan_free(scan);
        let finding = events.iter().find(|e| e["type"] == "finding").unwrap();
        assert_eq!(finding["finding"]["moduleId"], "node");
        assert_eq!(finding["finding"]["resource"]["kind"], "file");
        assert!(events.iter().any(|e| e["type"] == "moduleFinished"));
        assert_eq!(events.last().unwrap()["cancelled"], false);
        assert!(receiver.recv_timeout(Duration::from_millis(200)).is_err());

        let bad = CString::new("not json").unwrap();
        assert!(cym_scan_start(std::ptr::null(), bad.as_ptr(), collect, std::ptr::null_mut()).is_null());
        let invalid_config = CString::new("{").unwrap();
        assert!(cym_engine_new(invalid_config.as_ptr()).is_null());
    }
}
