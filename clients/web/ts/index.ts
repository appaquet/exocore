
import * as protos from '../protos';
import { exocore } from '../protos';
export { protos, exocore }

import { CellWrapper } from './cell';
import { DiscoveryWrapper } from './discovery';
export { CellWrapper as Cell };

import { NodeWrapper } from './node';
export { NodeWrapper };

import { Registry, matchTrait } from './registry';
export { Registry, matchTrait }

import { Store, WatchedQuery, MutationBuilder, QueryBuilder, TraitQueryBuilder } from './store';
export { WatchedQuery, MutationBuilder, QueryBuilder, TraitQueryBuilder };

import * as wasm from './wasm';
import { WasmModule, ExocoreClient, LocalNode, Discovery } from './wasm';
export { WasmModule, ExocoreClient, LocalNode, Discovery };

export class Exocore {
    static defaultInstance: ExocoreInstance = null;

    static get initialized(): boolean {
        return Exocore.defaultInstance != null;
    }

    static async ensureLoaded(): Promise<WasmModule> {
        return await wasm.getOrLoadModule();
    }

    static async initialize(config: object): Promise<ExocoreInstance> {
        const configJson = JSON.stringify(config); ``
        const configBytes = new TextEncoder().encode(configJson);

        const module = await Exocore.ensureLoaded();

        let instance: ExocoreInstance;
        const onStatusChange = (status: string) => {
            instance._triggerStatusChange(status)
        }

        const innerClient = new module.ExocoreClient(configBytes, 'json', onStatusChange);
        instance = new ExocoreInstance(innerClient);

        if (!Exocore.defaultInstance) {
            Exocore.defaultInstance = instance;
        }

        return instance;
    }

    static get cell(): CellWrapper {
        return Exocore.defaultInstance.cell;
    }

    static get store(): Store {
        return Exocore.defaultInstance.store;
    }

    static get registry(): Registry {
        return Exocore.defaultInstance.registry;
    }

    static node = new NodeWrapper();

    static discovery = new DiscoveryWrapper();
}

export class ExocoreInstance {
    wasmClient: ExocoreClient;
    cell: CellWrapper;
    store: Store;
    status: string;
    registry: Registry;
    node: Node;
    onChange?: () => void;

    constructor(client: ExocoreClient) {
        this.wasmClient = client;
        this.cell = new CellWrapper(client);
        this.store = new Store(client);
        this.registry = new Registry();
        this.node = new Node();
    }

    _triggerStatusChange(status: string): void {
        this.status = status;
        if (this.onChange) {
            this.onChange();
        }
    }
}

export function toProtoTimestamp(date: Date): protos.google.protobuf.ITimestamp {
    const epoch = date.getTime();
    const seconds = Math.floor(epoch / 1000);

    return new protos.google.protobuf.Timestamp({
        seconds: seconds,
        nanos: (epoch - (seconds * 1000)) * 1000000,
    });
}

export function fromProtoTimestamp(ts: protos.google.protobuf.ITimestamp): Date {
    return new Date((ts.seconds as number) * 1000 + ts.nanos / 1000000);
}