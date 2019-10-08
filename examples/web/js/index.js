import("../../../clients/wasm/pkg").then(module => {
    window.exocore = {
        client:  new module.ExocoreClient("ws://127.0.0.1:3340"),
        query: module.Query,
        mutation: module.Mutation,
    };
});
