use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use exocore_protos::apps::{InMessage, MessageStatus, OutMessage};
use exocore_protos::prost::{self, ProstMessageExt};
use log::Level;
use wasmtime::*;

#[derive(Clone)]
pub struct AppRuntime<E: HostEnvironment> {
    instance: Instance,
    func_send_message: Func,
    func_tick: Func,
    env: Arc<E>,
}

impl<E: HostEnvironment> AppRuntime<E> {
    pub fn from_file<P>(file: P, env: Arc<E>) -> Result<AppRuntime<E>, AppRuntimeError>
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

    pub fn run(&self) -> Result<(), AppRuntimeError> {
        loop {
            let next_tick_dur = self.tick()?;
            if let Some(next_tick_dur) = next_tick_dur {
                std::thread::sleep(next_tick_dur);
            };
        }
    }

    pub fn tick(&self) -> Result<Option<Duration>, AppRuntimeError> {
        let exocore_tick = self.func_tick.get0::<u64>()?;
        let now = unix_timestamp();
        let next_tick_time = exocore_tick().expect("Couldn't tick");

        if next_tick_time > now {
            Ok(Some(Duration::from_nanos(next_tick_time - now)))
        } else {
            Ok(None)
        }
    }

    pub fn send_message(&self, message: InMessage) -> Result<(), AppRuntimeError> {
        let message_bytes = message.encode_to_vec();

        let func = self.func_send_message.get2::<i32, i32, u32>()?;
        let (message_ptr, message_size) = wasm_alloc(&self.instance, &message_bytes)?;
        let res = func(message_ptr, message_size);
        wasm_free(&self.instance, message_ptr, message_size)?;

        match MessageStatus::from_i32(res? as i32) {
            Some(MessageStatus::Ok) => {}
            other => return Err(AppRuntimeError::MessageStatus(other)),
        }

        Ok(())
    }

    fn setup_host_module(linker: &mut Linker, env: &Arc<E>) -> Result<(), AppRuntimeError> {
        let env_clone = env.clone();
        linker.func(
            "exocore",
            "__exocore_host_log",
            move |caller: Caller<'_>, level: i32, ptr: i32, len: i32| {
                let log_level = match level {
                    1 => Level::Error,
                    2 => Level::Warn,
                    3 => Level::Info,
                    4 => Level::Debug,
                    5 => Level::Trace,
                    _ => Level::Error,
                };

                read_wasm_str(caller, ptr, len, |msg| {
                    env_clone.handle_log(log_level, msg);
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

pub trait HostEnvironment: Send + Sync + 'static {
    fn handle_message(&self, msg: OutMessage);
    fn handle_log(&self, level: log::Level, msg: &str);
}

#[derive(Debug, thiserror::Error)]
pub enum AppRuntimeError {
    #[error("The application is missing function '{0}'. Did you include SDK and implement #[exocore_app]?")]
    MissingFunction(&'static str),

    #[error("WASM runtime error '{0}'")]
    Runtime(&'static str),

    #[error("WASM execution aborted: {0}")]
    Trap(#[from] Trap),

    #[error("Message handling error: status={0:?}")]
    MessageStatus(Option<MessageStatus>),

    #[error("Message decoding error: {0}")]
    MessageDecode(#[from] exocore_protos::prost::DecodeError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<AppRuntimeError> for Trap {
    fn from(err: AppRuntimeError) -> Self {
        match err {
            AppRuntimeError::Trap(t) => t,
            other => Trap::new(other.to_string()),
        }
    }
}

fn bootstrap_module(instance: &Instance) -> Result<(Func, Func), AppRuntimeError> {
    // Initialize environment
    let exocore_init = instance
        .get_func("__exocore_init")
        .ok_or(AppRuntimeError::MissingFunction("__exocore_init"))?;
    let exocore_init = exocore_init.get0::<()>()?;
    exocore_init()?;

    // Create application instance
    let exocore_app_init = instance
        .get_func("__exocore_app_init")
        .ok_or(AppRuntimeError::MissingFunction("__exocore_app_init"))?;
    let exocore_app_init = exocore_app_init.get0::<()>()?;
    exocore_app_init()?;

    // Boot the application
    let exocore_app_boot = instance
        .get_func("__exocore_app_boot")
        .ok_or(AppRuntimeError::MissingFunction("__exocore_app_boot"))?;
    let exocore_app_boot = exocore_app_boot.get0::<()>()?;
    exocore_app_boot()?;

    // Extract tick & message sending functions
    let exocore_tick = instance
        .get_func("__exocore_tick")
        .ok_or(AppRuntimeError::MissingFunction("__exocore_tick"))?;
    let exocore_send_message = instance
        .get_func("__exocore_in_message")
        .ok_or(AppRuntimeError::MissingFunction("__exocore_in_message"))?;

    Ok((exocore_tick, exocore_send_message))
}

/// Reads a bytes from a wasm pointer and len.
///
/// Mostly copied from wasmtime::Func comments.
fn read_wasm_message<M: prost::Message + Default>(
    caller: Caller<'_>,
    ptr: i32,
    len: i32,
) -> Result<M, AppRuntimeError> {
    let mem = match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return Err(AppRuntimeError::Runtime("failed to find host memory")),
    };

    unsafe {
        let data = mem
            .data_unchecked()
            .get(ptr as u32 as usize..)
            .and_then(|arr| arr.get(..len as u32 as usize));

        match data {
            Some(data) => Ok(M::decode(data)?),
            None => Err(AppRuntimeError::Runtime("pointer/length out of bounds")),
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
) -> Result<(), AppRuntimeError> {
    let mem = match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return Err(AppRuntimeError::Runtime("failed to find host memory")),
    };

    unsafe {
        let data = mem
            .data_unchecked()
            .get(ptr as u32 as usize..)
            .and_then(|arr| arr.get(..len as u32 as usize));
        match data {
            Some(data) => match std::str::from_utf8(data) {
                Ok(s) => f(s),
                Err(_) => return Err(AppRuntimeError::Runtime("invalid utf-8")),
            },
            None => return Err(AppRuntimeError::Runtime("pointer/length out of bounds")),
        };
    }

    Ok(())
}

// Inspired from https://radu-matei.com/blog/practical-guide-to-wasm-memory/#passing-arrays-to-modules-using-wasmtime
fn wasm_alloc(instance: &Instance, bytes: &[u8]) -> Result<(i32, i32), AppRuntimeError> {
    let mem = match instance.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return Err(AppRuntimeError::Runtime("failed to find host memory")),
    };

    let alloc = instance
        .get_func("__exocore_alloc")
        .ok_or(AppRuntimeError::MissingFunction("__exocore_alloc"))?;
    let alloc_result = alloc.call(&[Val::from(bytes.len() as i32)])?;

    let guest_ptr_offset = match alloc_result
        .get(0)
        .expect("expected the result of the allocation to have one value")
    {
        Val::I32(val) => *val,
        _ => {
            return Err(AppRuntimeError::Runtime(
                "__exocore_alloc returned a non-i32 pointer",
            ))
        }
    };

    unsafe {
        let raw = mem.data_ptr().offset(guest_ptr_offset as isize);
        raw.copy_from(bytes.as_ptr(), bytes.len());
    }

    Ok((guest_ptr_offset, bytes.len() as i32))
}

fn wasm_free(instance: &Instance, ptr: i32, size: i32) -> Result<(), AppRuntimeError> {
    let alloc = instance
        .get_func("__exocore_free")
        .ok_or(AppRuntimeError::MissingFunction("__exocore_free"))?;
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
        let next_tick_duration = app
            .tick()
            .unwrap()
            .unwrap_or_else(|| Duration::from_nanos(0));
        let last_log = env.last_log().unwrap();
        assert!(last_log.contains("before sleep"));

        // wait for next tick duration
        assert!(next_tick_duration > Duration::from_millis(10));
        sleep(next_tick_duration);

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

    impl HostEnvironment for TestEnv {
        fn handle_message(&self, msg: OutMessage) {
            let mut messages = self.messages.lock().unwrap();
            messages.push(msg);
        }

        fn handle_log(&self, level: log::Level, msg: &str) {
            log!(level, "WASM APP: {}", msg);
            let mut logs = self.logs.lock().unwrap();
            logs.push(msg.to_string());
        }
    }
}
