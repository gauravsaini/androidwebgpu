/**
 * Milestone M1 Comprehensive Verification Test Suite:
 * Authentic APK Resource Resolution & Binary XML Layout Inflation
 * 
 * Tests:
 * 1. ArscDecoder & ArscResourceTable binary parsing and config matching.
 * 2. TypedValue complex unit decoding (dp, sp, px, pt, in, mm) and hex color decoding.
 * 3. MeasureSpec bitmask packing, mode extraction, and size calculations.
 * 4. LayoutParams properties (MATCH_PARENT, WRAP_CONTENT, margins, weight, rules).
 * 5. View base model (measure, layout, draw, reverse-Z touch dispatch, click listener, findViewById).
 * 6. ViewGroup & Container layouts (FrameLayout, LinearLayout with weights, RelativeLayout, ConstraintLayout, ScrollView, RecyclerView).
 * 7. Core widgets (TextView with font metrics/ellipsize, ImageView with scaleType, Button with MD3 styling).
 * 8. LayoutInflater binary XML inflation against real F-Droid.apk layouts.
 * 9. Adversarial edge cases (malformed buffers, unknown IDs, circular dependencies).
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
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
    TYPE_NULL,
    TYPE_REFERENCE,
    TYPE_STRING,
    TYPE_FLOAT,
    TYPE_DIMENSION,
    TYPE_INT_DEC,
    TYPE_INT_HEX,
    TYPE_INT_BOOLEAN,
    TYPE_INT_COLOR_ARGB8,
    TYPE_INT_COLOR_RGB8,
    TYPE_INT_COLOR_ARGB4,
    TYPE_INT_COLOR_RGB4,
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

let totalTests = 0;
let passedTests = 0;

function check(description, condition) {
    totalTests++;
    if (!condition) {
        console.error(`  ✖ [FAIL] ${description}`);
        throw new Error(`Assertion failed: ${description}`);
    }
    passedTests++;
    console.log(`  ✔ [PASS] ${description}`);
}

async function main() {
    console.log("================================================================================");
    console.log("⚡ STARTING MILESTONE M1 VIEW SYSTEM & ARSC RESOLVER VERIFICATION");
    console.log("================================================================================\n");

    const apkPath = path.join(ROOT_DIR, 'F-Droid.apk');
    const apkBytes = fs.readFileSync(apkPath);
    const apkZip = new ApkZipReader(apkBytes);

    // =========================================================================
    // Section 1: TypedValue Complex Dimension & Color Unit Decoding
    // =========================================================================
    console.log("▶ Section 1: TypedValue Complex Dimension & Color Unit Decoding");

    // 1.1 Dimension Unit Decoding (48dp, 16dp, 8dp, 18sp)
    const dim48dp = 0x3001; // 48dp
    check("1.1: 48dp at 1.0x density equals 48px", TypedValue.complexToDimension(dim48dp, 1.0) === 48);
    check("1.2: 48dp at 2.0x density equals 96px", TypedValue.complexToDimension(dim48dp, 2.0) === 96);
    check("1.3: 48dp at 1.5x density equals 72px", TypedValue.complexToDimension(dim48dp, 1.5) === 72);

    const dim18sp = 0x1202; // 18sp
    check("1.4: 18sp at 1.0x density equals 18px", TypedValue.complexToDimension(dim18sp, 1.0) === 18);
    check("1.5: 18sp at 2.0x density equals 36px", TypedValue.complexToDimension(dim18sp, 2.0) === 36);

    const dimPx = 0x1000; // 16px (unit = 0)
    check("1.6: 16px unit conversion preserves exact pixel size", TypedValue.complexToDimension(dimPx, 2.0) === 16);

    const dimPt = 0x4803; // 72pt (unit = 3)
    check("1.7: 72pt conversion scales correctly (160px at 1.0 density)", Math.round(TypedValue.complexToDimension(dimPt, 1.0)) === 160);

    const dimIn = 0x0104; // 1in (unit = 4)
    check("1.8: 1in conversion equals 160px at 1.0 density", TypedValue.complexToDimension(dimIn, 1.0) === 160);

    // 1.9 Dimension Pixel Size & Offset
    check("1.9: complexToDimensionPixelSize rounds properly", TypedValue.complexToDimensionPixelSize(0x0801, 1.5) === 12);
    check("1.10: complexToDimensionPixelOffset truncates properly", TypedValue.complexToDimensionPixelOffset(0x0701, 1.5) === 10);
    check("1.11: formatDimension outputs unit string", TypedValue.formatDimension(dim48dp) === '48dp');

    // 1.12 Color Decoding (ARGB8, RGB8, ARGB4, RGB4)
    check("1.12: ARGB8 opaque color decodes to #RRGGBB", TypedValue.decodeColor(0xFF123456, TYPE_INT_COLOR_ARGB8) === '#123456');
    check("1.13: ARGB8 semi-transparent color decodes to #AARRGGBB", TypedValue.decodeColor(0x80123456, TYPE_INT_COLOR_ARGB8) === '#80123456');
    check("1.14: RGB8 color decodes to #RRGGBB", TypedValue.decodeColor(0x123456, TYPE_INT_COLOR_RGB8) === '#123456');
    check("1.15: ARGB4 color expands nibbles", TypedValue.decodeColor(0xF123, TYPE_INT_COLOR_ARGB4) === '#112233');

    // 1.16 Primitive Value Decoding
    check("1.16: Boolean value decoding", TypedValue.decodeValue(TYPE_INT_BOOLEAN, 1) === true && TypedValue.decodeValue(TYPE_INT_BOOLEAN, 0) === false);
    check("1.17: Signed integer decoding for MATCH_PARENT (-1)", TypedValue.decodeValue(TYPE_INT_DEC, 0xFFFFFFFF) === -1);
    check("1.18: Signed integer decoding for WRAP_CONTENT (-2)", TypedValue.decodeValue(TYPE_INT_DEC, 0xFFFFFFFE) === -2);

    // =========================================================================
    // Section 2: ArscDecoder & ArscResourceTable Resolution from F-Droid.apk
    // =========================================================================
    console.log("\n▶ Section 2: ArscDecoder & ArscResourceTable Resolution from F-Droid.apk");

    const arscBytes = apkZip.readFile('resources.arsc');
    check("2.1: resources.arsc extracted from APK", arscBytes && arscBytes.length > 1000000);

    const resTable = ArscDecoder.decode(arscBytes);
    check("2.2: ArscDecoder parsed resources.arsc without error", resTable instanceof ArscResourceTable);
    check("2.3: Package org.fdroid.fdroid registered", resTable.packages.has(0x7f));

    // 2.4 Layout Path Resolution
    check("2.4: resolveLayoutPath(0x7f0c0020) resolves to res/v9.xml", resTable.resolveLayoutPath(0x7f0c0020) === 'res/v9.xml');
    check("2.5: resolveLayoutPath('activity_main') resolves to res/v9.xml", resTable.resolveLayoutPath('activity_main') === 'res/v9.xml');
    check("2.6: resolveLayoutPath('app_list_item') resolves to res/Kt.xml", resTable.resolveLayoutPath('app_list_item') === 'res/Kt.xml');

    // 2.7 Identifier Resolution
    const actMainId = resTable.resolveIdentifier('activity_main', 'layout');
    check("2.7: resolveIdentifier('activity_main', 'layout') returns 0x7f0c0020", actMainId === 0x7f0c0020);

    const iconId = resTable.resolveIdentifier('icon', 'id');
    check("2.8: resolveIdentifier('icon', 'id') returns 0x7f09013e", iconId === 0x7f09013e);

    const appNameId = resTable.resolveIdentifier('app_name', 'id');
    check("2.9: resolveIdentifier('app_name', 'id') returns 0x7f09006d", appNameId === 0x7f09006d);

    // 2.10 String Resolution with Config Matching & Fallback
    const appNameStr = resTable.resolveString(0x7f120075);
    check("2.10: Default string resolveString(0x7f120075) returns 'F-Droid'", appNameStr === 'F-Droid');

    const deTable = ArscDecoder.decode(arscBytes, 'de');
    const deStr = deTable.resolveString(0x7f120075);
    check("2.11: German locale table fallback resolves string correctly", typeof deStr === 'string' && deStr.length > 0);

    // 2.12 Style / Bag Resolution
    const detailsButtonStyleId = resTable.resolveIdentifier('DetailsPrimaryButtonStyle', 'style');
    if (detailsButtonStyleId) {
        const styleBag = resTable.resolveStyle(detailsButtonStyleId);
        check("2.12: resolveStyle resolves complex style bag", styleBag && styleBag.key === 'DetailsPrimaryButtonStyle');
    } else {
        check("2.12: resolveStyle handles non-existent style safely", resTable.resolveStyle(0x7f139999) === null);
    }

    // 2.13 All Layout Entries Count
    const allLayouts = resTable.getAllEntries('layout');
    check("2.13: getAllEntries('layout') returns all 197 layouts", allLayouts.length === 197);

    // =========================================================================
    // Section 3: MeasureSpec & LayoutParams Invariants
    // =========================================================================
    console.log("\n▶ Section 3: MeasureSpec & LayoutParams Invariants");

    const specExact = MeasureSpec.makeMeasureSpec(360, EXACTLY);
    check("3.1: MeasureSpec.getMode for EXACTLY", MeasureSpec.getMode(specExact) === EXACTLY);
    check("3.2: MeasureSpec.getSize for EXACTLY", MeasureSpec.getSize(specExact) === 360);

    const specAtMost = MeasureSpec.makeMeasureSpec(640, AT_MOST);
    check("3.3: MeasureSpec.getMode for AT_MOST", MeasureSpec.getMode(specAtMost) === AT_MOST);
    check("3.4: MeasureSpec.getSize for AT_MOST", MeasureSpec.getSize(specAtMost) === 640);

    const specUnspecified = MeasureSpec.makeMeasureSpec(0, UNSPECIFIED);
    check("3.5: MeasureSpec.getMode for UNSPECIFIED", MeasureSpec.getMode(specUnspecified) === UNSPECIFIED);
    check("3.6: MeasureSpec.getSize for UNSPECIFIED", MeasureSpec.getSize(specUnspecified) === 0);

    const lp = new LayoutParams(MATCH_PARENT, WRAP_CONTENT);
    lp.setMargins(10, 20, 30, 40);
    check("3.7: LayoutParams width and height", lp.width === MATCH_PARENT && lp.height === WRAP_CONTENT);
    check("3.8: LayoutParams margins array", lp.marginLeft === 10 && lp.marginTop === 20 && lp.marginRight === 30 && lp.marginBottom === 40);

    // =========================================================================
    // Section 4: In-Memory View Hierarchy & Event Handling
    // =========================================================================
    console.log("\n▶ Section 4: In-Memory View Hierarchy & Event Handling");

    const parentView = new ViewGroup(new LayoutParams(MATCH_PARENT, MATCH_PARENT));
    const child1 = new View(new LayoutParams(100, 50));
    child1.id = 101;
    let child1Clicked = false;
    child1.setOnClickListener(() => { child1Clicked = true; });

    const child2 = new View(new LayoutParams(100, 50));
    child2.id = 102;

    parentView.addView(child1);
    parentView.addView(child2);

    check("4.1: ViewGroup child count is 2", parentView.getChildCount() === 2);
    check("4.2: findViewById finds child by ID", parentView.findViewById(101) === child1 && parentView.findViewById(102) === child2);
    check("4.3: getRootView resolves to parentView", child1.getRootView() === parentView);

    // Measure & Layout
    parentView.measure(MeasureSpec.makeMeasureSpec(400, EXACTLY), MeasureSpec.makeMeasureSpec(800, EXACTLY));
    parentView.layout(0, 0, 400, 800);
    child1.layout(10, 10, 110, 60);
    child2.layout(10, 70, 110, 120);

    // Hit Testing & Touch Dispatch
    // Down event on child1 (x=50, y=30)
    const downEvent = { action: 0 /* ACTION_DOWN */, x: 50, y: 30 };
    const handledDown = parentView.dispatchTouchEvent(downEvent);
    check("4.4: dispatchTouchEvent routes ACTION_DOWN to child1", handledDown === true && child1.isPressed === true);

    // Up event on child1 (x=50, y=30)
    const upEvent = { action: 1 /* ACTION_UP */, x: 50, y: 30 };
    const handledUp = parentView.dispatchTouchEvent(upEvent);
    check("4.5: dispatchTouchEvent routes ACTION_UP and fires click listener", handledUp === true && child1Clicked === true);

    // Visibility GONE behavior
    child2.visibility = GONE;
    child2.measure(MeasureSpec.makeMeasureSpec(100, EXACTLY), MeasureSpec.makeMeasureSpec(50, EXACTLY));
    check("4.6: Visibility GONE view measures to 0x0", child2.measuredWidth === 0 && child2.measuredHeight === 0);

    // =========================================================================
    // Section 5: Layout Containers (FrameLayout, LinearLayout, Relative, Constraint)
    // =========================================================================
    console.log("\n▶ Section 5: Layout Containers");

    // 5.1 LinearLayout Vertical with Weight Distribution
    const vLinear = new LinearLayout(VERTICAL, new LayoutParams(MATCH_PARENT, 300));
    const topItem = new View(new LayoutParams(MATCH_PARENT, 100));
    const weightedItem1 = new View(new LayoutParams(MATCH_PARENT, 0));
    weightedItem1.layoutParams.weight = 1;
    const weightedItem2 = new View(new LayoutParams(MATCH_PARENT, 0));
    weightedItem2.layoutParams.weight = 1;

    vLinear.addView(topItem);
    vLinear.addView(weightedItem1);
    vLinear.addView(weightedItem2);

    vLinear.measure(MeasureSpec.makeMeasureSpec(300, EXACTLY), MeasureSpec.makeMeasureSpec(300, EXACTLY));
    vLinear.layout(0, 0, 300, 300);

    check("5.1: LinearLayout measures top fixed item to 100px", topItem.measuredHeight === 100);
    check("5.2: LinearLayout distributes remaining 200px equally (100px each)", weightedItem1.measuredHeight === 100 && weightedItem2.measuredHeight === 100);
    check("5.3: LinearLayout positions children in vertical sequence", topItem.top === 0 && weightedItem1.top === 100 && weightedItem2.top === 200);

    // 5.4 FrameLayout Child Gravity
    const frame = new FrameLayout(new LayoutParams(200, 200));
    const centeredChild = new View(new LayoutParams(60, 60));
    centeredChild.layoutParams.gravity = 17; // CENTER
    frame.addView(centeredChild);
    frame.measure(MeasureSpec.makeMeasureSpec(200, EXACTLY), MeasureSpec.makeMeasureSpec(200, EXACTLY));
    frame.layout(0, 0, 200, 200);

    check("5.4: FrameLayout centers child with gravity=CENTER", centeredChild.left === 70 && centeredChild.top === 70);

    // 5.5 RelativeLayout Anchor Dependencies
    const relLayout = new RelativeLayout(new LayoutParams(300, 300));
    const header = new View(new LayoutParams(MATCH_PARENT, 50));
    header.id = 1;
    header.layoutParams.alignRules.alignParentTop = true;

    const footer = new View(new LayoutParams(MATCH_PARENT, 50));
    footer.id = 2;
    footer.layoutParams.alignRules.alignParentBottom = true;

    const body = new View(new LayoutParams(MATCH_PARENT, MATCH_PARENT));
    body.id = 3;
    body.layoutParams.alignRules.below = 1;
    body.layoutParams.alignRules.above = 2;

    relLayout.addView(header);
    relLayout.addView(footer);
    relLayout.addView(body);

    relLayout.measure(MeasureSpec.makeMeasureSpec(300, EXACTLY), MeasureSpec.makeMeasureSpec(300, EXACTLY));
    relLayout.layout(0, 0, 300, 300);

    check("5.5: RelativeLayout positions header at top (y=0)", header.top === 0);
    check("5.6: RelativeLayout positions footer at bottom (y=250)", footer.top === 250);
    check("5.7: RelativeLayout positions body between header and footer (top=50, bottom=250)", body.top === 50 && body.bottom === 250);

    // 5.8 ConstraintLayout Anchor Chaining
    const cLayout = new ConstraintLayout(new LayoutParams(400, 400));
    const cChild = new View(new LayoutParams(100, 50));
    cChild.layoutParams.constraints.layout_constraintStart_toStartOf = 'parent';
    cChild.layoutParams.constraints.layout_constraintEnd_toEndOf = 'parent';
    cChild.layoutParams.constraints.layout_constraintTop_toTopOf = 'parent';
    cChild.layoutParams.constraints.layout_constraintBottom_toBottomOf = 'parent';
    cLayout.addView(cChild);

    cLayout.measure(MeasureSpec.makeMeasureSpec(400, EXACTLY), MeasureSpec.makeMeasureSpec(400, EXACTLY));
    cLayout.layout(0, 0, 400, 400);

    check("5.8: ConstraintLayout centers view horizontally (left=150)", cChild.left === 150);
    check("5.9: ConstraintLayout centers view vertically (top=175)", cChild.top === 175);

    // 5.10 ScrollView Measurement
    const scrollView = new ScrollView(new LayoutParams(300, 400));
    const longContent = new View(new LayoutParams(300, 1200));
    scrollView.addView(longContent);
    scrollView.measure(MeasureSpec.makeMeasureSpec(300, EXACTLY), MeasureSpec.makeMeasureSpec(400, EXACTLY));
    scrollView.layout(0, 0, 300, 400);

    check("5.10: ScrollView measures long child without height clipping (1200px)", longContent.measuredHeight === 1200);
    check("5.11: ScrollView viewport measures to parent spec height (400px)", scrollView.measuredHeight === 400);

    // =========================================================================
    // Section 6: Core Widgets (TextView, ImageView, Button)
    // =========================================================================
    console.log("\n▶ Section 6: Core Widgets");

    // 6.1 TextView text measurement
    const tv = new TextView("Hello Android WebGPU");
    tv.textSize = 16;
    tv.measure(MeasureSpec.makeMeasureSpec(300, AT_MOST), MeasureSpec.makeMeasureSpec(100, AT_MOST));
    check("6.1: TextView measures non-zero dimensions for text", tv.measuredWidth > 50 && tv.measuredHeight > 10);

    // 6.2 ImageView scaleType
    const iv = new ImageView();
    iv.scaleType = ImageView.CENTER_CROP;
    iv.measure(MeasureSpec.makeMeasureSpec(48, EXACTLY), MeasureSpec.makeMeasureSpec(48, EXACTLY));
    check("6.2: ImageView measures to specified 48x48 bounds", iv.measuredWidth === 48 && iv.measuredHeight === 48);

    // 6.3 Button MD3 Properties
    const btn = new Button("Install");
    check("6.3: Button is clickable by default", btn.isClickable === true);
    check("6.4: Button text is 'Install'", btn.getText() === 'Install');

    // =========================================================================
    // Section 7: Authentic Binary XML Layout Inflation (F-Droid.apk)
    // =========================================================================
    console.log("\n▶ Section 7: Authentic Binary XML Layout Inflation (F-Droid.apk)");

    // 7.1 activity_main inflation (res/v9.xml)
    const v9Bytes = apkZip.readFile('res/v9.xml');
    const rootMain = LayoutInflater.inflate(v9Bytes, resTable);

    check("7.1: activity_main inflates to RelativeLayout root", rootMain instanceof RelativeLayout);
    check("7.2: activity_main contains 2 children", rootMain.getChildCount() === 2);

    const bottomNav = rootMain.getChildAt(0);
    check("7.3: BottomNavigationView id is 0x7f090088", bottomNav.id === 0x7f090088);

    const mainViewPager = rootMain.getChildAt(1);
    check("7.4: main_view_pager is RecyclerView id 0x7f09016d", mainViewPager instanceof RecyclerView && mainViewPager.id === 0x7f09016d);

    rootMain.measure(MeasureSpec.makeMeasureSpec(360, EXACTLY), MeasureSpec.makeMeasureSpec(640, EXACTLY));
    rootMain.layout(0, 0, 360, 640);
    check("7.5: activity_main measures to 360x640", rootMain.measuredWidth === 360 && rootMain.measuredHeight === 640);

    // 7.6 app_list_item inflation (res/Kt.xml)
    const ktBytes = apkZip.readFile('res/Kt.xml');
    const rootListItem = LayoutInflater.inflate(ktBytes, resTable);

    check("7.6: app_list_item inflates to ConstraintLayout root", rootListItem instanceof ConstraintLayout);
    check("7.7: app_list_item contains 4 child views", rootListItem.getChildCount() === 4);

    const itemIcon = rootListItem.findViewById(0x7f09013e);
    check("7.8: app_list_item finds icon ImageView (0x7f09013e)", itemIcon instanceof ImageView);

    const itemName = rootListItem.findViewById(0x7f09006d);
    check("7.9: app_list_item finds app_name TextView (0x7f09006d)", itemName instanceof TextView);

    // =========================================================================
    // Section 8: Adversarial & Edge Case Invariants
    // =========================================================================
    console.log("\n▶ Section 8: Adversarial & Edge Case Invariants");

    // 8.1 Malformed / Undersized ARSC Buffer
    const badArsc = ArscDecoder.decode(new Uint8Array([0x00, 0x00, 0x04]));
    check("8.1: Malformed ARSC returns empty table without throwing", badArsc instanceof ArscResourceTable);
    check("8.2: Resolving string on empty table returns null", badArsc.resolveString(0x7f120001) === null);

    // 8.3 Non-existent Layout and String
    check("8.3: resolveLayoutPath for non-existent ID returns null", resTable.resolveLayoutPath(0x7f0c9999) === null);
    check("8.4: resolveString for non-existent ID returns null", resTable.resolveString(0x7f129999) === null);

    // 8.5 Null buffer to LayoutInflater returns empty View
    const nullInflated = LayoutInflater.inflate(null);
    check("8.5: LayoutInflater handles null input gracefully", nullInflated instanceof View);

    // 8.6 Circular dependency in RelativeLayout
    const circLayout = new RelativeLayout(new LayoutParams(300, 300));
    const circA = new View(new LayoutParams(50, 50));
    circA.id = 1;
    circA.layoutParams.alignRules.below = 2;
    const circB = new View(new LayoutParams(50, 50));
    circB.id = 2;
    circB.layoutParams.alignRules.below = 1;

    circLayout.addView(circA);
    circLayout.addView(circB);
    circLayout.measure(MeasureSpec.makeMeasureSpec(300, EXACTLY), MeasureSpec.makeMeasureSpec(300, EXACTLY));
    circLayout.layout(0, 0, 300, 300);
    check("8.6: Circular RelativeLayout rules execute without infinite loop", circLayout.measuredWidth === 300);

    console.log("\n================================================================================");
    console.log(`📊 EXECUTION SUMMARY: ${passedTests}/${totalTests} Tests Passed (100% Target Met)`);
    console.log("================================================================================");

    if (passedTests === totalTests) {
        console.log("✔ Milestone M1 View System Verification PASSED cleanly with zero failures!\n");
        process.exit(0);
    } else {
        console.error(`✖ ${totalTests - passedTests} tests FAILED!`);
        process.exit(1);
    }
}

main().catch(err => {
    console.error("Fatal test error:", err);
    process.exit(1);
});
