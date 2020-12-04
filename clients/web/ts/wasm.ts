
type WasmModule = typeof import("../wasm/exocore_client_web");
type ExocoreClient = import("../wasm/exocore_client_web").ExocoreClient;
type Discovery = import("../wasm/exocore_client_web").Discovery;
type LocalNode = import("../wasm/exocore_client_web").LocalNode;

export { WasmModule, ExocoreClient, Discovery, LocalNode };

var module: WasmModule = null;

export async function getOrLoadModule(): Promise<WasmModule> {
    if (module == null) {
        module = await import('../wasm/exocore_client_web');
    }

    return module;
}

export function getModule(): WasmModule {
    if (!module) {
        throw "exocore wasm needs to be loaded first";
    }

    return module;
}