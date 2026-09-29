/* tslint:disable */
/* eslint-disable */

/**
 * The browser-facing GPU adapter: owns the WebGPU canvas and drains the
 * shell's GPU submit queue through it. wasm32-only — the U13 canvas module
 * (real `virtio_gpu_bridge` → `gles2wgpu` → WebGPU) does not exist natively.
 */
export class GpuCanvas {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Boot the bridge and bind it to `canvas_id`. Async because WebGPU
     * device acquisition awaits the browser. Fails loudly on a missing
     * canvas, missing WebGPU, or an incompatible canvas format — never
     * half-initialised.
     */
    static create(canvas_id: string): Promise<GpuCanvas>;
    /**
     * Drain the shell's pending GPU submit (if any) and execute every
     * decoded command through U8 dispatch on the canvas bridge, WITHOUT
     * presenting. Returns the number of commands executed. Used to isolate
     * render vs present failures.
     */
    execute_pending(shell: PathnShell): number;
    /**
     * Present the bridge's render target to the canvas surface.
     *
     * NOTE: headless Chromium's Dawn loses the device on
     * `get_current_texture()`; headless flows must use `execute_pending` +
     * `readback` instead and never call this.
     */
    present_canvas(): void;
    /**
     * Drain the shell's pending GPU submit (if any), execute every decoded
     * command through U8 dispatch on the canvas bridge, then present.
     * Returns the number of commands executed.
     *
     * U7 decode errors fail the call — they are surfaced, never silently
     * dropped (U12's `GpuSubmit` contract).
     */
    pump(shell: PathnShell): number;
    /**
     * Read back the bridge's 3D render target as 640×480 RGBA8 bytes —
     * the ground-truth pixel assertion path (headless screenshots do not
     * composite WebGPU canvases).
     */
    readback(): Promise<Uint8Array>;
}

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
    readonly __wbg_gpucanvas_free: (a: number, b: number) => void;
    readonly __wbg_pathnshell_free: (a: number, b: number) => void;
    readonly gpucanvas_create: (a: number, b: number) => any;
    readonly gpucanvas_execute_pending: (a: number, b: number) => [number, number, number];
    readonly gpucanvas_present_canvas: (a: number) => [number, number];
    readonly gpucanvas_pump: (a: number, b: number) => [number, number, number];
    readonly gpucanvas_readback: (a: number) => any;
    readonly pathnshell_new: () => [number, number, number];
    readonly pathnshell_parked: (a: number) => number;
    readonly pathnshell_push_key: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly pathnshell_step_frame: (a: number) => [number, number];
    readonly pathnshell_steps: (a: number) => number;
    readonly pathnshell_tx_text: (a: number) => [number, number];
    readonly wasm_bindgen_395a491bc4a88f71___convert__closures_____invoke___js_sys_c4aba649b38ec40f___Function_fn_wasm_bindgen_395a491bc4a88f71___JsValue_____wasm_bindgen_395a491bc4a88f71___sys__Undefined___js_sys_c4aba649b38ec40f___Function_fn_wasm_bindgen_395a491bc4a88f71___JsValue_____wasm_bindgen_395a491bc4a88f71___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_395a491bc4a88f71___convert__closures_____invoke___wasm_bindgen_395a491bc4a88f71___JsValue__core_ed718c3d60ebd546___result__Result_____wasm_bindgen_395a491bc4a88f71___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_395a491bc4a88f71___convert__closures_____invoke___wasm_bindgen_395a491bc4a88f71___JsValue______true_: (a: number, b: number, c: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
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
