import * as wasm from './wasm';
import { LocalNode } from './wasm';

export class NodeWrapper {
    generate(): LocalNode {
        const module = wasm.getModule();
        return module.LocalNode.generate();
    }

    from_json(json: string): LocalNode {
        const module = wasm.getModule();
        return module.LocalNode.from_json(json);
    }

    from_yaml(yaml: string): LocalNode {
        const module = wasm.getModule();
        return module.LocalNode.from_yaml(yaml);
    }
}