use std::time::Duration;

use anyhow::anyhow;
use wasmtime::*;

fn main() -> Result<(), anyhow::Error> {
    let engine = Engine::default();
    let store = Store::new(&engine);

    let mut linker = Linker::new(&store);
    linker.func(
        "exocore",
        "__exocore_host_log",
        |caller: Caller<'_>, ptr: i32, len: i32| {
            println!("WASM: {}", read_wasm_string(caller, ptr, len)?);
            Ok(())
        },
    )?;
    linker.func(
        "exocore",
        "__exocore_host_now",
        |_caller: Caller<'_>| -> u64 { unix_timestamp() },
    )?;

    let module = Module::from_file(&engine, "hello.wasm")?;
    let instance = linker.instantiate(&module)?;

    // Initialize environment
    let exocore_init = instance
        .get_func("__exocore_init")
        .expect("`__exocore_init` was not an exported function. Did you include SDK?");
    let exocore_init = exocore_init.get0::<()>()?;
    exocore_init()?;

    // Create application instance
    let exocore_app_init = instance.get_func("__exocore_app_init").expect(
        "`__exocore_app_init` was not an exported function. Did you implement #[exocore_app]?",
    );
    let exocore_app_init = exocore_app_init.get0::<()>()?;
    exocore_app_init()?;

    // Boot the application
    let exocore_app_boot = instance.get_func("__exocore_app_boot").expect(
        "`__exocore_app_boot` was not an exported function. Did you implement #[exocore_app]?",
    );
    let exocore_app_boot = exocore_app_boot.get0::<()>()?;
    exocore_app_boot()?;

    // We start ticking
    let exocore_tick = instance
        .get_func("__exocore_tick")
        .expect("`__exocore_tick` was not an exported function. Did you implement #[exocore_app]?");
    let exocore_tick = exocore_tick.get0::<u64>()?;
    loop {
        let next_tick_time = exocore_tick().expect("Couldn't tick");
        let now = unix_timestamp();

        if next_tick_time == 0 {
            std::thread::sleep(Duration::from_secs(1));
        } else if next_tick_time > now {
            std::thread::sleep(Duration::from_nanos(next_tick_time - now));
        };
    }

    // let send_resp = instance
    //     .get_func("send_resp")
    //     .expect("`send_resp` was not an exported function");
    // let data_ptr = wasm_alloc(&instance, b"hello")?;
    // let send_resp = send_resp.get2::<i32, i32, ()>()?;
    // send_resp(data_ptr, 5)?;
    // wasm_free(&instance, data_ptr, 5)?;

    // let print_hello = instance
    //     .get_func("print_hello")
    //     .expect("`print_hello` was not an exported function");
    // let print_hello = print_hello.get0::<()>()?;
    // print_hello()?;

    // Ok(())
}

/// Reads a string from a wasm pointer and len.
///
/// Mostly copied from wasmtime::Func comments.
fn read_wasm_string(caller: Caller<'_>, ptr: i32, len: i32) -> Result<String, Trap> {
    let mem = match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return Err(Trap::new("failed to find host memory")),
    };

    unsafe {
        let data = mem
            .data_unchecked()
            .get(ptr as u32 as usize..)
            .and_then(|arr| arr.get(..len as u32 as usize));
        let string = match data {
            Some(data) => match std::str::from_utf8(data) {
                Ok(s) => s,
                Err(_) => return Err(Trap::new("invalid utf-8")),
            },
            None => return Err(Trap::new("pointer/length out of bounds")),
        };

        Ok(string.into())
    }
}

// Inspired from https://radu-matei.com/blog/practical-guide-to-wasm-memory/#passing-arrays-to-modules-using-wasmtime
fn wasm_alloc(instance: &Instance, bytes: &[u8]) -> Result<i32, anyhow::Error> {
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

    Ok(guest_ptr_offset)
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
