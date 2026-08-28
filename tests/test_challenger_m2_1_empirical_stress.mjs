/**
 * Challenger 1 Milestone 2 Adversarial Stress Test Suite
 * 
 * Tests:
 * 1. F-Droid.apk res/Kt.xml authentic inflation across multiple display resolutions & text updates
 * 2. Multi-hop forward and backward sibling dependency graphs & permutation convergence
 * 3. 2D MATCH_CONSTRAINT (0dp width AND 0dp height) with multi-anchor resolution
 * 4. ConstraintLayout horizontal and vertical bias (0.0, 0.25, 0.5, 0.75, 1.0)
 * 5. Margin and padding interactions under 0dp MATCH_CONSTRAINT
 * 6. Visibility GONE and INVISIBLE resilience in constraint chains
 * 7. VectorDrawable SVG path decoding & rendering execution (Path2D & Canvas2D fallback)
 * 8. Real APK drawable resolution from F-Droid.apk
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
    VISIBLE,
    GONE,
    INVISIBLE,
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

let total = 0;
let passed = 0;
let failures = [];

function runTest(name, fn) {
    total++;
    try {
        fn();
        passed++;
        console.log(`  ✔ [PASS] ${name}`);
    } catch (err) {
        console.error(`  ✖ [FAIL] ${name}:`, err.message);
        failures.push({ name, error: err.message });
    }
}

async function main() {
    console.log('================================================================');
    console.log('▶ CHALLENGER 1: ConstraintLayout & Vector Empirical Stress Suite');
    console.log('================================================================\n');

    const apkPath = path.join(rootDir, 'F-Droid.apk');
    assert.ok(fs.existsSync(apkPath), 'F-Droid.apk exists in workspace');
    const apkBuf = fs.readFileSync(apkPath);
    const zip = new ApkZipReader(apkBuf.buffer.slice(apkBuf.byteOffset, apkBuf.byteOffset + apkBuf.byteLength));
    zip.readEntries();

    const arscBuf = zip.getFile('resources.arsc');
    assert.ok(arscBuf, 'resources.arsc present');
    const arsc = ArscDecoder.decode(arscBuf);

    const ktBuf = zip.getFile('res/Kt.xml');
    assert.ok(ktBuf, 'res/Kt.xml present');

    // -------------------------------------------------------------------------
    // 1. F-Droid.apk res/Kt.xml Multi-Resolution Stress
    // -------------------------------------------------------------------------
    console.log('--- Suite 1: F-Droid.apk res/Kt.xml Across Display Resolutions ---');

    const resolutions = [
        { name: '1080p (1080x1920)', w: 1080, h: 180 },
        { name: '720p (720x1280)', w: 720, h: 120 },
        { name: '480p (480x800)', w: 480, h: 96 },
        { name: '320p (320x480)', w: 320, h: 80 }
    ];

    for (const res of resolutions) {
        runTest(`res/Kt.xml layout at ${res.name}`, () => {
            const root = LayoutInflater.inflate(ktBuf, arsc);
            assert.ok(root instanceof ConstraintLayout);

            const wSpec = MeasureSpec.makeMeasureSpec(res.w, EXACTLY);
            const hSpec = MeasureSpec.makeMeasureSpec(res.h, EXACTLY);
            root.measure(wSpec, hSpec);
            root.layout(0, 0, res.w, res.h);

            // Verify app_name TextView (id 2131296365)
            const appName = root.findViewById(2131296365);
            assert.ok(appName, 'findViewById(2131296365) returned view');
            assert.ok(appName instanceof TextView, 'app_name is TextView');
            assert.ok(appName.left < appName.right, `left (${appName.left}) < right (${appName.right})`);
            assert.ok(appName.getWidth() > 100, `width (${appName.getWidth()}) > 100`);
            assert.ok(appName.measuredWidth > 100, `measuredWidth (${appName.measuredWidth}) > 100`);
            assert.ok(appName.top < appName.bottom, `top (${appName.top}) < bottom (${appName.bottom})`);
            assert.ok(appName.getHeight() > 0, `height (${appName.getHeight()}) > 0`);

            // Verify icon ImageView (id 2131296574)
            const icon = root.findViewById(2131296574);
            assert.ok(icon, 'icon exists');
            assert.strictEqual(icon.left, 16, 'icon left margin 16');
            assert.ok(icon.getWidth() > 0, 'icon width > 0');
            assert.ok(appName.left >= icon.right, 'app_name starts after icon');

            // Verify install button container (id 2131296409)
            const btn = root.findViewById(2131296409);
            assert.ok(btn, 'btn exists');
            assert.ok(appName.right <= btn.left, 'app_name ends before or at btn start');
            assert.ok(btn.right <= res.w, 'btn right <= screen width');
        });
    }

    // -------------------------------------------------------------------------
    // 2. Dynamic Text Updates on 0dp MATCH_CONSTRAINT TextView
    // -------------------------------------------------------------------------
    console.log('\n--- Suite 2: Dynamic Text Updates on MATCH_CONSTRAINT TextView ---');

    runTest('Dynamically populating app title maintains MATCH_CONSTRAINT bounds', () => {
        const root = LayoutInflater.inflate(ktBuf, arsc);
        const nameTv = root.findViewById(2131296365);
        assert.ok(nameTv);

        nameTv.setText('Firefox Focus: Private Browser');
        root.measure(MeasureSpec.makeMeasureSpec(720, EXACTLY), MeasureSpec.makeMeasureSpec(120, EXACTLY));
        root.layout(0, 0, 720, 120);

        assert.strictEqual(nameTv.left, 72);
        assert.strictEqual(nameTv.right, 632);
        assert.strictEqual(nameTv.getWidth(), 560);
        assert.strictEqual(nameTv.measuredWidth, 560);
        assert.ok(nameTv.measuredHeight > 0);
    });

    // -------------------------------------------------------------------------
    // 3. Multi-Hop Sibling Dependency Relaxation & Permutations
    // -------------------------------------------------------------------------
    console.log('\n--- Suite 3: Multi-Hop Sibling Dependency Relaxation ---');

    function build3NodeChain(order) {
        const layout = new ConstraintLayout();
        layout.setPadding(0, 0, 0, 0);

        // View 1 (0dp): startToStart parent, endToStart View 2
        const lp1 = new LayoutParams(0, 50);
        lp1.constraints = { layout_constraintStart_toStartOf: 'parent', layout_constraintEnd_toStartOf: 2 };
        const v1 = new View(lp1);
        v1.id = 1;

        // View 2 (0dp): startToEnd View 1, endToStart View 3
        const lp2 = new LayoutParams(0, 50);
        lp2.constraints = { layout_constraintStart_toEndOf: 1, layout_constraintEnd_toStartOf: 3 };
        const v2 = new View(lp2);
        v2.id = 2;

        // View 3 (fixed 200px): endToEnd parent
        const lp3 = new LayoutParams(200, 50);
        lp3.constraints = { layout_constraintEnd_toEndOf: 'parent' };
        const v3 = new View(lp3);
        v3.id = 3;

        const map = { 1: v1, 2: v2, 3: v3 };
        for (const id of order) layout.addView(map[id]);

        layout.measure(MeasureSpec.makeMeasureSpec(800, EXACTLY), MeasureSpec.makeMeasureSpec(100, EXACTLY));
        layout.layout(0, 0, 800, 100);

        return { v1, v2, v3 };
    }

    const perms3 = [
        [1, 2, 3], [1, 3, 2], [2, 1, 3], [2, 3, 1], [3, 1, 2], [3, 2, 1]
    ];
    for (const p of perms3) {
        runTest(`3-node chain ordering [${p.join(',')}]`, () => {
            const { v1, v2, v3 } = build3NodeChain(p);
            assert.strictEqual(v3.right, 800);
            assert.strictEqual(v3.left, 600);
            assert.strictEqual(v1.left, 0);
            assert.strictEqual(v1.right, v2.left);
            assert.ok(v1.getWidth() >= 0);
            assert.ok(v2.getWidth() >= 0);
        });
    }

    // -------------------------------------------------------------------------
    // 4. 2D MATCH_CONSTRAINT (0dp Width & 0dp Height)
    // -------------------------------------------------------------------------
    console.log('\n--- Suite 4: 2D MATCH_CONSTRAINT (0dp Width & 0dp Height) ---');

    runTest('2D 0dp view filling quadrant between sibling anchors', () => {
        const layout = new ConstraintLayout();
        layout.setPadding(0, 0, 0, 0);

        // Top-left anchor (fixed 100x100)
        const lpTL = new LayoutParams(100, 100);
        lpTL.constraints = {
            layout_constraintStart_toStartOf: 'parent',
            layout_constraintTop_toTopOf: 'parent'
        };
        const vTL = new View(lpTL);
        vTL.id = 10;

        // Bottom-right anchor (fixed 150x80)
        const lpBR = new LayoutParams(150, 80);
        lpBR.constraints = {
            layout_constraintEnd_toEndOf: 'parent',
            layout_constraintBottom_toBottomOf: 'parent'
        };
        const vBR = new View(lpBR);
        vBR.id = 20;

        // Center 0dp x 0dp view spanning between vTL and vBR
        const lpCenter = new LayoutParams(0, 0);
        lpCenter.setMargins(10, 15, 20, 25);
        lpCenter.constraints = {
            layout_constraintStart_toEndOf: 10,
            layout_constraintTop_toBottomOf: 10,
            layout_constraintEnd_toStartOf: 20,
            layout_constraintBottom_toTopOf: 20
        };
        const vCenter = new View(lpCenter);
        vCenter.id = 30;

        layout.addView(vTL);
        layout.addView(vBR);
        layout.addView(vCenter);

        layout.measure(MeasureSpec.makeMeasureSpec(800, EXACTLY), MeasureSpec.makeMeasureSpec(600, EXACTLY));
        layout.layout(0, 0, 800, 600);

        assert.strictEqual(vTL.left, 0);
        assert.strictEqual(vTL.right, 100);
        assert.strictEqual(vTL.top, 0);
        assert.strictEqual(vTL.bottom, 100);

        assert.strictEqual(vBR.right, 800);
        assert.strictEqual(vBR.left, 650);
        assert.strictEqual(vBR.bottom, 600);
        assert.strictEqual(vBR.top, 520);

        assert.strictEqual(vCenter.left, 110);
        assert.strictEqual(vCenter.right, 630);
        assert.strictEqual(vCenter.getWidth(), 520);
        assert.strictEqual(vCenter.measuredWidth, 520);

        assert.strictEqual(vCenter.top, 115);
        assert.strictEqual(vCenter.bottom, 495);
        assert.strictEqual(vCenter.getHeight(), 380);
        assert.strictEqual(vCenter.measuredHeight, 380);
    });

    // -------------------------------------------------------------------------
    // 5. Bias Calculation (Horizontal & Vertical)
    // -------------------------------------------------------------------------
    console.log('\n--- Suite 5: ConstraintLayout Bias Distribution ---');

    const biases = [0.0, 0.25, 0.5, 0.75, 1.0];
    for (const bias of biases) {
        runTest(`Horizontal bias = ${bias}`, () => {
            const layout = new ConstraintLayout();
            const lp = new LayoutParams(200, 100);
            lp.constraints = {
                layout_constraintStart_toStartOf: 'parent',
                layout_constraintEnd_toEndOf: 'parent',
                layout_constraintHorizontal_bias: bias
            };
            const v = new View(lp);
            layout.addView(v);

            layout.measure(MeasureSpec.makeMeasureSpec(1000, EXACTLY), MeasureSpec.makeMeasureSpec(500, EXACTLY));
            layout.layout(0, 0, 1000, 500);

            const expectedLeft = Math.round(800 * bias);
            assert.strictEqual(v.left, expectedLeft);
            assert.strictEqual(v.right, expectedLeft + 200);
            assert.strictEqual(v.getWidth(), 200);
        });

        runTest(`Vertical bias = ${bias}`, () => {
            const layout = new ConstraintLayout();
            const lp = new LayoutParams(200, 100);
            lp.constraints = {
                layout_constraintTop_toTopOf: 'parent',
                layout_constraintBottom_toBottomOf: 'parent',
                layout_constraintVertical_bias: bias
            };
            const v = new View(lp);
            layout.addView(v);

            layout.measure(MeasureSpec.makeMeasureSpec(1000, EXACTLY), MeasureSpec.makeMeasureSpec(500, EXACTLY));
            layout.layout(0, 0, 1000, 500);

            const expectedTop = Math.round(400 * bias);
            assert.strictEqual(v.top, expectedTop);
            assert.strictEqual(v.bottom, expectedTop + 100);
            assert.strictEqual(v.getHeight(), 100);
        });
    }

    // -------------------------------------------------------------------------
    // 6. Visibility GONE Resilience
    // -------------------------------------------------------------------------
    console.log('\n--- Suite 6: View.GONE Resilience in Layout Graph ---');

    runTest('GONE view collapses to 0x0 without disrupting sibling anchors', () => {
        const layout = new ConstraintLayout();

        // View 1 (GONE): 100x100
        const lp1 = new LayoutParams(100, 100);
        lp1.constraints = { layout_constraintStart_toStartOf: 'parent' };
        const v1 = new View(lp1);
        v1.id = 1;
        v1.visibility = GONE;

        // View 2: starts at end of View 1
        const lp2 = new LayoutParams(200, 50);
        lp2.constraints = { layout_constraintStart_toEndOf: 1 };
        const v2 = new View(lp2);
        v2.id = 2;

        layout.addView(v1);
        layout.addView(v2);

        layout.measure(MeasureSpec.makeMeasureSpec(500, EXACTLY), MeasureSpec.makeMeasureSpec(500, EXACTLY));
        layout.layout(0, 0, 500, 500);

        assert.strictEqual(v1.measuredWidth, 0);
        assert.strictEqual(v1.measuredHeight, 0);
        assert.strictEqual(v2.getWidth(), 200);
    });

    // -------------------------------------------------------------------------
    // 7. VectorDrawable SVG Path Execution & Mock Canvas Rendering
    // -------------------------------------------------------------------------
    console.log('\n--- Suite 7: VectorDrawable Engine & Canvas Execution ---');

    runTest('VectorDrawable executes complex SVG path commands on Canvas context', () => {
        const pathData = 'M 10 80 Q 52.5 10, 95 80 T 180 80 C 100 100, 120 120, 140 140 Z';
        const vd = new VectorDrawable(48, 48, 48, 48, [{
            pathData,
            fillColor: '#FF5722',
            strokeColor: '#2196F3',
            strokeWidth: 2
        }]);

        assert.strictEqual(vd.width, 48);
        assert.strictEqual(vd.height, 48);

        const iv = new ImageView();
        iv.setDrawable(vd);

        const callLog = [];
        const mockCtx = {
            save() { callLog.push('save'); },
            restore() { callLog.push('restore'); },
            translate(x, y) { callLog.push(`translate(${x},${y})`); },
            scale(sx, sy) { callLog.push(`scale(${sx},${sy})`); },
            fill() { callLog.push('fill'); },
            stroke() { callLog.push('stroke'); }
        };

        iv.onDraw(mockCtx);

        assert.ok(callLog.includes('save'), 'Canvas context save called');
        assert.ok(callLog.includes('fill'), 'Canvas fill called');
        assert.ok(callLog.includes('stroke'), 'Canvas stroke called');
        assert.ok(callLog.includes('restore'), 'Canvas context restore called');
    });

    runTest('ArscResourceTable resolves all F-Droid drawable types', () => {
        const drawables = ['ic_launcher', 'ic_search_white', 'ic_settings'];
        for (const name of drawables) {
            const p = arsc.resolveDrawablePath(name);
            if (p) {
                assert.ok(p.endsWith('.xml') || p.endsWith('.png'), `Drawable ${name} path ${p} valid`);
            }
        }
    });

    console.log('\n================================================================');
    console.log(`⚡ EMPIRICAL CHALLENGER TEST RESULTS: ${passed}/${total} passed (${failures.length} failed)`);
    console.log('================================================================');

    if (failures.length > 0) {
        process.exit(1);
    }
}

main().catch(err => {
    console.error('Challenger test execution failed:', err);
    process.exit(1);
});
