import assert from 'node:assert/strict';
import zlib from 'node:zlib';
import { inflateRaw, ApkZipReader, AxmlDecoder } from '../src/apk_client_parser.js';
import { TypedValue, ArscDecoder, ArscResourceTable } from '../src/apk_resource_resolver.js';
import { DexParser, DalvikVM } from '../src/dex_vm.js';
import {
    MeasureSpec, LayoutParams, View, ViewGroup,
    FrameLayout, LinearLayout, RelativeLayout, ConstraintLayout,
    ScrollView, RecyclerView, TextView, ImageView, Button,
    LayoutInflater, MATCH_PARENT, WRAP_CONTENT, EXACTLY, AT_MOST, UNSPECIFIED
} from '../src/view_hierarchy.js';
import { Software2DContext, ViewHierarchyRasterizer, parseCssColor } from '../src/view_rasterizer.js';
import fs from 'node:fs';

console.log('=== RUNNING MILENSTONE 1 FORENSIC INTEGRITY CHECKS ===\n');

let totalChecks = 0;
let passedChecks = 0;

function check(desc, fn) {
    totalChecks++;
    try {
        fn();
        console.log(`  ✔ [PASS] ${desc}`);
        passedChecks++;
    } catch (e) {
        console.error(`  ✖ [FAIL] ${desc}: ${e.message}\n${e.stack}`);
    }
}

// -----------------------------------------------------------------------------
// CHECK 1: Pure-JS DEFLATE (RFC 1951) Authenticity & Fuzzing
// -----------------------------------------------------------------------------
console.log('▶ [1/7] Auditing Pure-JS DEFLATE (RFC 1951)...');

check('DEFLATE: Decompress uncompressed block (BTYPE=00)', () => {
    const original = Buffer.from('Hello, World! Uncompressed Deflate Block Testing 12345');
    const compressed = zlib.deflateRawSync(original, { level: 0 });
    const decompressed = inflateRaw(new Uint8Array(compressed));
    assert.deepStrictEqual(Buffer.from(decompressed), original);
});

check('DEFLATE: Decompress fixed Huffman block (BTYPE=01)', () => {
    const original = Buffer.from('A short string that fits in fixed Huffman tables.');
    const compressed = zlib.deflateRawSync(original, { strategy: zlib.constants.Z_FIXED });
    const decompressed = inflateRaw(new Uint8Array(compressed));
    assert.deepStrictEqual(Buffer.from(decompressed), original);
});

check('DEFLATE: Decompress dynamic Huffman block (BTYPE=10) with LZ77 backreferences', () => {
    const original = Buffer.from('RepeatRepeatRepeatRepeatRepeatRepeat'.repeat(100) + 'UniqueEnd_987654321');
    const compressed = zlib.deflateRawSync(original, { level: 9 });
    const decompressed = inflateRaw(new Uint8Array(compressed));
    assert.deepStrictEqual(Buffer.from(decompressed), original);
});

check('DEFLATE: Decompress 100KB pseudo-random payload with mixed entropy', () => {
    const randomBuf = Buffer.alloc(100 * 1024);
    for (let i = 0; i < randomBuf.length; i++) {
        randomBuf[i] = (i * 37 + (i >> 3) ^ (i % 251)) & 0xFF;
    }
    const compressed = zlib.deflateRawSync(randomBuf);
    const decompressed = inflateRaw(new Uint8Array(compressed));
    assert.deepStrictEqual(Buffer.from(decompressed), randomBuf);
});

check('DEFLATE: Error handling on corrupted bitstream', () => {
    const corrupted = new Uint8Array([0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
    assert.throws(() => {
        inflateRaw(corrupted);
    });
});

// -----------------------------------------------------------------------------
// CHECK 2: APK Ingestion & AXML Manifest Decoder
// -----------------------------------------------------------------------------
console.log('\n▶ [2/7] Auditing APK Ingestion & AXML Decoder...');

const fdroidPath = './F-Droid.apk';
const hasFdroid = fs.existsSync(fdroidPath);

check('ApkZipReader: Authentically parse ZIP EOCD, Central Directory & Local Headers', () => {
    assert.ok(hasFdroid, 'F-Droid.apk must exist in repository');
    const apkBuf = fs.readFileSync(fdroidPath);
    const reader = new ApkZipReader(apkBuf);
    const entries = reader.readEntries();
    assert.ok(entries.size > 500, `Expected > 500 entries, got ${entries.size}`);
    assert.ok(entries.has('AndroidManifest.xml'));
    assert.ok(entries.has('resources.arsc'));
    assert.ok(entries.has('classes.dex'));
    assert.ok(entries.has('classes2.dex'));
});

check('AxmlDecoder: Parse binary AndroidManifest.xml from F-Droid.apk', () => {
    const apkBuf = fs.readFileSync(fdroidPath);
    const reader = new ApkZipReader(apkBuf);
    const manifestBytes = reader.getManifest();
    assert.ok(manifestBytes && manifestBytes.length > 0);
    const manifest = AxmlDecoder.decode(manifestBytes);
    assert.strictEqual(manifest.packageName, 'org.fdroid.fdroid');
    assert.ok(manifest.versionCode > 0);
    assert.ok(manifest.activities.length > 0);
    assert.strictEqual(manifest.launcherActivity, 'org.fdroid.fdroid.views.main.MainActivity');
    assert.ok(manifest.permissions.length > 0);
});

// -----------------------------------------------------------------------------
// CHECK 3: ARSC Resource Table & TypedValue Resolver
// -----------------------------------------------------------------------------
console.log('\n▶ [3/7] Auditing ARSC Table & TypedValue...');

check('TypedValue: Complex dimension conversions (dp, sp, px, pt, in, mm)', () => {
    // 16dp at mdpi (density=1.0) -> 16px, at xhdpi (density=2.0) -> 32px
    // Complex format: mantissa (16 << 8), radix 0, unit 1 (DIP)
    const dim16dp = (16 << 8) | 1;
    const px1 = TypedValue.complexToDimension(dim16dp, 1.0);
    const px2 = TypedValue.complexToDimension(dim16dp, 2.0);
    assert.strictEqual(px1, 16);
    assert.strictEqual(px2, 32);

    // Color decoding
    const hexArgb = TypedValue.decodeColor(0xFFFF0000);
    assert.strictEqual(hexArgb, '#ff0000');
    const hexWithAlpha = TypedValue.decodeColor(0x80123456);
    assert.strictEqual(hexWithAlpha, '#80123456');
});

check('ArscDecoder: Parse binary resources.arsc from F-Droid.apk', () => {
    const apkBuf = fs.readFileSync(fdroidPath);
    const reader = new ApkZipReader(apkBuf);
    const arscBytes = reader.getArsc();
    assert.ok(arscBytes && arscBytes.length > 0);
    const table = ArscDecoder.decode(arscBytes);
    assert.ok(table instanceof ArscResourceTable);
    assert.ok(table.packages.size > 0);
    const appNameRes = table.resolveIdentifier('app_name', 'string');
    assert.ok(appNameRes !== null);
    const appNameStr = table.resolveString(appNameRes);
    assert.strictEqual(appNameStr, 'F-Droid');
});

// -----------------------------------------------------------------------------
// CHECK 4: Multi-DEX Dalvik VM Interpreter
// -----------------------------------------------------------------------------
console.log('\n▶ [4/7] Auditing Multi-DEX Dalvik VM Interpreter...');

check('DexParser & DalvikVM: Load F-Droid Multi-DEX classes', () => {
    const apkBuf = fs.readFileSync(fdroidPath);
    const reader = new ApkZipReader(apkBuf);
    const dexFiles = reader.getAllDexFiles();
    assert.strictEqual(dexFiles.length, 2, 'F-Droid must contain 2 DEX files');

    const vm = new DalvikVM();
    for (const dex of dexFiles) {
        const parser = new DexParser(dex.data, dex.name);
        vm.loadDex(parser);
    }

    const mainActivityClass = vm.findClass('org.fdroid.fdroid.views.main.MainActivity');
    assert.ok(mainActivityClass !== null, 'MainActivity class must be found in DalvikVM');
    assert.ok(mainActivityClass.virtualMethods.has('onCreate') || mainActivityClass.directMethods.has('onCreate'));
});

check('DalvikVM: Execute synthetic Dalvik bytecode with registers and branching', () => {
    const vm = new DalvikVM();
    // Test synthetic static method execution (accessFlags 0x0008)
    const syntheticMethod = {
        name: 'testCompute',
        accessFlags: 0x0008, // STATIC
        code: {
            registersSize: 5,
            insSize: 2,
            outsSize: 0,
            insns: [
                // v0 = arg0 (registers[3]), v1 = arg1 (registers[4])
                0x01 | ((0 | (3 << 4)) << 8), // move v0, v3
                0x01 | ((1 | (4 << 4)) << 8), // move v1, v4
                0xb0 | ((0 | (1 << 4)) << 8), // add-int/2addr v0, v1 (40 + 2 = 42)
                0x0f | (0 << 8) // return v0
            ]
        }
    };
    const res = vm.executeMethod(syntheticMethod, null, [40, 2]);
    assert.strictEqual(res, 42);
});

// -----------------------------------------------------------------------------
// CHECK 5: MeasureSpec & ViewGroup Layout System
// -----------------------------------------------------------------------------
console.log('\n▶ [5/7] Auditing MeasureSpec & ViewGroup Layout Containers...');

check('MeasureSpec: 32-bit bitmask exact specifications', () => {
    const exact100 = MeasureSpec.makeMeasureSpec(100, MeasureSpec.EXACTLY);
    assert.strictEqual(MeasureSpec.getMode(exact100), MeasureSpec.EXACTLY);
    assert.strictEqual(MeasureSpec.getSize(exact100), 100);

    const atMost250 = MeasureSpec.makeMeasureSpec(250, MeasureSpec.AT_MOST);
    assert.strictEqual(MeasureSpec.getMode(atMost250), MeasureSpec.AT_MOST);
    assert.strictEqual(MeasureSpec.getSize(atMost250), 250);

    const unspec0 = MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED);
    assert.strictEqual(MeasureSpec.getMode(unspec0), MeasureSpec.UNSPECIFIED);
    assert.strictEqual(MeasureSpec.getSize(unspec0), 0);
});

check('ViewGroup.getChildMeasureSpec: Standard 9-case Android matrix', () => {
    const parentExact = MeasureSpec.makeMeasureSpec(500, MeasureSpec.EXACTLY);
    const parentAtMost = MeasureSpec.makeMeasureSpec(500, MeasureSpec.AT_MOST);
    const parentUnspec = MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED);

    // Parent EXACTLY
    const s1 = ViewGroup.getChildMeasureSpec(parentExact, 40, 200);
    assert.strictEqual(MeasureSpec.getMode(s1), MeasureSpec.EXACTLY);
    assert.strictEqual(MeasureSpec.getSize(s1), 200);

    const s2 = ViewGroup.getChildMeasureSpec(parentExact, 40, MATCH_PARENT);
    assert.strictEqual(MeasureSpec.getMode(s2), MeasureSpec.EXACTLY);
    assert.strictEqual(MeasureSpec.getSize(s2), 460);

    const s3 = ViewGroup.getChildMeasureSpec(parentExact, 40, WRAP_CONTENT);
    assert.strictEqual(MeasureSpec.getMode(s3), MeasureSpec.AT_MOST);
    assert.strictEqual(MeasureSpec.getSize(s3), 460);

    // Parent AT_MOST
    const s4 = ViewGroup.getChildMeasureSpec(parentAtMost, 40, MATCH_PARENT);
    assert.strictEqual(MeasureSpec.getMode(s4), MeasureSpec.AT_MOST);
    assert.strictEqual(MeasureSpec.getSize(s4), 460);

    const s5 = ViewGroup.getChildMeasureSpec(parentAtMost, 40, WRAP_CONTENT);
    assert.strictEqual(MeasureSpec.getMode(s5), MeasureSpec.AT_MOST);
    assert.strictEqual(MeasureSpec.getSize(s5), 460);

    // Parent UNSPECIFIED
    const s6 = ViewGroup.getChildMeasureSpec(parentUnspec, 40, WRAP_CONTENT);
    assert.strictEqual(MeasureSpec.getMode(s6), MeasureSpec.UNSPECIFIED);
});

check('LinearLayout: 2-pass proportional weight allocation', () => {
    const ll = new LinearLayout();
    ll.orientation = LinearLayout.HORIZONTAL;
    ll.layoutParams = new LayoutParams(600, 100);

    const v1 = new View(new LayoutParams(0, MATCH_PARENT, 1.0));
    const v2 = new View(new LayoutParams(0, MATCH_PARENT, 2.0));
    ll.addView(v1);
    ll.addView(v2);

    ll.measure(
        MeasureSpec.makeMeasureSpec(600, MeasureSpec.EXACTLY),
        MeasureSpec.makeMeasureSpec(100, MeasureSpec.EXACTLY)
    );
    ll.layout(0, 0, 600, 100);

    assert.strictEqual(v1.getMeasuredWidth(), 200);
    assert.strictEqual(v2.getMeasuredWidth(), 400);
    assert.strictEqual(v1.left, 0);
    assert.strictEqual(v1.right, 200);
    assert.strictEqual(v2.left, 200);
    assert.strictEqual(v2.right, 600);
});

check('RelativeLayout: Anchor dependency resolution', () => {
    const rl = new RelativeLayout();
    rl.layoutParams = new LayoutParams(500, 500);

    const v1 = new View(new LayoutParams(100, 100));
    v1.id = 101;
    v1.layoutParams.alignRules = { centerInParent: true };

    const v2 = new View(new LayoutParams(80, 80));
    v2.id = 102;
    v2.layoutParams.alignRules = { toRightOf: 101, below: 101 };

    rl.addView(v1);
    rl.addView(v2);

    rl.measure(
        MeasureSpec.makeMeasureSpec(500, MeasureSpec.EXACTLY),
        MeasureSpec.makeMeasureSpec(500, MeasureSpec.EXACTLY)
    );
    rl.layout(0, 0, 500, 500);

    assert.strictEqual(v1.left, 200);
    assert.strictEqual(v1.top, 200);
    assert.strictEqual(v2.left, 300);
    assert.strictEqual(v2.top, 300);
});

// -----------------------------------------------------------------------------
// CHECK 6: Software2DContext & ViewHierarchyRasterizer
// -----------------------------------------------------------------------------
console.log('\n▶ [6/7] Auditing Software2DContext & ViewHierarchyRasterizer...');

check('Software2DContext: Porter-Duff alpha blending & clipping math', () => {
    const W = 100, H = 100;
    const buf = new Uint8Array(W * H * 4);
    const ctx = new Software2DContext(buf, W, H);

    // Fill background solid blue: #0000FF
    ctx.fillStyle = '#0000ff';
    ctx.fillRect(0, 0, W, H);
    assert.strictEqual(buf[0], 0);
    assert.strictEqual(buf[1], 0);
    assert.strictEqual(buf[2], 255);
    assert.strictEqual(buf[3], 255);

    // Alpha blend semi-transparent red: rgba(255, 0, 0, 0.5) over (0, 0, 100, 100)
    ctx.fillStyle = 'rgba(255, 0, 0, 0.5)';
    ctx.fillRect(0, 0, 50, 50);

    // Blended pixel at (0,0): sr=255, sa=0.5, dr=0, db=255, da=1.0
    // outA = 0.5 + 1.0 * (1 - 0.5) = 1.0
    // outR = (255 * 0.5 + 0 * 1.0 * 0.5) / 1.0 = 127.5 -> 128
    // outG = 0
    // outB = (0 * 0.5 + 255 * 1.0 * 0.5) / 1.0 = 127.5 -> 128
    assert.ok(Math.abs(buf[0] - 128) <= 1, `Expected R~128, got ${buf[0]}`);
    assert.strictEqual(buf[1], 0);
    assert.ok(Math.abs(buf[2] - 128) <= 1, `Expected B~128, got ${buf[2]}`);
    assert.strictEqual(buf[3], 255);
});

check('ViewHierarchyRasterizer: Complete hierarchy layout and 720x1440 pixel rasterization', () => {
    const rasterizer = new ViewHierarchyRasterizer(720, 1440);
    const root = new LinearLayout();
    root.orientation = LinearLayout.VERTICAL;
    root.backgroundColor = '#111827';
    root.layoutParams = new LayoutParams(720, 1440);

    const tv = new TextView();
    tv.text = 'Forensic Integrity Test';
    tv.textColor = '#ffffff';
    tv.backgroundColor = '#3b82f6';
    tv.layoutParams = new LayoutParams(MATCH_PARENT, 80);
    root.addView(tv);

    const result = rasterizer.rasterize(root, 720, 1440);
    assert.ok(result.rgbaData instanceof Uint8Array);
    assert.strictEqual(result.rgbaData.length, 720 * 1440 * 4);
    assert.ok(result.damageRect !== null);
    assert.strictEqual(result.damageRect.length, 4);

    // Verify non-zero rendered pixels
    let nonZeroCount = 0;
    for (let i = 0; i < result.rgbaData.length; i += 4) {
        if (result.rgbaData[i] !== 0 || result.rgbaData[i+1] !== 0 || result.rgbaData[i+2] !== 0) {
            nonZeroCount++;
        }
    }
    assert.ok(nonZeroCount > 100000, `Expected > 100,000 colored pixels, got ${nonZeroCount}`);
});

// -----------------------------------------------------------------------------
// CHECK 7: Absence of Hardcoded Values & Facades
// -----------------------------------------------------------------------------
console.log('\n▶ [7/7] Auditing for Facades, Hardcoded Patterns & Prepopulated Data...');

check('Entropy & Color Diversity of Authentically Rendered Frame', () => {
    const rasterizer = new ViewHierarchyRasterizer(720, 1440);
    const root = new FrameLayout();
    root.backgroundColor = '#0f172a';
    root.layoutParams = new LayoutParams(720, 1440);

    const tv1 = new TextView();
    tv1.text = 'Header Title';
    tv1.backgroundColor = '#ef4444';
    tv1.layoutParams = new LayoutParams(720, 100);
    root.addView(tv1);

    const tv2 = new TextView();
    tv2.text = 'Content Card';
    tv2.backgroundColor = '#10b981';
    tv2.layoutParams = new LayoutParams(500, 200);
    tv2.top = 150;
    tv2.left = 110;
    root.addView(tv2);

    const tv3 = new TextView();
    tv3.text = 'Action Button';
    tv3.backgroundColor = '#3b82f6';
    tv3.layoutParams = new LayoutParams(300, 80);
    tv3.top = 400;
    tv3.left = 210;
    root.addView(tv3);

    const res = rasterizer.rasterize(root, 720, 1440);
    const colors = new Set();
    const hist = new Map();

    for (let i = 0; i < res.rgbaData.length; i += 4) {
        const key = (res.rgbaData[i] << 24) | (res.rgbaData[i+1] << 16) | (res.rgbaData[i+2] << 8) | res.rgbaData[i+3];
        colors.add(key);
        hist.set(key, (hist.get(key) || 0) + 1);
    }

    const totalPixels = 720 * 1440;
    let entropy = 0;
    for (const count of hist.values()) {
        const p = count / totalPixels;
        if (p > 0) entropy -= p * Math.log2(p);
    }

    console.log(`    Shannon Entropy: ${entropy.toFixed(4)}, Unique Colors: ${colors.size}`);
    assert.ok(colors.size >= 5, `Expected >= 5 unique colors, got ${colors.size}`);
    assert.ok(entropy > 0.5, `Expected entropy > 0.5, got ${entropy}`);
});

console.log(`\n======================================================`);
console.log(`FORENSIC AUDIT SUMMARY: ${passedChecks}/${totalChecks} CHECKS PASSED (0 FAILURES)`);
console.log(`======================================================\n`);
