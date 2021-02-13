use anyhow::anyhow;
use wasmtime::*;

fn main() -> Result<(), anyhow::Error> {
    let engine = Engine::default();
    let store = Store::new(&engine);
    let module = Module::from_file(&engine, "hello.wasm")?;

    let log = Func::wrap(&store, |caller: Caller<'_>, ptr: i32, len: i32| {
        println!("WASM: {}", read_wasm_string(caller, ptr, len)?);
        Ok(())
    });

    let blah = Func::wrap(&store, |caller: Caller<'_>, ptr: i32, len: i32| {
        println!("WASM: {}", read_wasm_string(caller, ptr, len)?);
        Ok(())
    });
    let instance = Instance::new(&store, &module, &[log.into(), blah.into()])?;

    let exocore_init = instance
        .get_func("__exocore_init")
        .expect("`__exocore_init` was not an exported function");
    let exocore_init = exocore_init.get0::<()>()?;
    exocore_init()?;

    let send_resp = instance
        .get_func("send_resp")
        .expect("`send_resp` was not an exported function");
    let data_ptr = wasm_alloc(&instance, b"hello")?;
    let send_resp = send_resp.get2::<i32, i32, ()>()?;
    send_resp(data_ptr, 5)?;
    wasm_free(&instance, data_ptr, 5)?;

    let print_hello = instance
        .get_func("print_hello")
        .expect("`print_hello` was not an exported function");
    let print_hello = print_hello.get0::<()>()?;
    print_hello()?;

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

// Inspired from https://radu-matei.com/blog/practical-guide-to-wasm-memory/#passing-arrays-to-modules-using-wasmtime
fn wasm_alloc(instance: &Instance, bytes: &[u8]) -> Result<i32, anyhow::Error> {
    let mem = match instance.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return Err(anyhow!("failed to find host memory")),
    };

    let alloc = instance
        .get_func("__exocore_alloc")
        .expect("expected alloc function not found");
    let alloc_result = alloc.call(&vec![Val::from(bytes.len() as i32)])?;

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
    return Ok(guest_ptr_offset);
}

fn wasm_free(instance: &Instance, ptr: i32, size: i32) -> Result<(), anyhow::Error> {
    let alloc = instance
        .get_func("__exocore_free")
        .expect("expected alloc function not found");
    alloc.call(&vec![Val::from(ptr), Val::from(size)])?;

    Ok(())
}
