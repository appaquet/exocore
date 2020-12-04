
export class CellWrapper {
    wasmClient: any;
    statusChangeCallback: () => void;

    constructor(inner: any) {
        this.wasmClient = inner;
    }

    generateAuthToken(expirationDays?: number): Array<string> {
        return this.wasmClient.cell_generate_auth_token(expirationDays ?? 0);
    }
}