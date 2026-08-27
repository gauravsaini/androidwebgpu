/**
 * Milestone M1 Challenger 2 Empirical Stress Test Suite:
 * Deep Hierarchy, Coordinate Systems, Container Solvers, & Reverse-Z Hit Testing
 * 
 * Conforms to ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { ApkZipReader } from '../src/apk_client_parser.js';
import {
    ArscDecoder,
    ArscResourceTable,
    TypedValue,
    TYPE_DIMENSION,
    TYPE_INT_DEC,
    TYPE_INT_COLOR_ARGB8,
    TYPE_INT_BOOLEAN,
    COMPLEX_UNIT_PX,
    COMPLEX_UNIT_DIP,
    COMPLEX_UNIT_SP,
    COMPLEX_UNIT_PT,
    COMPLEX_UNIT_IN,
    COMPLEX_UNIT_MM
} from '../src/apk_resource_resolver.js';
import {
    MeasureSpec,
    UNSPECIFIED,
    EXACTLY,
    AT_MOST,
    LayoutParams,
    MATCH_PARENT,
    WRAP_CONTENT,
    View,
    ViewGroup,
    VISIBLE,
    INVISIBLE,
    GONE,
    FrameLayout,
    LinearLayout,
    HORIZONTAL,
    VERTICAL,
    RelativeLayout,
    ConstraintLayout,
    ScrollView,
    RecyclerView,
    TextView,
    ImageView,
    Button,
    LayoutInflater
} from '../src/view_hierarchy.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

const results = [];

function recordTest(suiteName, testName, passed, error = null, details = {}) {
    results.push({ suiteName, testName, passed, error: error ? error.message : null, details });
    const mark = passed ? '✔ [PASS]' : '✖ [FAIL]';
    console.log(`  ${mark} [${suiteName}] ${testName}`);
    if (!passed && error) {
        console.error(`     Error: ${error.message}`);
        if (details && Object.keys(details).length > 0) {
            console.error(`     Details: ${JSON.stringify(details)}`);
        }
    }
}

async function runSuite(title, fn) {
    console.log(`\n================================================================================`);
    console.log(`▶ [CHALLENGER-M1-2] ${title}`);
    console.log(`================================================================================`);
    const t0 = performance.now();
    try {
        await fn();
    } catch (err) {
        console.error(`Suite exception in "${title}":`, err);
    }
    const dt = (performance.now() - t0).toFixed(2);
    console.log(`✔ Suite Finished (${dt}ms)`);
}

async function main() {
    console.log("⚡ STARTING CHALLENGER 2 EMPIRICAL STRESS HARNESS FOR M1 VIEW HIERARCHY...\n");

    // =========================================================================
    // Suite 1: Deep Hierarchy Scaling & Coordinate System Integrity (Depth 10..50)
    // =========================================================================
    await runSuite("1. Deep Hierarchy Scaling & Coordinate Propagation (Depth 10..50)", async () => {
        for (const targetDepth of [10, 20, 50]) {
            let root = new FrameLayout(new LayoutParams(MATCH_PARENT, MATCH_PARENT));
            root.id = 1000;
            let current = root;

            for (let d = 1; d < targetDepth; d++) {
                const child = new FrameLayout(new LayoutParams(MATCH_PARENT, MATCH_PARENT));
                child.id = 1000 + d;
                child.setPadding(2, 2, 2, 2);
                current.addView(child);
                current = child;
            }

            const leafBtn = new Button("Leaf", new LayoutParams(100, 40));
            leafBtn.id = 9999;
            let leafClicked = false;
            leafBtn.setOnClickListener(() => { leafClicked = true; });
            current.addView(leafBtn);

            // 1.1 Depth verification
            try {
                let measuredDepth = 0;
                let cur = leafBtn;
                while (cur) { measuredDepth++; cur = cur.parent; }
                const pass = (measuredDepth === targetDepth + 1);
                recordTest("Deep Hierarchy", `Tree depth verification (depth=${targetDepth})`, pass, null, { measuredDepth, expected: targetDepth + 1 });
            } catch (e) { recordTest("Deep Hierarchy", `Tree depth verification (depth=${targetDepth})`, false, e); }

            // 1.2 getRootView
            try {
                const pass = (leafBtn.getRootView() === root);
                recordTest("Deep Hierarchy", `getRootView from leaf (depth=${targetDepth})`, pass);
            } catch (e) { recordTest("Deep Hierarchy", `getRootView from leaf (depth=${targetDepth})`, false, e); }

            // 1.3 findViewById from root
            try {
                const found = root.findViewById(9999);
                recordTest("Deep Hierarchy", `findViewById leaf across depth=${targetDepth}`, found === leafBtn);
            } catch (e) { recordTest("Deep Hierarchy", `findViewById leaf across depth=${targetDepth}`, false, e); }

            // 1.4 Measure
            try {
                root.measure(MeasureSpec.makeMeasureSpec(800, EXACTLY), MeasureSpec.makeMeasureSpec(1200, EXACTLY));
                const pass = (root.measuredWidth === 800 && root.measuredHeight === 1200 && leafBtn.measuredWidth === 100 && leafBtn.measuredHeight === 40);
                recordTest("Deep Hierarchy", `Measure propagation through depth=${targetDepth}`, pass, null, {
                    rootW: root.measuredWidth, rootH: root.measuredHeight, leafW: leafBtn.measuredWidth, leafH: leafBtn.measuredHeight
                });
            } catch (e) { recordTest("Deep Hierarchy", `Measure propagation through depth=${targetDepth}`, false, e); }

            // 1.5 Layout & Screen Coordinates (Expected: (targetDepth - 1) * 2)
            try {
                root.layout(0, 0, 800, 1200);
                const screenLoc = leafBtn.getLocationOnScreen();
                const expectedScreenOffset = (targetDepth - 1) * 2;
                const pass = (screenLoc.x === expectedScreenOffset && screenLoc.y === expectedScreenOffset);
                recordTest("Deep Hierarchy", `getLocationOnScreen coordinate accuracy (depth=${targetDepth})`, pass,
                    pass ? null : new Error(`Expected (${expectedScreenOffset}, ${expectedScreenOffset}) but got (${screenLoc.x}, ${screenLoc.y})`),
                    { actual: screenLoc, expected: { x: expectedScreenOffset, y: expectedScreenOffset } });
            } catch (e) { recordTest("Deep Hierarchy", `getLocationOnScreen coordinate accuracy (depth=${targetDepth})`, false, e); }

            // 1.6 Touch Event Routing down depth
            try {
                const expectedScreenOffset = (targetDepth - 1) * 2;
                leafClicked = false;
                const down = root.dispatchTouchEvent({ action: 0, x: expectedScreenOffset + 10, y: expectedScreenOffset + 10 });
                const up = root.dispatchTouchEvent({ action: 1, x: expectedScreenOffset + 10, y: expectedScreenOffset + 10 });
                const pass = (down === true && up === true && leafClicked === true);
                recordTest("Deep Hierarchy", `dispatchTouchEvent touch delivery to leaf (depth=${targetDepth})`, pass,
                    pass ? null : new Error(`Touch event failed to reach leaf button: down=${down}, up=${up}, clicked=${leafClicked}`),
                    { down, up, leafClicked });
            } catch (e) { recordTest("Deep Hierarchy", `dispatchTouchEvent touch delivery to leaf (depth=${targetDepth})`, false, e); }
        }
    });

    // =========================================================================
    // Suite 2: Nested Container Coordinate Relativity (Depth = 2)
    // =========================================================================
    await runSuite("2. Nested Container Local Coordinate Relativity", async () => {
        // Parent at (50, 50, 250, 250) with child at (10, 10, 110, 60) relative to parent
        const root = new FrameLayout(new LayoutParams(500, 500));
        const parentContainer = new FrameLayout(new LayoutParams(200, 200));
        parentContainer.setPadding(20, 20, 20, 20);
        parentContainer.layoutParams.setMargins(50, 50, 0, 0);

        const childBtn = new Button("Child", new LayoutParams(100, 50));
        let clicked = false;
        childBtn.setOnClickListener(() => { clicked = true; });
        parentContainer.addView(childBtn);
        root.addView(parentContainer);

        root.measure(MeasureSpec.makeMeasureSpec(500, EXACTLY), MeasureSpec.makeMeasureSpec(500, EXACTLY));
        root.layout(0, 0, 500, 500);

        // In standard view tree:
        // parentContainer: left=50, top=50, right=250, bottom=250
        // childBtn: left=20 (paddingLeft), top=20 (paddingTop), right=120, bottom=70 relative to parentContainer
        try {
            const passParent = (parentContainer.left === 50 && parentContainer.top === 50);
            recordTest("Nested Coordinates", "Parent container positioned at margin offset (50, 50)", passParent,
                passParent ? null : new Error(`Parent at (${parentContainer.left}, ${parentContainer.top})`));

            const passChildRelative = (childBtn.left === 20 && childBtn.top === 20);
            recordTest("Nested Coordinates", "Child view left/top is relative to parent origin (20, 20)", passChildRelative,
                passChildRelative ? null : new Error(`Child view left=${childBtn.left}, top=${childBtn.top}, expected (20, 20)`),
                { childLeft: childBtn.left, childTop: childBtn.top });

            const screenLoc = childBtn.getLocationOnScreen();
            const passScreen = (screenLoc.x === 70 && screenLoc.y === 70);
            recordTest("Nested Coordinates", "Child getLocationOnScreen equals parent.left + child.left (70, 70)", passScreen,
                passScreen ? null : new Error(`Child screen location is (${screenLoc.x}, ${screenLoc.y}), expected (70, 70)`),
                { screenLoc });

            // Touch at screen coordinate (75, 75)
            clicked = false;
            const down = root.dispatchTouchEvent({ action: 0, x: 75, y: 75 });
            const up = root.dispatchTouchEvent({ action: 1, x: 75, y: 75 });
            const passTouch = (down === true && up === true && clicked === true);
            recordTest("Nested Coordinates", "Touch event at screen (75, 75) delivers to child button in nested container", passTouch,
                passTouch ? null : new Error(`down=${down}, up=${up}, clicked=${clicked}`),
                { down, up, clicked });
        } catch (e) { recordTest("Nested Coordinates", "Nested container coordinate relativity test", false, e); }
    });

    // =========================================================================
    // Suite 3: MeasureSpec Matrix & getChildMeasureSpec Exhaustive Matrix
    // =========================================================================
    await runSuite("3. MeasureSpec Matrix & getChildMeasureSpec Matrix", async () => {
        const sizes = [0, 1, 16, 360, 640, 1080, 1920, 2560, 3840, 0x3FFFFFFF];
        const modes = [UNSPECIFIED, EXACTLY, AT_MOST];

        try {
            let allPassed = true;
            for (const mode of modes) {
                for (const size of sizes) {
                    const spec = MeasureSpec.makeMeasureSpec(size, mode);
                    if (MeasureSpec.getMode(spec) !== mode || MeasureSpec.getSize(spec) !== size) {
                        allPassed = false;
                    }
                }
            }
            recordTest("MeasureSpec", "30-element mode-size bitmask packing/unpacking matrix", allPassed);
        } catch (e) { recordTest("MeasureSpec", "30-element mode-size bitmask matrix", false, e); }

        const parentSize = 400;
        const padding = 50;
        const avail = 350;

        try {
            const exSpec = MeasureSpec.makeMeasureSpec(parentSize, EXACTLY);
            const atSpec = MeasureSpec.makeMeasureSpec(parentSize, AT_MOST);
            const unSpec = MeasureSpec.makeMeasureSpec(0, UNSPECIFIED);

            const m1 = ViewGroup.getChildMeasureSpec(exSpec, padding, MATCH_PARENT);
            const m2 = ViewGroup.getChildMeasureSpec(exSpec, padding, WRAP_CONTENT);
            const m3 = ViewGroup.getChildMeasureSpec(exSpec, padding, 120);

            const passEx = (MeasureSpec.getMode(m1) === EXACTLY && MeasureSpec.getSize(m1) === avail) &&
                           (MeasureSpec.getMode(m2) === AT_MOST && MeasureSpec.getSize(m2) === avail) &&
                           (MeasureSpec.getMode(m3) === EXACTLY && MeasureSpec.getSize(m3) === 120);
            recordTest("MeasureSpec", "getChildMeasureSpec under Parent EXACTLY", passEx);

            const a1 = ViewGroup.getChildMeasureSpec(atSpec, padding, MATCH_PARENT);
            const a2 = ViewGroup.getChildMeasureSpec(atSpec, padding, WRAP_CONTENT);
            const a3 = ViewGroup.getChildMeasureSpec(atSpec, padding, 80);

            const passAt = (MeasureSpec.getMode(a1) === AT_MOST && MeasureSpec.getSize(a1) === avail) &&
                           (MeasureSpec.getMode(a2) === AT_MOST && MeasureSpec.getSize(a2) === avail) &&
                           (MeasureSpec.getMode(a3) === EXACTLY && MeasureSpec.getSize(a3) === 80);
            recordTest("MeasureSpec", "getChildMeasureSpec under Parent AT_MOST", passAt);

            const u1 = ViewGroup.getChildMeasureSpec(unSpec, padding, MATCH_PARENT);
            const u2 = ViewGroup.getChildMeasureSpec(unSpec, padding, WRAP_CONTENT);
            const u3 = ViewGroup.getChildMeasureSpec(unSpec, padding, 150);

            const passUn = (MeasureSpec.getMode(u1) === UNSPECIFIED && MeasureSpec.getSize(u1) === 0) &&
                           (MeasureSpec.getMode(u2) === UNSPECIFIED && MeasureSpec.getSize(u2) === 0) &&
                           (MeasureSpec.getMode(u3) === EXACTLY && MeasureSpec.getSize(u3) === 150);
            recordTest("MeasureSpec", "getChildMeasureSpec under Parent UNSPECIFIED", passUn);

            const excessSpec = ViewGroup.getChildMeasureSpec(exSpec, 500, MATCH_PARENT);
            recordTest("MeasureSpec", "getChildMeasureSpec clamping when padding > size", MeasureSpec.getSize(excessSpec) === 0);
        } catch (e) { recordTest("MeasureSpec", "getChildMeasureSpec permutations", false, e); }
    });

    // =========================================================================
    // Suite 4: LinearLayout Weights & Space Distribution
    // =========================================================================
    await runSuite("4. LinearLayout Layout Weights & Proportional Distribution", async () => {
        try {
            const vLayout = new LinearLayout(VERTICAL);
            vLayout.setPadding(10, 10, 10, 10);

            const c1 = new View(new LayoutParams(MATCH_PARENT, 0)); c1.layoutParams.weight = 1;
            const c2 = new View(new LayoutParams(MATCH_PARENT, 0)); c2.layoutParams.weight = 2;
            const c3 = new View(new LayoutParams(MATCH_PARENT, 0)); c3.layoutParams.weight = 1;

            vLayout.addView(c1); vLayout.addView(c2); vLayout.addView(c3);
            vLayout.measure(MeasureSpec.makeMeasureSpec(200, EXACTLY), MeasureSpec.makeMeasureSpec(400, EXACTLY));
            vLayout.layout(0, 0, 200, 400);

            const pass = (c1.measuredHeight === 95 && c2.measuredHeight === 190 && c3.measuredHeight === 95 &&
                          c1.top === 10 && c2.top === 105 && c3.top === 295);
            recordTest("LinearLayout", "Vertical weight distribution (weights: 1, 2, 1) in 400px", pass,
                pass ? null : new Error(`Heights: [${c1.measuredHeight}, ${c2.measuredHeight}, ${c3.measuredHeight}], Tops: [${c1.top}, ${c2.top}, ${c3.top}]`),
                { heights: [c1.measuredHeight, c2.measuredHeight, c3.measuredHeight], tops: [c1.top, c2.top, c3.top] });
        } catch (e) { recordTest("LinearLayout", "Vertical weight distribution", false, e); }

        try {
            const hLayout = new LinearLayout(HORIZONTAL);
            hLayout.weightSum = 5.0;
            const h1 = new View(new LayoutParams(0, MATCH_PARENT)); h1.layoutParams.weight = 1.0;
            const h2 = new View(new LayoutParams(0, MATCH_PARENT)); h2.layoutParams.weight = 1.0;
            hLayout.addView(h1); hLayout.addView(h2);

            hLayout.measure(MeasureSpec.makeMeasureSpec(500, EXACTLY), MeasureSpec.makeMeasureSpec(100, EXACTLY));
            const pass = (h1.measuredWidth === 100 && h2.measuredWidth === 100);
            recordTest("LinearLayout", "Horizontal explicit weightSum=5.0 distribution", pass,
                pass ? null : new Error(`Widths: [${h1.measuredWidth}, ${h2.measuredWidth}] expected [100, 100]`),
                { widths: [h1.measuredWidth, h2.measuredWidth] });
        } catch (e) { recordTest("LinearLayout", "Horizontal explicit weightSum", false, e); }

        try {
            const vGone = new LinearLayout(VERTICAL);
            const a1 = new View(new LayoutParams(MATCH_PARENT, 0)); a1.layoutParams.weight = 1;
            const g = new View(new LayoutParams(MATCH_PARENT, 100)); g.visibility = GONE; g.layoutParams.weight = 10;
            const a2 = new View(new LayoutParams(MATCH_PARENT, 0)); a2.layoutParams.weight = 1;
            vGone.addView(a1); vGone.addView(g); vGone.addView(a2);

            vGone.measure(MeasureSpec.makeMeasureSpec(200, EXACTLY), MeasureSpec.makeMeasureSpec(300, EXACTLY));
            vGone.layout(0, 0, 200, 300);

            const pass = (g.measuredHeight === 0 && a1.measuredHeight === 150 && a2.measuredHeight === 150 && a2.top === 150);
            recordTest("LinearLayout", "Visibility GONE child zero-weight & zero-space invariant", pass,
                pass ? null : new Error(`Heights: a1=${a1.measuredHeight}, g=${g.measuredHeight}, a2=${a2.measuredHeight}, top a2=${a2.top}`),
                { a1H: a1.measuredHeight, gH: g.measuredHeight, a2H: a2.measuredHeight, a2Top: a2.top });
        } catch (e) { recordTest("LinearLayout", "Visibility GONE child invariant", false, e); }
    });

    // =========================================================================
    // Suite 5: RelativeLayout Spatial Rules & Cyclic Dependency
    // =========================================================================
    await runSuite("5. RelativeLayout Spatial Rules & Cyclic Graph Solver", async () => {
        try {
            const rel = new RelativeLayout();
            rel.setPadding(5, 5, 5, 5);

            const header = new View(new LayoutParams(MATCH_PARENT, 50)); header.id = 101; header.layoutParams.alignRules.alignParentTop = true;
            const footer = new View(new LayoutParams(MATCH_PARENT, 60)); footer.id = 102; footer.layoutParams.alignRules.alignParentBottom = true;
            const sidebar = new View(new LayoutParams(80, MATCH_PARENT)); sidebar.id = 103;
            sidebar.layoutParams.alignRules.alignParentLeft = true; sidebar.layoutParams.alignRules.below = 101; sidebar.layoutParams.alignRules.above = 102;
            const content = new View(new LayoutParams(MATCH_PARENT, MATCH_PARENT)); content.id = 104;
            content.layoutParams.alignRules.toRightOf = 103; content.layoutParams.alignRules.alignParentRight = true;
            content.layoutParams.alignRules.below = 101; content.layoutParams.alignRules.above = 102;

            rel.addView(header); rel.addView(footer); rel.addView(sidebar); rel.addView(content);
            rel.measure(MeasureSpec.makeMeasureSpec(400, EXACTLY), MeasureSpec.makeMeasureSpec(600, EXACTLY));
            rel.layout(0, 0, 400, 600);

            const pass = (header.top === 5 && header.bottom === 55) &&
                         (footer.top === 535 && footer.bottom === 595) &&
                         (sidebar.top === 55 && sidebar.bottom === 535 && sidebar.left === 5 && sidebar.right === 85) &&
                         (content.left === 85 && content.right === 395 && content.top === 55 && content.bottom === 535);
            recordTest("RelativeLayout", "4-node spatial constraint graph (Header, Footer, Sidebar, Content)", pass,
                pass ? null : new Error(`Header: [${header.top}, ${header.bottom}], Footer: [${footer.top}, ${footer.bottom}], Sidebar: [${sidebar.left}, ${sidebar.top}, ${sidebar.right}, ${sidebar.bottom}], Content: [${content.left}, ${content.top}, ${content.right}, ${content.bottom}]`));
        } catch (e) { recordTest("RelativeLayout", "4-node spatial constraint graph", false, e); }

        try {
            const cyclicRel = new RelativeLayout();
            const cA = new View(new LayoutParams(100, 50)); cA.id = 201; cA.layoutParams.alignRules.below = 202;
            const cB = new View(new LayoutParams(100, 50)); cB.id = 202; cB.layoutParams.alignRules.below = 201;
            cyclicRel.addView(cA); cyclicRel.addView(cB);

            cyclicRel.measure(MeasureSpec.makeMeasureSpec(300, EXACTLY), MeasureSpec.makeMeasureSpec(300, EXACTLY));
            cyclicRel.layout(0, 0, 300, 300);
            recordTest("RelativeLayout", "Cyclic dependency termination without infinite loop", typeof cA.top === 'number' && typeof cB.top === 'number');
        } catch (e) { recordTest("RelativeLayout", "Cyclic dependency termination", false, e); }

        // Unknown sibling ID lookup
        try {
            const unknownRel = new RelativeLayout();
            const item = new View(new LayoutParams(100, 50));
            item.layoutParams.alignRules.below = 999999; // Non-existent ID
            unknownRel.addView(item);
            unknownRel.measure(MeasureSpec.makeMeasureSpec(300, EXACTLY), MeasureSpec.makeMeasureSpec(300, EXACTLY));
            unknownRel.layout(0, 0, 300, 300);
            recordTest("RelativeLayout", "Non-existent anchor ID lookup gracefully handles null target", typeof item.top === 'number');
        } catch (e) { recordTest("RelativeLayout", "Non-existent anchor ID lookup", false, e); }
    });

    // =========================================================================
    // Suite 6: FrameLayout Stacking & Multi-Gravity Alignment
    // =========================================================================
    await runSuite("6. FrameLayout Multi-Gravity & Overlapping Bounds", async () => {
        try {
            const frame = new FrameLayout();
            frame.setPadding(10, 10, 10, 10);

            const tl = new View(new LayoutParams(50, 50)); tl.layoutParams.gravity = 0x33;
            const tr = new View(new LayoutParams(50, 50)); tr.layoutParams.gravity = 0x35;
            const bl = new View(new LayoutParams(50, 50)); bl.layoutParams.gravity = 0x53;
            const br = new View(new LayoutParams(50, 50)); br.layoutParams.gravity = 0x55;
            const c = new View(new LayoutParams(60, 60)); c.layoutParams.gravity = 17;

            frame.addView(tl); frame.addView(tr); frame.addView(bl); frame.addView(br); frame.addView(c);
            frame.measure(MeasureSpec.makeMeasureSpec(300, EXACTLY), MeasureSpec.makeMeasureSpec(300, EXACTLY));
            frame.layout(0, 0, 300, 300);

            const pass = (tl.left === 10 && tl.top === 10 && tl.right === 60 && tl.bottom === 60) &&
                         (tr.left === 240 && tr.top === 10 && tr.right === 290 && tr.bottom === 60) &&
                         (bl.left === 10 && bl.top === 240 && bl.right === 60 && bl.bottom === 290) &&
                         (br.left === 240 && br.top === 240 && br.right === 290 && br.bottom === 290) &&
                         (c.left === 120 && c.top === 120 && c.right === 180 && c.bottom === 180);
            recordTest("FrameLayout", "5-point gravity positioning (TL, TR, BL, BR, Center)", pass,
                pass ? null : new Error(`TL: (${tl.left}, ${tl.top}), TR: (${tr.left}, ${tr.top}), BL: (${bl.left}, ${bl.top}), BR: (${br.left}, ${br.top}), Center: (${c.left}, ${c.top})`));
        } catch (e) { recordTest("FrameLayout", "5-point gravity positioning", false, e); }
    });

    // =========================================================================
    // Suite 7: ConstraintLayout Dual-Anchor Graph & Bias
    // =========================================================================
    await runSuite("7. ConstraintLayout Dual-Anchor Graph & Bias", async () => {
        try {
            const cl = new ConstraintLayout();
            const biased = new View(new LayoutParams(100, 100));
            biased.layoutParams.constraints = {
                layout_constraintStart_toStartOf: 'parent',
                layout_constraintEnd_toEndOf: 'parent',
                layout_constraintTop_toTopOf: 'parent',
                layout_constraintBottom_toBottomOf: 'parent',
                layout_constraintHorizontal_bias: 0.25,
                layout_constraintVertical_bias: 0.75
            };

            const match = new View(new LayoutParams(0, 0));
            match.layoutParams.setMargins(20, 30, 20, 30);
            match.layoutParams.constraints = {
                layout_constraintStart_toStartOf: 'parent',
                layout_constraintEnd_toEndOf: 'parent',
                layout_constraintTop_toTopOf: 'parent',
                layout_constraintBottom_toBottomOf: 'parent'
            };

            cl.addView(biased); cl.addView(match);
            cl.measure(MeasureSpec.makeMeasureSpec(500, EXACTLY), MeasureSpec.makeMeasureSpec(500, EXACTLY));
            cl.layout(0, 0, 500, 500);

            const passBias = (biased.left === 100 && biased.right === 200 && biased.top === 300 && biased.bottom === 400);
            const passMatch = (match.left === 20 && match.right === 480 && match.top === 30 && match.bottom === 470);
            recordTest("ConstraintLayout", "Biased positioning (hBias=0.25, vBias=0.75)", passBias,
                passBias ? null : new Error(`Biased bounds: left=${biased.left}, top=${biased.top}`));
            recordTest("ConstraintLayout", "MATCH_CONSTRAINT anchor filling with margins (20..480, 30..470)", passMatch,
                passMatch ? null : new Error(`Match bounds: left=${match.left}, right=${match.right}, top=${match.top}, bottom=${match.bottom}`));
        } catch (e) { recordTest("ConstraintLayout", "Dual-anchor and bias constraints", false, e); }
    });

    // =========================================================================
    // Suite 8: ScrollView Unbounded Measurement & Scroll Hit Testing
    // =========================================================================
    await runSuite("8. ScrollView Unbounded Measurement & Scroll Hit Testing", async () => {
        try {
            const scroll = new ScrollView(new LayoutParams(MATCH_PARENT, MATCH_PARENT));
            const content = new LinearLayout(VERTICAL, new LayoutParams(MATCH_PARENT, WRAP_CONTENT));

            const btns = [];
            for (let i = 0; i < 10; i++) {
                const btn = new Button(`Item ${i}`, new LayoutParams(MATCH_PARENT, 100));
                btn.id = 3000 + i;
                btn.wasClicked = false;
                btn.setOnClickListener(() => { btn.wasClicked = true; });
                content.addView(btn);
                btns.push(btn);
            }
            scroll.addView(content);

            scroll.measure(MeasureSpec.makeMeasureSpec(360, EXACTLY), MeasureSpec.makeMeasureSpec(400, EXACTLY));
            scroll.layout(0, 0, 360, 400);

            const passMeasure = (scroll.measuredHeight === 400 && content.measuredHeight === 1000 && scroll.maxScrollY === 600);
            recordTest("ScrollView", "Unbounded child measurement (1000px in 400px viewport)", passMeasure);

            scroll.scrollTo(0, 9999);
            const passClamp = (scroll.scrollY === 600);
            recordTest("ScrollView", "Max scroll clamp (scrollY=600)", passClamp);

            scroll.scrollTo(0, 500);
            btns[5].wasClicked = false;
            scroll.dispatchTouchEvent({ action: 0, x: 100, y: 50 });
            scroll.dispatchTouchEvent({ action: 1, x: 100, y: 50 });
            const passScrollHit = (btns[5].wasClicked === true);
            recordTest("ScrollView", "Scrolled coordinate touch dispatch (scrollY=500 -> Item 5 clicked)", passScrollHit,
                passScrollHit ? null : new Error(`Item 5 wasClicked is false`));
        } catch (e) { recordTest("ScrollView", "ScrollView tests", false, e); }
    });

    // =========================================================================
    // Suite 9: RecyclerView Dynamic Adapter Binding
    // =========================================================================
    await runSuite("9. RecyclerView Dynamic Adapter Binding", async () => {
        try {
            const recycler = new RecyclerView();
            let clickedRow = -1;
            const adapter = {
                getItemCount() { return 20; },
                onCreateViewHolder() {
                    const tv = new TextView('', new LayoutParams(MATCH_PARENT, 40));
                    tv.isClickable = true;
                    return { itemView: tv };
                },
                onBindViewHolder(holder, position) {
                    holder.itemView.setText(`Row ${position}`);
                    holder.itemView.setOnClickListener(() => { clickedRow = position; });
                }
            };
            recycler.setAdapter(adapter);

            recycler.measure(MeasureSpec.makeMeasureSpec(360, EXACTLY), MeasureSpec.makeMeasureSpec(600, EXACTLY));
            recycler.layout(0, 0, 360, 600);

            const passCount = (recycler.getChildCount() === 20);
            recordTest("RecyclerView", "Adapter item instantiation (20 items)", passCount);

            recycler.dispatchTouchEvent({ action: 0, x: 100, y: 220 });
            recycler.dispatchTouchEvent({ action: 1, x: 100, y: 220 });
            const passHit = (clickedRow === 5);
            recordTest("RecyclerView", "Touch routing to item index 5 (y: 200..240)", passHit,
                passHit ? null : new Error(`clickedRow was ${clickedRow}, expected 5`));
        } catch (e) { recordTest("RecyclerView", "RecyclerView tests", false, e); }
    });

    // =========================================================================
    // Suite 10: Reverse-Z Touch Event Routing & Bubbling
    // =========================================================================
    await runSuite("10. Reverse-Z Touch Routing & Bubbling", async () => {
        try {
            const frame = new FrameLayout(new LayoutParams(300, 300));
            let b0 = false, b2 = false;

            const l0 = new Button("L0", new LayoutParams(200, 200)); l0.setOnClickListener(() => { b0 = true; });
            const l1 = new View(new LayoutParams(200, 200)); l1.isClickable = false;
            const l2 = new Button("L2", new LayoutParams(100, 200)); l2.layoutParams.setMargins(100, 0, 0, 0); l2.setOnClickListener(() => { b2 = true; });

            frame.addView(l0); frame.addView(l1); frame.addView(l2);
            frame.measure(MeasureSpec.makeMeasureSpec(300, EXACTLY), MeasureSpec.makeMeasureSpec(300, EXACTLY));
            frame.layout(0, 0, 300, 300);

            b0 = false; b2 = false;
            frame.dispatchTouchEvent({ action: 0, x: 150, y: 100 });
            frame.dispatchTouchEvent({ action: 1, x: 150, y: 100 });
            const passTop = (b2 === true && b0 === false);
            recordTest("Reverse-Z Touch", "Topmost layer 2 captures touch on overlap (150, 100)", passTop);

            b0 = false; b2 = false;
            frame.dispatchTouchEvent({ action: 0, x: 50, y: 100 });
            frame.dispatchTouchEvent({ action: 1, x: 50, y: 100 });
            const passBubble = (b0 === true && b2 === false);
            recordTest("Reverse-Z Touch", "Touch bubbles through non-clickable layer 1 to layer 0 (50, 100)", passBubble);

            l2.visibility = INVISIBLE;
            b0 = false; b2 = false;
            frame.dispatchTouchEvent({ action: 0, x: 150, y: 100 });
            frame.dispatchTouchEvent({ action: 1, x: 150, y: 100 });
            const passInvis = (b0 === true && b2 === false);
            recordTest("Reverse-Z Touch", "INVISIBLE layer 2 passes touch through to layer 0", passInvis);
        } catch (e) { recordTest("Reverse-Z Touch", "Reverse-Z touch tests", false, e); }
    });

    // =========================================================================
    // Suite 11: LayoutInflater F-Droid.apk Inflation & Malformed XML Fuzzing
    // =========================================================================
    await runSuite("11. LayoutInflater Real APK Layouts & Fuzzing", async () => {
        try {
            const apkBytes = fs.readFileSync(path.join(ROOT_DIR, 'F-Droid.apk'));
            const apkZip = new ApkZipReader(apkBytes);
            const arscBytes = apkZip.readFile('resources.arsc');
            const resTable = ArscDecoder.decode(arscBytes);

            const testLayouts = [
                'res/v9.xml', 'res/Kt.xml', 'res/mQ.xml', 'res/u8.xml', 'res/FB.xml',
                'res/2h.xml', 'res/Md.xml', 'res/6V.xml', 'res/B_.xml', 'res/aR.xml'
            ];

            let allInflated = true;
            for (const layoutPath of testLayouts) {
                const buf = apkZip.readFile(layoutPath);
                const view = LayoutInflater.inflate(buf, resTable);
                if (!(view instanceof View)) {
                    allInflated = false;
                    break;
                }
                view.measure(MeasureSpec.makeMeasureSpec(360, EXACTLY), MeasureSpec.makeMeasureSpec(640, EXACTLY));
                view.layout(0, 0, 360, 640);
            }
            recordTest("LayoutInflater", "Inflate, measure, layout 10 distinct F-Droid layout binaries", allInflated);
        } catch (e) { recordTest("LayoutInflater", "Inflate 10 F-Droid layouts", false, e); }

        try {
            const corruptInputs = [
                new Uint8Array(0),
                new Uint8Array([0x01, 0x02]),
                new Uint8Array([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
                null, undefined, "invalid string"
            ];
            let allSafe = true;
            for (const c of corruptInputs) {
                const v = LayoutInflater.inflate(c, null);
                if (!(v instanceof View)) allSafe = false;
            }
            recordTest("LayoutInflater", "Safe fallback to View on malformed buffer inputs", allSafe);

            const emptyObj = {};
            const objResult = LayoutInflater.inflate(emptyObj, null);
            const passObj = (objResult instanceof View);
            recordTest("LayoutInflater", "Safe fallback to View on empty object AST input ({})", passObj,
                passObj ? null : new Error(`LayoutInflater.inflate({}) returned ${objResult} instead of View instance`));
        } catch (e) { recordTest("LayoutInflater", "Malformed buffer fuzzing", false, e); }
    });

    // =========================================================================
    // Suite 12: TypedValue Complex Dimension Units & Color Boundary Fuzzing
    // =========================================================================
    await runSuite("12. TypedValue Complex Dimension Units & Color Boundaries", async () => {
        try {
            // Negative dimension mantissa
            // 24-bit signed negative mantissa: -10dp
            const negMantissa = (-10 & 0x00FFFFFF) << 8;
            const negDimWord = negMantissa | COMPLEX_UNIT_DIP;
            const dimResult = TypedValue.complexToDimension(negDimWord, 1.0);
            recordTest("TypedValue", "Negative dimension mantissa decoding (-10dp)", dimResult === -10,
                dimResult === -10 ? null : new Error(`Got ${dimResult}, expected -10`));

            // Float decoding
            const floatBytes = new Uint8Array([0x00, 0x00, 0x80, 0x3F]); // 1.0f in IEEE 754
            const floatData = new DataView(floatBytes.buffer).getUint32(0, true);
            const floatVal = TypedValue.decodeValue(TypedValue.TYPE_FLOAT, floatData);
            recordTest("TypedValue", "IEEE 754 32-bit Float decoding (1.0f)", floatVal === 1.0,
                floatVal === 1.0 ? null : new Error(`Got ${floatVal}, expected 1.0`));

            // Null and empty strings
            const nullVal = TypedValue.decodeValue(TypedValue.TYPE_NULL, 0);
            recordTest("TypedValue", "TYPE_NULL decodes to null", nullVal === null);
        } catch (e) { recordTest("TypedValue", "TypedValue tests", false, e); }
    });

    // =========================================================================
    // Summary
    // =========================================================================
    console.log("\n================================================================================");
    const passedCount = results.filter(r => r.passed).length;
    const failedCount = results.filter(r => !r.passed).length;
    console.log(`📊 CHALLENGER 2 SUMMARY: ${passedCount}/${results.length} Tests Passed (${failedCount} Failed)`);
    console.log("================================================================================\n");

    const failedItems = results.filter(r => !r.passed);
    if (failedItems.length > 0) {
        console.log("FAILED TESTS SUMMARY:");
        for (const item of failedItems) {
            console.log(`  - [${item.suiteName}] ${item.testName}: ${item.error}`);
        }
    }
}

main().catch(err => {
    console.error("FATAL ERROR IN HARNESS:", err);
    process.exit(1);
});
