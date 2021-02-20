use runtime::AppRuntime;

mod runtime;

fn main() -> anyhow::Result<()> {
    let app_runtime = AppRuntime::from_file("hello.wasm", |msg| {
        println!("Got out message");
    })?;
    app_runtime.run()?;
    Ok(())
}
