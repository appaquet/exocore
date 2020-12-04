import * as wasm from './wasm';
import { Discovery, LocalNode } from './wasm';

export class DiscoveryWrapper {
    create(discoveryServiceUrl?: string): Discovery {
        const module = wasm.getModule();
        return module.Discovery.new(discoveryServiceUrl);
    }
}