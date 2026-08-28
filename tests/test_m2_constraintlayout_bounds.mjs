/**
 * Milestone M2 Verification Test Suite:
 * ConstraintLayout MATCH_CONSTRAINT 0dp Measurement & Vector Drawable Engine
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { ApkZipReader, AxmlDecoder } from '../src/apk_client_parser.js';
import { ArscDecoder } from '../src/apk_resource_resolver.js';
import {
    MeasureSpec,
    EXACTLY,
    AT_MOST,
    UNSPECIFIED,
    LayoutParams,
    MATCH_PARENT,
    WRAP_CONTENT,
    View,
    ViewGroup,
    TextView,
    ImageView,
    VectorDrawable,
    BitmapDrawable,
    ConstraintLayout,
    FrameLayout,
    LayoutInflater
} from '../src/view_hierarchy.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

let totalTests = 0;
let passedTests = 0;

function test(name, fn) {
    totalTests++;
    try {
        fn();
        passedTests++;
        console.log(`  ✔ [PASS] ${name}`);
    } catch (err) {
        console.error(`  ✖ [FAIL] ${name}:`, err.message);
        throw err;
    }
}

async function runTestSuite() {
    console.log('======================================================');
    console.log('▶ Milestone 2: ConstraintLayout Bounds & Vector Engine');
    console.log('======================================================\n');

    // -------------------------------------------------------------------------
    // Test 1: Authentic F-Droid.apk res/Kt.xml inflation and bounds validation
    // -------------------------------------------------------------------------
    console.log('Test Suite 1: Authentic F-Droid.apk Layout Inflation & Bounds');
    const apkPath = path.join(rootDir, 'F-Droid.apk');
    assert.ok(fs.existsSync(apkPath), 'F-Droid.apk must exist in root directory');

    const apkBuf = fs.readFileSync(apkPath);
    const zip = new ApkZipReader(apkBuf.buffer.slice(apkBuf.byteOffset, apkBuf.byteOffset + apkBuf.byteLength));
    zip.readEntries();

    const arscBuf = zip.getFile('resources.arsc');
    assert.ok(arscBuf, 'resources.arsc must exist in F-Droid.apk');
    const arsc = ArscDecoder.decode(arscBuf);

    const ktBuf = zip.getFile('res/Kt.xml');
    assert.ok(ktBuf, 'res/Kt.xml (app_list_item) must exist in F-Droid.apk');

    const root = LayoutInflater.inflate(ktBuf, arsc);
    assert.ok(root instanceof ConstraintLayout, 'Root view must be an instance of ConstraintLayout');

    const widthSpec = MeasureSpec.makeMeasureSpec(720, EXACTLY);
    const heightSpec = MeasureSpec.makeMeasureSpec(120, EXACTLY);

    root.measure(widthSpec, heightSpec);
    root.layout(0, 0, 720, 120);

    test('res/Kt.xml inflates child with id 2131296365 (app_name)', () => {
        const nameTv = root.findViewById(2131296365);
        assert.ok(nameTv, 'Child view with id 2131296365 must exist');
        assert.ok(nameTv instanceof TextView, 'Child view 2131296365 must be TextView');
    });

    test('findViewById(2131296365) has left < right and width > 100', () => {
        const nameTv = root.findViewById(2131296365);
        assert.ok(nameTv.left < nameTv.right, `left (${nameTv.left}) must be strictly less than right (${nameTv.right})`);
        assert.ok(nameTv.getWidth() > 100, `width (${nameTv.getWidth()}) must be greater than 100px`);
        assert.ok(nameTv.measuredWidth > 100, `measuredWidth (${nameTv.measuredWidth}) must be greater than 100px`);
    });

    test('Sibling Button container (2131296409) resolves to right edge', () => {
        const btnContainer = root.findViewById(2131296409);
        assert.ok(btnContainer, 'Button container 2131296409 must exist');
        assert.ok(btnContainer.right <= 720, 'Button container right boundary <= 720');
        assert.ok(btnContainer.left >= 500, `Button container left boundary (${btnContainer.left}) >= 500`);
    });

    test('App icon (2131296574) resolves to left edge', () => {
        const iconView = root.findViewById(2131296574);
        assert.ok(iconView, 'Icon view 2131296574 must exist');
        assert.strictEqual(iconView.left, 16);
        assert.strictEqual(iconView.getWidth(), 48);
    });

    // -------------------------------------------------------------------------
    // Test 2: ViewGroup.getChildMeasureSpec with 0dp (MATCH_CONSTRAINT)
    // -------------------------------------------------------------------------
    console.log('\nTest Suite 2: ViewGroup.getChildMeasureSpec 0dp MATCH_CONSTRAINT');

    test('getChildMeasureSpec under EXACTLY allows flexible AT_MOST for 0dp', () => {
        const parentSpec = MeasureSpec.makeMeasureSpec(500, EXACTLY);
        const childSpec = ViewGroup.getChildMeasureSpec(parentSpec, 20, 0);
        assert.strictEqual(MeasureSpec.getMode(childSpec), AT_MOST);
        assert.strictEqual(MeasureSpec.getSize(childSpec), 480);
    });

    test('getChildMeasureSpec under AT_MOST allows flexible AT_MOST for 0dp', () => {
        const parentSpec = MeasureSpec.makeMeasureSpec(500, AT_MOST);
        const childSpec = ViewGroup.getChildMeasureSpec(parentSpec, 20, 0);
        assert.strictEqual(MeasureSpec.getMode(childSpec), AT_MOST);
        assert.strictEqual(MeasureSpec.getSize(childSpec), 480);
    });

    test('getChildMeasureSpec under UNSPECIFIED returns UNSPECIFIED 0 for 0dp', () => {
        const parentSpec = MeasureSpec.makeMeasureSpec(0, UNSPECIFIED);
        const childSpec = ViewGroup.getChildMeasureSpec(parentSpec, 20, 0);
        assert.strictEqual(MeasureSpec.getMode(childSpec), UNSPECIFIED);
        assert.strictEqual(MeasureSpec.getSize(childSpec), 0);
    });

    test('getChildMeasureSpec preserves explicit positive dimensions', () => {
        const parentSpec = MeasureSpec.makeMeasureSpec(500, EXACTLY);
        const childSpec = ViewGroup.getChildMeasureSpec(parentSpec, 20, 150);
        assert.strictEqual(MeasureSpec.getMode(childSpec), EXACTLY);
        assert.strictEqual(MeasureSpec.getSize(childSpec), 150);
    });

    // -------------------------------------------------------------------------
    // Test 3: ConstraintLayout 2-pass onMeasure & 3-pass onLayout Convergence
    // -------------------------------------------------------------------------
    console.log('\nTest Suite 3: 2-Pass onMeasure & Sibling Dependency Convergence');

    test('ConstraintLayout resolves forward and backward sibling dependencies', () => {
        const layout = new ConstraintLayout();
        layout.setPadding(0, 0, 0, 0);

        // Child A (id 1): MATCH_CONSTRAINT (0dp), startToStart parent, endToStart Child B
        const lpA = new LayoutParams(0, 50);
        lpA.constraints = {
            layout_constraintStart_toStartOf: 'parent',
            layout_constraintEnd_toStartOf: 2
        };
        const viewA = new View(lpA);
        viewA.id = 1;

        // Child B (id 2): 100px fixed, endToEnd parent
        const lpB = new LayoutParams(100, 50);
        lpB.constraints = {
            layout_constraintEnd_toEndOf: 'parent'
        };
        const viewB = new View(lpB);
        viewB.id = 2;

        layout.addView(viewA);
        layout.addView(viewB);

        layout.measure(MeasureSpec.makeMeasureSpec(800, EXACTLY), MeasureSpec.makeMeasureSpec(200, EXACTLY));
        layout.layout(0, 0, 800, 200);

        assert.strictEqual(viewB.left, 700);
        assert.strictEqual(viewB.right, 800);
        assert.strictEqual(viewB.getWidth(), 100);

        assert.strictEqual(viewA.left, 0);
        assert.strictEqual(viewA.right, 700);
        assert.strictEqual(viewA.getWidth(), 700);
    });

    // -------------------------------------------------------------------------
    // Test 4: VectorDrawable & BitmapDrawable in ImageView
    // -------------------------------------------------------------------------
    console.log('\nTest Suite 4: VectorDrawable & BitmapDrawable in ImageView');

    test('VectorDrawable parses and draws SVG path data', () => {
        const paths = [{
            pathData: 'M0,0 L10,0 L10,10 L0,10 Z',
            fillColor: '#FF0000',
            strokeColor: '#000000',
            strokeWidth: 1
        }];
        const vd = new VectorDrawable(24, 24, 24, 24, paths);
        assert.strictEqual(vd.width, 24);
        assert.strictEqual(vd.height, 24);

        const iv = new ImageView();
        iv.setDrawable(vd);
        assert.strictEqual(iv.intrinsicWidth, 24);
        assert.strictEqual(iv.intrinsicHeight, 24);

        let fillCalls = 0;
        const mockCtx = {
            save() {},
            restore() {},
            translate() {},
            scale() {},
            fill() { fillCalls++; },
            stroke() {}
        };

        iv.onDraw(mockCtx);
        assert.ok(fillCalls >= 1, 'VectorDrawable must execute fill path on mock canvas context');
    });

    test('ArscResourceTable resolves real APK vector drawable paths', () => {
        const drawablePath = arsc.resolveDrawablePath('ic_launcher') || arsc.resolveDrawablePath(0x7f0800a0);
        assert.ok(drawablePath, 'Resource resolver must resolve vector drawable path');
        assert.ok(drawablePath.endsWith('.xml') || drawablePath.endsWith('.png'), 'Drawable path must end in .xml or .png');

        const drawableObj = arsc.resolveDrawable(0x7f0800a0, zip);
        assert.ok(drawableObj, 'resolveDrawable must return descriptor object');
        assert.ok(['vector', 'bitmap', 'color'].includes(drawableObj.type), 'Drawable type must be vector, bitmap, or color');
    });

    console.log('\n======================================================');
    console.log(`⚡ ALL MILESTONE 2 TESTS PASSED! (${passedTests}/${totalTests} passed)`);
    console.log('======================================================');
}

runTestSuite().catch(err => {
    console.error('Test suite failed:', err);
    process.exit(1);
});
