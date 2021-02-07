use std::error::Error;
use wasmtime::*;

fn main() -> Result<(), Box<dyn Error>> {
    let engine = Engine::default();
    let store = Store::new(&engine);
    let module = Module::from_file(&engine, "hello.wasm")?;

    let log = Func::wrap(&store, |caller: Caller<'_>, ptr: i32, len: i32| {
        println!("WASM: {}", read_wasm_string(caller, ptr, len)?);
        Ok(())
    });

    let instance = Instance::new(&store, &module, &[log.into()])?;
    let run = instance
        .get_func("print_hello")
        .expect("`print_hello` was not an exported function");

    let run = run.get0::<()>()?;
    run()?;

    Ok(())
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
