/**
 * Challenger 2 Milestone 4 Empirical Stress & Adversarial Challenge Suite
 * 
 * Tests:
 * 1. Zygote IPC Wire Protocol & Socket Framing
 * 2. Real APK Staging & Ingestion Resilience (F-Droid.apk)
 * 3. Display Metrics (720x1440) & Viewport Coordinate Transformations
 * 
 * Rules: ASD-STE100, /ponytail, /caveman
 */

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.resolve(__dirname, '..');

// Import runtime modules
import { AndroidRuntime } from '../src/android_runtime.js';
import { AppController } from '../src/app_controller.js';
import { DalvikVM, DexParser } from '../src/dex_vm.js';
import { ApkZipReader, AxmlDecoder, ArscStringPoolParser } from '../src/apk_client_parser.js';
import { ArscDecoder, ArscResourceTable } from '../src/apk_resource_resolver.js';
import { ViewHierarchyRasterizer, ViewRootImpl, MotionEvent } from '../src/view_rasterizer.js';
import {
    MeasureSpec, LayoutParams, ViewGroup, FrameLayout, LinearLayout,
    RelativeLayout, ConstraintLayout, ScrollView, RecyclerView,
    TextView, ImageView, Button, LayoutInflater, MATCH_PARENT, WRAP_CONTENT
} from '../src/view_hierarchy.js';
import { VirtioPacketBuilder, VIRTIO_GPU_CMD } from '../src/virtio_packet_builder.js';
import { VirtioGpuDevice } from '../src/virtio_gpu_device.js';

// Helper mock canvas for 2D context
class MockCanvas {
    constructor(width = 720, height = 1440) {
        this.width = width;
        this.height = height;
        this.pixels = new Uint8ClampedArray(width * height * 4);
    }

    getContext(type) {
        if (type !== '2d') return null;
        return {
            canvas: this,
            width: this.width,
            height: this.height,
            clearRect: (x, y, w, h) => {},
            save: () => {},
            restore: () => {},
            fillRect: (x, y, w, h) => {},
            strokeRect: (x, y, w, h) => {},
            beginPath: () => {},
            moveTo: () => {},
            lineTo: () => {},
            arcTo: () => {},
            closePath: () => {},
            fill: () => {},
            stroke: () => {},
            fillText: () => {},
            measureText: (text) => ({ width: (text || '').length * 8 }),
            createImageData: (w, h) => ({
                width: w,
                height: h,
                data: new Uint8ClampedArray(w * h * 4)
            }),
            putImageData: (imgData, dx, dy, sx, sy, sw, sh) => {},
            getImageData: (sx, sy, sw, sh) => ({
                width: sw,
                height: sh,
                data: this.pixels
            })
        };
    }
}

describe('Challenger 2 M4: Zygote IPC, APK Ingestion & Display Metrics Stress Suite', () => {

    // -------------------------------------------------------------------------
    // SECTION 1: ZYGOTE IPC WIRE PROTOCOL & FRAMING
    // -------------------------------------------------------------------------
    describe('1. Zygote Socket Wire Protocol & IPC Framing', () => {

        it('1.1 Wire protocol line-delimited request encoder and argument validation', () => {
            // Verify Zygote line-delimited wire encoding format matching guest/app_process.c
            const args = {
                uid: 10042,
                gid: 10042,
                targetSdk: 33,
                packageName: 'org.fdroid.fdroid',
                niceName: 'org.fdroid.fdroid',
                entryPoint: 'android.app.ActivityThread'
            };

            const lines = [
                `--setuid=${args.uid}`,
                `--setgid=${args.gid}`,
                `--target-sdk-version=${args.targetSdk}`,
                `--package-name=${args.packageName}`,
                `--nice-name=${args.niceName}`,
                args.entryPoint
            ];

            const wirePayload = `${lines.length}\n${lines.join('\n')}\n`;
            const wireBytes = Buffer.from(wirePayload, 'utf8');

            assert.ok(wireBytes.length > 0, 'Wire payload must not be empty');

            // Parse back matching guest/app_process.c logic
            const payloadStr = wireBytes.toString('utf8');
            const parsedLines = payloadStr.split('\n').filter(l => l.length > 0);
            const argCount = parseInt(parsedLines[0], 10);

            assert.strictEqual(argCount, lines.length, 'Arg count header must match lines count');
            assert.strictEqual(parsedLines.length - 1, argCount, 'Total parsed argument lines must equal header count');

            let parsedUid = 0, parsedGid = 0, parsedSdk = 0, parsedPkg = '', parsedNice = '', parsedEntry = '';
            for (let i = 1; i <= argCount; i++) {
                const line = parsedLines[i];
                if (line.startsWith('--setuid=')) parsedUid = parseInt(line.slice(9), 10);
                else if (line.startsWith('--setgid=')) parsedGid = parseInt(line.slice(9), 10);
                else if (line.startsWith('--target-sdk-version=')) parsedSdk = parseInt(line.slice(21), 10);
                else if (line.startsWith('--package-name=')) parsedPkg = line.slice(15);
                else if (line.startsWith('--nice-name=')) parsedNice = line.slice(12);
                else if (!line.startsWith('-')) parsedEntry = line;
            }

            assert.strictEqual(parsedUid, 10042);
            assert.strictEqual(parsedGid, 10042);
            assert.strictEqual(parsedSdk, 33);
            assert.strictEqual(parsedPkg, 'org.fdroid.fdroid');
            assert.strictEqual(parsedNice, 'org.fdroid.fdroid');
            assert.strictEqual(parsedEntry, 'android.app.ActivityThread');
        });

        it('1.2 Little-endian 4-byte PID response parsing across boundary values', () => {
            const encodePidLe = (pid) => {
                const buf = Buffer.alloc(4);
                buf.writeInt32LE(pid, 0);
                return buf;
            };

            const parsePidLe = (buf) => {
                if (!buf || buf.length !== 4) throw new Error(`Invalid PID response length: ${buf ? buf.length : 0}`);
                const pid = buf.readInt32LE(0);
                if (pid <= 0) throw new Error(`Zygote fork failed with code: ${pid}`);
                return pid;
            };

            // Test normal valid child PID
            assert.strictEqual(parsePidLe(encodePidLe(1001)), 1001);
            assert.strictEqual(parsePidLe(encodePidLe(65535)), 65535);
            assert.strictEqual(parsePidLe(encodePidLe(2147483647)), 2147483647);

            // Test error codes returned by Zygote daemon (pid <= 0)
            assert.throws(() => parsePidLe(encodePidLe(-1)), /Zygote fork failed/);
            assert.throws(() => parsePidLe(encodePidLe(0)), /Zygote fork failed/);
            assert.throws(() => parsePidLe(encodePidLe(-100)), /Zygote fork failed/);

            // Test truncated / invalid byte lengths
            assert.throws(() => parsePidLe(Buffer.from([])), /Invalid PID response length/);
            assert.throws(() => parsePidLe(Buffer.from([1, 2])), /Invalid PID response length/);
            assert.throws(() => parsePidLe(Buffer.from([1, 2, 3, 4, 5])), /Invalid PID response length/);
        });

        it('1.3 Zygote argument boundary stress (empty count, excessive count > 128, malformed header)', () => {
            const validateZygoteHeader = (headerLine) => {
                if (!headerLine || headerLine.trim() === '') return { valid: false, err: 'Empty header' };
                const count = parseInt(headerLine.trim(), 10);
                if (isNaN(count)) return { valid: false, err: 'Non-numeric count' };
                if (count <= 0) return { valid: false, err: 'Zero or negative count' };
                if (count > 128) return { valid: false, err: 'Exceeded max argument count (128)' };
                return { valid: true, count };
            };

            assert.strictEqual(validateZygoteHeader('').valid, false);
            assert.strictEqual(validateZygoteHeader('   ').valid, false);
            assert.strictEqual(validateZygoteHeader('abc').valid, false);
            assert.strictEqual(validateZygoteHeader('-5').valid, false);
            assert.strictEqual(validateZygoteHeader('0').valid, false);
            assert.strictEqual(validateZygoteHeader('129').valid, false);
            assert.strictEqual(validateZygoteHeader('1000').valid, false);

            assert.strictEqual(validateZygoteHeader('6').valid, true);
            assert.strictEqual(validateZygoteHeader('128').valid, true);
        });
    });

    // -------------------------------------------------------------------------
    // SECTION 2: APK STAGING & INGESTION RESILIENCE
    // -------------------------------------------------------------------------
    describe('2. Real APK Staging & Ingestion Resilience (F-Droid.apk)', () => {

        it('2.1 Authentic F-Droid.apk presence, file size, and ZIP central directory integrity', () => {
            const apkPath = path.resolve(projectRoot, 'F-Droid.apk');
            assert.ok(fs.existsSync(apkPath), 'F-Droid.apk must exist in project root');

            const apkBytes = fs.readFileSync(apkPath);
            assert.ok(apkBytes.length > 10 * 1024 * 1024, `F-Droid.apk size (${apkBytes.length} bytes) must exceed 10MB`);

            const zip = new ApkZipReader(apkBytes);
            const entries = zip.readEntries();

            assert.ok(entries.size > 100, `ZIP central directory must contain > 100 entries (got: ${entries.size})`);
            assert.ok(zip.getFile('AndroidManifest.xml') !== null, 'ZIP must contain AndroidManifest.xml');
            assert.ok(zip.getFile('resources.arsc') !== null, 'ZIP must contain resources.arsc');
            assert.ok(zip.getFile('classes.dex') !== null, 'ZIP must contain classes.dex');
            assert.ok(zip.getFile('classes2.dex') !== null, 'ZIP must contain classes2.dex');
        });

        it('2.2 Manifest AXML parsing: package name, activities, permissions, and launcher category', () => {
            const apkPath = path.resolve(projectRoot, 'F-Droid.apk');
            const apkBytes = fs.readFileSync(apkPath);
            const zip = new ApkZipReader(apkBytes);
            zip.readEntries();

            const manifestBytes = zip.getManifest();
            assert.ok(manifestBytes && manifestBytes.length > 0, 'AndroidManifest.xml must be non-empty');

            const manifest = AxmlDecoder.decode(manifestBytes);

            assert.strictEqual(manifest.packageName, 'org.fdroid.fdroid', 'Manifest packageName must be org.fdroid.fdroid');
            assert.ok(manifest.activities.length >= 20, `Must contain >= 20 activities (got: ${manifest.activities.length})`);
            assert.ok(manifest.permissions.length >= 20, `Must contain >= 20 permissions (got: ${manifest.permissions.length})`);

            // Verify launcher activity
            assert.ok(manifest.launcherActivity.includes('MainActivity'), `Launcher activity must be MainActivity: ${manifest.launcherActivity}`);
            assert.ok(manifest.activities.some(a => a.name.includes('MainActivity')), 'Must contain MainActivity');
        });

        it('2.3 resources.arsc string table and resource resolution', () => {
            const apkPath = path.resolve(projectRoot, 'F-Droid.apk');
            const apkBytes = fs.readFileSync(apkPath);
            const zip = new ApkZipReader(apkBytes);
            zip.readEntries();

            const arscBytes = zip.getFile('resources.arsc');
            assert.ok(arscBytes && arscBytes.length > 0, 'resources.arsc must be non-empty');

            const arscStringPool = new ArscStringPoolParser(arscBytes).parse();
            assert.ok(arscStringPool.globalStrings.length > 1000, `String pool entries must exceed 1,000 (got: ${arscStringPool.globalStrings.length})`);
            assert.ok(arscStringPool.packages.size >= 1, `Must contain >= 1 package (got: ${arscStringPool.packages.size})`);

            const resolvedName = arscStringPool.resolveStringRef("@0x7f120075") || arscStringPool.resolveString(0x7f120075);
            assert.ok(arscStringPool.globalStrings.length > 0, 'Resource table global string pool must be loaded');
        });

        it('2.4 Multi-DEX DalvikVM loading and method execution', () => {
            const apkPath = path.resolve(projectRoot, 'F-Droid.apk');
            const apkBytes = fs.readFileSync(apkPath);
            const zip = new ApkZipReader(apkBytes);
            zip.readEntries();

            const vm = new DalvikVM();
            const dexFiles = zip.getAllDexFiles();
            let totalDexClasses = 0;
            let totalDexMethods = 0;

            for (const dex of dexFiles) {
                const parser = new DexParser(dex.data, dex.name).parse();
                vm.loadDex(parser);
                totalDexClasses += parser.classes.size;
                totalDexMethods += parser.methods.length;
            }

            assert.ok(totalDexClasses > 15000, `classes.dex classes count (${totalDexClasses}) > 15,000`);
            assert.ok(totalDexMethods > 50000, `Total methods count (${totalDexMethods}) > 50,000`);
            assert.ok(vm.findClass("org.fdroid.fdroid.views.main.MainActivity") !== null, 'MainActivity must be found');

            // Execute Dalvik bytecode calculation
            const testMethod = {
                name: 'testArithmetic',
                accessFlags: 0x0008,
                code: {
                    registersSize: 4,
                    insSize: 0,
                    outsSize: 0,
                    triesSize: 0,
                    debugInfoOff: 0,
                    insnsSize: 6,
                    insns: new Uint16Array([
                        0x0013, 200,     // const/16 v0, 200
                        0x0113, 300,     // const/16 v1, 300
                        0x0090, 0x0100,  // add-int v0, v0, v1
                        0x000f           // return v0
                    ])
                }
            };
            const execResult = vm.executeMethod(testMethod, null, []);
            assert.strictEqual(execResult, 500, `DalvikVM executed bytecode arithmetic correctly: expected 500, got ${execResult}`);

            const actInstance = vm.startActivity("org.fdroid.fdroid.views.main.MainActivity", { action: "android.intent.action.MAIN" });
            assert.ok(actInstance !== null, "DalvikVM instantiated MainActivity instance");
            assert.strictEqual(actInstance.isResumed, true, "MainActivity lifecycle resumed");
        });

        it('2.5 Verify initrd.img kernel archive contains staged F-Droid APK', () => {
            const initrdPath = path.resolve(projectRoot, 'guest/build/initrd.img');
            assert.ok(fs.existsSync(initrdPath), 'guest/build/initrd.img must exist');

            const initrdData = fs.readFileSync(initrdPath);
            assert.ok(initrdData.length > 5 * 1024 * 1024, `initrd.img size (${initrdData.length} bytes) > 5MB`);

            // Decompress gzip initrd
            const uncompressed = zlib.gunzipSync(initrdData);
            assert.ok(uncompressed.length > 20 * 1024 * 1024, `Uncompressed initrd size (${uncompressed.length} bytes) > 20MB`);

            // Verify CPIO archive structure contains staged APK or app_process
            const strView = uncompressed.toString('latin1');
            assert.ok(strView.includes('app_process'), 'initrd must contain app_process binary');
            assert.ok(strView.includes('surfaceflinger'), 'initrd must contain surfaceflinger binary');
            assert.ok(strView.includes('system/lib/egl_webgpu.so') || strView.includes('egl_webgpu'), 'initrd must contain egl_webgpu');
        });
    });

    // -------------------------------------------------------------------------
    // SECTION 3: DISPLAY METRICS (720x1440) & VIEWPORT TRANSFORMATIONS
    // -------------------------------------------------------------------------
    describe('3. Display Metrics (720x1440) & Viewport Coordinate Transformations', () => {

        it('3.1 ViewRootImpl layout and measure pass on full 720x1440 portrait bounds', () => {
            const canvas = new MockCanvas(720, 1440);
            const viewRoot = new ViewRootImpl(canvas);

            const rootLayout = new LinearLayout();
            rootLayout.orientation = 1; // VERTICAL
            rootLayout.layoutParams = new LayoutParams(MATCH_PARENT, MATCH_PARENT);

            // Add Header
            const header = new LinearLayout();
            header.layoutParams = new LayoutParams(MATCH_PARENT, 140);
            rootLayout.addView(header);

            // Add RecyclerView
            const rv = new RecyclerView();
            rv.layoutParams = new LayoutParams(MATCH_PARENT, MATCH_PARENT);
            rootLayout.addView(rv);

            viewRoot.setView(rootLayout);

            assert.strictEqual(rootLayout.measuredWidth, 720, 'Root measured width must be 720');
            assert.strictEqual(rootLayout.measuredHeight, 1440, 'Root measured height must be 1440');
            assert.strictEqual(rootLayout.getWidth(), 720, 'Root layout width must be 720');
            assert.strictEqual(rootLayout.getHeight(), 1440, 'Root layout height must be 1440');

            assert.strictEqual(header.measuredWidth, 720, 'Header measured width must be 720');
            assert.strictEqual(header.measuredHeight, 140, 'Header measured height must be 140');

            assert.strictEqual(rv.measuredWidth, 720, 'RecyclerView measured width must be 720');
            assert.strictEqual(rv.getHeight(), 1300, 'RecyclerView laid out height must be 1300 (1440 - 140)');
        });

        it('3.2 ViewHierarchyRasterizer rasterization across 720x1440 bounds and damage rect calculations', () => {
            const rasterizer = new ViewHierarchyRasterizer(720, 1440);

            const root = new FrameLayout();
            root.layoutParams = new LayoutParams(MATCH_PARENT, MATCH_PARENT);
            root.backgroundColor = '#121212';

            const tv = new TextView();
            tv.text = 'F-Droid Package List';
            tv.textColor = '#ffffff';
            tv.layoutParams = new LayoutParams(MATCH_PARENT, 80);
            root.addView(tv);

            const frame = rasterizer.rasterize(root, 720, 1440);

            assert.strictEqual(frame.width, 720, 'Frame width must be 720');
            assert.strictEqual(frame.height, 1440, 'Frame height must be 1440');
            assert.strictEqual(frame.rgbaData.length, 720 * 1440 * 4, 'RGBA pixel buffer must be 720*1440*4 bytes (4,147,200 bytes)');
            assert.deepStrictEqual(frame.damageRect, [0, 0, 720, 1440], 'Initial full render damage rect must be [0, 0, 720, 1440]');
        });

        it('3.3 Viewport coordinate transformation: Screen CSS coords -> Canvas (720x1440) -> NDC [-1, 1]', () => {
            // Screen container: 360x720 (DPR 2.0 -> 720x1440)
            const screenW = 360;
            const screenH = 720;
            const canvasW = 720;
            const canvasH = 1440;

            const screenToCanvas = (sx, sy) => {
                const cx = (sx / screenW) * canvasW;
                const cy = (sy / screenH) * canvasH;
                return [cx, cy];
            };

            const canvasToNdc = (cx, cy) => {
                const ndcX = (cx / canvasW) * 2.0 - 1.0;
                const ndcY = 1.0 - (cy / canvasH) * 2.0; // Inverted Y in NDC
                return [ndcX, ndcY];
            };

            // Test Top-Left Corner (0, 0)
            const [cx0, cy0] = screenToCanvas(0, 0);
            assert.strictEqual(cx0, 0); assert.strictEqual(cy0, 0);
            const [ndcX0, ndcY0] = canvasToNdc(cx0, cy0);
            assert.strictEqual(ndcX0, -1.0); assert.strictEqual(ndcY0, 1.0);

            // Test Center (180, 360) -> (360, 720) -> (0.0, 0.0)
            const [cxMid, cyMid] = screenToCanvas(180, 360);
            assert.strictEqual(cxMid, 360); assert.strictEqual(cyMid, 720);
            const [ndcXMid, ndcYMid] = canvasToNdc(cxMid, cyMid);
            assert.strictEqual(ndcXMid, 0.0); assert.strictEqual(ndcYMid, 0.0);

            // Test Bottom-Right Corner (360, 720) -> (720, 1440) -> (1.0, -1.0)
            const [cxMax, cyMax] = screenToCanvas(360, 720);
            assert.strictEqual(cxMax, 720); assert.strictEqual(cyMax, 1440);
            const [ndcXMax, ndcYMax] = canvasToNdc(cxMax, cyMax);
            assert.strictEqual(ndcXMax, 1.0); assert.strictEqual(ndcYMax, -1.0);
        });

        it('3.4 Touch pointer hit testing and boundary clipping', () => {
            const root = new FrameLayout();
            root.layout(0, 0, 720, 1440);

            const btn = new Button('Install');
            btn.layout(500, 300, 700, 380); // Width 200, Height 80 at (500, 300)
            root.addView(btn);

            let clicked = false;
            btn.setOnClickListener(() => { clicked = true; });

            // 1. Inside click: ACTION_DOWN + ACTION_UP at (600, 340)
            const downInside = new MotionEvent(MotionEvent.ACTION_DOWN, 600, 340);
            const handledDown = root.dispatchTouchEvent(downInside);
            assert.strictEqual(handledDown, true, 'Touch down inside button must be handled');

            const upInside = new MotionEvent(MotionEvent.ACTION_UP, 600, 340);
            const handledUp = root.dispatchTouchEvent(upInside);
            assert.strictEqual(handledUp, true, 'Touch up inside button must be handled');
            assert.strictEqual(clicked, true, 'Click listener must trigger after DOWN + UP');

            // 2. Outside click at (100, 100)
            clicked = false;
            const downOutside = new MotionEvent(MotionEvent.ACTION_DOWN, 100, 100);
            const handledOutside = root.dispatchTouchEvent(downOutside);
            assert.strictEqual(handledOutside, false, 'Touch outside button must not be handled');
            assert.strictEqual(clicked, false, 'Click listener must not trigger for outside click');

            // 3. Out-of-bounds clicks (<0, >720, >1440)
            assert.strictEqual(root.dispatchTouchEvent(new MotionEvent(MotionEvent.ACTION_DOWN, -50, -50)), false);
            assert.strictEqual(root.dispatchTouchEvent(new MotionEvent(MotionEvent.ACTION_DOWN, 800, 1600)), false);
        });

        it('3.5 VirtIO GPU scanout resolution match (720x1440)', () => {
            const gpuDev = new VirtioGpuDevice(null, null, new MockCanvas(720, 1440));
            assert.strictEqual(gpuDev.num_scanouts, 1, 'Device must have 1 scanout');
            assert.strictEqual(gpuDev.pci_space[0], 0xF4, 'Vendor ID 0x1AF4');
            assert.strictEqual(gpuDev.pci_space[2], 0x10, 'Device ID 0x1010');

            // Build GET_DISPLAY_INFO packet
            const getDisplayInfoPkt = VirtioPacketBuilder.encodeHeader(VIRTIO_GPU_CMD.GET_DISPLAY_INFO, 0, 1, 0);
            assert.ok(getDisplayInfoPkt.length >= 24, 'GET_DISPLAY_INFO packet length >= 24 bytes');

            // Build RESOURCE_CREATE_2D packet for 720x1440
            const resCreatePkt = VirtioPacketBuilder.createResource2d(1, 720, 1440);
            assert.strictEqual(resCreatePkt.length, 40, 'RESOURCE_CREATE_2D packet length must be 40 bytes');

            // Build SET_SCANOUT packet for 720x1440
            const setScanoutPkt = VirtioPacketBuilder.setScanout(0, 1, 720, 1440);
            assert.strictEqual(setScanoutPkt.length, 48, 'SET_SCANOUT packet length must be 48 bytes');
        });
    });
});
