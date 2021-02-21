use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use exocore_protos::apps::{InMessage, MessageStatus, OutMessage};
use exocore_protos::prost::{self, ProstMessageExt};
use wasmtime::*;

#[derive(Clone)]
pub struct AppRuntime<E: Environment> {
    instance: Instance,
    func_send_message: Func,
    func_tick: Func,
    env: Arc<E>,
}

pub trait Environment: Send + Sync + 'static {
    fn handle_message(&self, msg: OutMessage);
    fn handle_log(&self, msg: &str);
}

impl<E: Environment> AppRuntime<E> {
    pub fn from_file<P>(file: P, env: Arc<E>) -> anyhow::Result<AppRuntime<E>>
    where
        P: AsRef<Path>,
    {
        let engine = Engine::default();
        let store = Store::new(&engine);

        let mut linker = Linker::new(&store);
        Self::setup_host_module(&mut linker, &env)?;

        let module = Module::from_file(&engine, file)?;
        let instance = linker.instantiate(&module)?;

        let (func_tick, func_send_message) = bootstrap_module(&instance)?;

        Ok(AppRuntime {
            instance,
            func_tick,
            func_send_message,
            env,
        })
    }

    pub fn run(&self) -> anyhow::Result<()> {
        loop {
            let next_tick_time = self.tick()?;
            let now = unix_timestamp();

            if next_tick_time == 0 {
                std::thread::sleep(Duration::from_secs(1));
            } else if next_tick_time > now {
                std::thread::sleep(Duration::from_nanos(next_tick_time - now));
            };
        }
    }

    pub fn tick(&self) -> anyhow::Result<u64> {
        let exocore_tick = self.func_tick.get0::<u64>()?;
        Ok(exocore_tick().expect("Couldn't tick"))
    }

    pub fn send_message(&self, message: InMessage) -> anyhow::Result<()> {
        let message_bytes = message.encode_to_vec();

        let func = self.func_send_message.get2::<i32, i32, u32>()?;
        let (message_ptr, message_size) = wasm_alloc(&self.instance, &message_bytes)?;
        let res = func(message_ptr, message_size);
        wasm_free(&self.instance, message_ptr, message_size)?;

        let status = MessageStatus::from_i32(res? as i32);
        // TODO: Handle error

        Ok(())
    }

    fn setup_host_module(linker: &mut Linker, env: &Arc<E>) -> anyhow::Result<()> {
        let env_clone = env.clone();
        linker.func(
            "exocore",
            "__exocore_host_log",
            move |caller: Caller<'_>, level: i32, ptr: i32, len: i32| {
                // TODO: Handle level

                read_wasm_str(caller, ptr, len, |msg| {
                    env_clone.handle_log(msg);
                })?;

                Ok(())
            },
        )?;

        linker.func(
            "exocore",
            "__exocore_host_now",
            |_caller: Caller<'_>| -> u64 { unix_timestamp() },
        )?;

        let env = env.clone();
        linker.func(
            "exocore",
            "__exocore_host_out_message",
            move |caller: Caller<'_>, ptr: i32, len: i32| -> u32 {
                let status = match read_wasm_message::<OutMessage>(caller, ptr, len) {
                    Ok(msg) => {
                        env.as_ref().handle_message(msg);
                        MessageStatus::Ok
                    }
                    Err(err) => {
                        error!("Couldn't decode message sent from application: {}", err);
                        MessageStatus::DecodeError
                    }
                };

                status as u32
            },
        )?;

        Ok(())
    }
}

fn bootstrap_module(instance: &Instance) -> anyhow::Result<(Func, Func)> {
    // TODO: Convert expects to Error

    // Initialize environment
    let exocore_init = instance
        .get_func("__exocore_init")
        .expect("`__exocore_init` was not an exported function. Did you include the SDK?");
    let exocore_init = exocore_init.get0::<()>()?;
    exocore_init()?;

    // Create application instance
    let exocore_app_init = instance.get_func("__exocore_app_init").expect(
        "`__exocore_app_init` was not an exported function. Did you implement #[exocore_app]?",
    );
    let exocore_app_init = exocore_app_init.get0::<()>()?;
    exocore_app_init()?;

    // Boot the application
    let exocore_app_boot = instance
        .get_func("__exocore_app_boot")
        .expect("`__exocore_app_boot` was not an exported function. Did you include the SDK?");
    let exocore_app_boot = exocore_app_boot.get0::<()>()?;
    exocore_app_boot()?;

    // Extract tick & message sending functions
    let exocore_tick = instance
        .get_func("__exocore_tick")
        .expect("`__exocore_tick` was not an exported function. Did you include the SDK?");
    let exocore_send_message = instance
        .get_func("__exocore_in_message")
        .expect("`__exocore_in_message` was not an exported function. Did you include the SDK?");

    Ok((exocore_tick, exocore_send_message))
}

/// Reads a bytes from a wasm pointer and len.
///
/// Mostly copied from wasmtime::Func comments.
fn read_wasm_message<M: prost::Message + Default>(
    caller: Caller<'_>,
    ptr: i32,
    len: i32,
) -> Result<M, Trap> {
    let mem = match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return Err(Trap::new("failed to find host memory")),
    };

    unsafe {
        let data = mem
            .data_unchecked()
            .get(ptr as u32 as usize..)
            .and_then(|arr| arr.get(..len as u32 as usize));

        match data {
            Some(data) => {
                M::decode(data).map_err(|err| Trap::new("couldn't read message"))
                // TODO: Better error handling
            }
            None => Err(Trap::new("pointer/length out of bounds")),
        }
    }
}

/// Reads a str from a wasm pointer and len.
///
/// Mostly copied from wasmtime::Func comments.
fn read_wasm_str<F: FnOnce(&str)>(
    caller: Caller<'_>,
    ptr: i32,
    len: i32,
    f: F,
) -> Result<(), Trap> {
    let mem = match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return Err(Trap::new("failed to find host memory")),
    };

    unsafe {
        let data = mem
            .data_unchecked()
            .get(ptr as u32 as usize..)
            .and_then(|arr| arr.get(..len as u32 as usize));
        match data {
            Some(data) => match std::str::from_utf8(data) {
                Ok(s) => f(s),
                Err(_) => return Err(Trap::new("invalid utf-8")),
            },
            None => return Err(Trap::new("pointer/length out of bounds")),
        };
    }

    Ok(())
}

// Inspired from https://radu-matei.com/blog/practical-guide-to-wasm-memory/#passing-arrays-to-modules-using-wasmtime
fn wasm_alloc(instance: &Instance, bytes: &[u8]) -> Result<(i32, i32), anyhow::Error> {
    let mem = match instance.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return Err(anyhow!("failed to find host memory")),
    };

    let alloc = instance
        .get_func("__exocore_alloc")
        .expect("expected alloc function not found");
    let alloc_result = alloc.call(&[Val::from(bytes.len() as i32)])?;

    let guest_ptr_offset = match alloc_result
        .get(0)
        .expect("expected the result of the allocation to have one value")
    {
        Val::I32(val) => *val,
        _ => return Err(anyhow!("guest pointer must be Val::I32")),
    };

    unsafe {
        let raw = mem.data_ptr().offset(guest_ptr_offset as isize);
        raw.copy_from(bytes.as_ptr(), bytes.len());
    }

    Ok((guest_ptr_offset, bytes.len() as i32))
}

fn wasm_free(instance: &Instance, ptr: i32, size: i32) -> Result<(), anyhow::Error> {
    let alloc = instance
        .get_func("__exocore_free")
        .expect("expected alloc function not found");
    alloc.call(&[Val::from(ptr), Val::from(size)])?;

    Ok(())
}

fn unix_timestamp() -> u64 {
    // TODO: Should be consistent time
    let now = std::time::SystemTime::now();
    now.duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use exocore_core::tests_utils::find_test_fixture;
    use exocore_protos::apps::in_message::InMessageType;
    use exocore_protos::apps::out_message::OutMessageType;
    use exocore_protos::store::{EntityResults, MutationResult};
    use std::sync::Mutex;
    use std::thread::sleep;

    use super::*;

    /// Runs the application defined in `exocore-apps-example`. See its `lib.rs` to follow the sequence.
    #[test]
    fn example_golden_path() {
        exocore_core::logging::setup(None);
        let example_path = find_test_fixture("fixtures/example.wasm");
        let env = Arc::new(TestEnv::new());

        let app = AppRuntime::from_file(example_path, env.clone()).unwrap();

        // first tick should execute up to sleep
        app.tick().unwrap();
        assert!(env.find_log("application initialized").is_some());
        assert!(env.find_log("task starting").is_some());

        // should now be sleeping for 100ms
        let last_log = env.last_log().unwrap();
        assert!(last_log.contains("before sleep"));
        let time_before_sleep = last_log
            .replace("before sleep ", "")
            .parse::<u64>()
            .unwrap();

        // ticking right away shouldn't do anything since app is sleeping for 100ms
        app.tick().unwrap();
        let last_log = env.last_log().unwrap();
        assert!(last_log.contains("before sleep"));

        // wait 100ms
        sleep(Duration::from_millis(100));

        // ticking after sleep time should now wake up and continue
        app.tick().unwrap();
        let after_sleep_log = env.find_log("after sleep").unwrap();
        let time_after_sleep = after_sleep_log
            .replace("after sleep ", "")
            .parse::<u64>()
            .unwrap();
        assert!((time_after_sleep - time_before_sleep) > 100_000_000); // 100ms in nano

        // should have sent mutation request to host
        let message = env.pop_message().unwrap();
        assert_eq!(message.r#type, OutMessageType::StoreMutationRequest as i32);

        // reply with mutation result, should report that mutation has succeed
        app.send_message(InMessage {
            r#type: InMessageType::StoreMutationResult.into(),
            rendez_vous_id: message.rendez_vous_id,
            data: MutationResult::default().encode_to_vec(),
        })
        .unwrap();
        app.tick().unwrap();
        assert!(env.find_log("mutation success").is_some());

        // should have sent query to host
        let message = env.pop_message().unwrap();
        assert_eq!(message.r#type, OutMessageType::StoreEntityQuery as i32);

        // reply with query result, should report that query has succeed
        app.send_message(InMessage {
            r#type: InMessageType::StoreEntityResults.into(),
            rendez_vous_id: message.rendez_vous_id,
            data: EntityResults::default().encode_to_vec(),
        })
        .unwrap();
        app.tick().unwrap();
        assert!(env.find_log("query success").is_some());

        // should have completed
        assert_eq!(env.last_log(), Some("task done".to_string()));
    }

    struct TestEnv {
        logs: Mutex<Vec<String>>,
        messages: Mutex<Vec<OutMessage>>,
    }

    impl TestEnv {
        fn new() -> TestEnv {
            TestEnv {
                logs: Mutex::new(Vec::new()),
                messages: Mutex::new(Vec::new()),
            }
        }

        fn find_log(&self, needle: &str) -> Option<String> {
            let logs = self.logs.lock().unwrap();
            for log in logs.iter() {
                if log.contains(needle) {
                    return Some(log.clone());
                }
            }

            None
        }

        fn last_log(&self) -> Option<String> {
            let logs = self.logs.lock().unwrap();
            logs.last().cloned()
        }

        fn pop_message(&self) -> Option<OutMessage> {
            let mut msgs = self.messages.lock().unwrap();
            msgs.pop()
        }
    }

    impl Environment for TestEnv {
        fn handle_message(&self, msg: OutMessage) {
            let mut messages = self.messages.lock().unwrap();
            messages.push(msg);
        }

        fn handle_log(&self, msg: &str) {
            info!("Got log from app: {}", msg);
            let mut logs = self.logs.lock().unwrap();
            logs.push(msg.to_string());
        }
    }
}
