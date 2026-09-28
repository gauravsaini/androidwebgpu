/* tslint:disable */
/* eslint-disable */

/**
 * The browser-facing shell: emulator state owned by Rust, rendering and
 * event capture owned by JavaScript.
 */
export class PathnShell {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Boot the real guest. Fails closed: any divergence from the exact
     * boot evidence (park address, prompt bytes) returns an error instead
     * of a shell that only looks alive.
     */
    constructor();
    /**
     * True while the guest is alive and parked at WFI waiting for input.
     * WFI parking never poisons `halted` — only a real halt does.
     */
    parked(): boolean;
    /**
     * Feed one DOM keyboard event into the byte queue. Called from the
     * JS `keydown`/`keyup` listeners with the event's `code`, `key`, and
     * pressed state. Key-up events and unmapped keys produce no bytes
     * (pinned by the native `KeyboardInput` tests).
     */
    push_key(code: string, key: string, pressed: boolean): void;
    /**
     * Run the guest for up to one frame's worth of steps and return the
     * NEW console TX bytes since the last call (UTF-8 lossy; the guest is
     * ASCII-only by platform contract).
     *
     * Stops early when the guest parks at WFI (nothing more to do until
     * new input arrives) or halts for real.
     */
    step_frame(): string;
    /**
     * Total vCPU blocks executed so far (diagnostics / acceptance).
     */
    steps(): number;
    /**
     * The exact console TX bytes observed so far (acceptance hook).
     */
    tx_text(): string;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_pathnshell_free: (a: number, b: number) => void;
    readonly pathnshell_new: () => [number, number, number];
    readonly pathnshell_parked: (a: number) => number;
    readonly pathnshell_push_key: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly pathnshell_step_frame: (a: number) => [number, number];
    readonly pathnshell_steps: (a: number) => number;
    readonly pathnshell_tx_text: (a: number) => [number, number];
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
