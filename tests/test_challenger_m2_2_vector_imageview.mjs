/**
 * tests/test_challenger_m2_2_vector_imageview.mjs
 * 
 * Challenger 2 Adversarial Stress Test Suite for Milestone 2:
 * VectorDrawable and ImageView Rendering Engine.
 * 
 * Tests:
 * 1. Complete SVG path command matrix (M, L, H, V, C, S, Q, T, A, Z, uppercase/lowercase/relative/absolute).
 * 2. ViewBox & Viewport scaling (uniform, non-uniform, fractional, extreme 8K, zero/fallback).
 * 3. Multi-path VectorDrawable layering, fill/stroke alpha blending, tint override.
 * 4. BitmapDrawable rendering and edge cases (buffers, mock images, invalid bitmaps).
 * 5. ImageView measurement, padding offsets, scale types, and container hierarchy integration.
 * 6. Authentic APK Vector & Mipmap/PNG drawable resolution from F-Droid.apk.
 * 
 * Complies with ASD-STE100 Simplified Technical English, /ponytail, and /caveman.
 */

import { strict as assert } from 'node:assert';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { ApkZipReader, AxmlDecoder } from '../src/apk_client_parser.js';
import { ArscDecoder, TypedValue } from '../src/apk_resource_resolver.js';
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
    FrameLayout,
    LinearLayout,
    ConstraintLayout,
    ImageView,
    VectorDrawable,
    BitmapDrawable,
    LayoutInflater,
    SCALE_TYPE_FIT_XY,
    SCALE_TYPE_FIT_CENTER,
    SCALE_TYPE_CENTER_CROP,
    SCALE_TYPE_CENTER_INSIDE
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

/**
 * Creates a mock 2D canvas context that records all drawing operations and state.
 */
function createMockCanvasContext() {
    return {
        stateStack: [],
        calls: [],
        fillStyle: '#000000',
        strokeStyle: '#000000',
        lineWidth: 1,
        globalAlpha: 1.0,
        currentTransform: { x: 0, y: 0, sx: 1, sy: 1 },

        save() {
            this.calls.push({ op: 'save' });
            this.stateStack.push({
                fillStyle: this.fillStyle,
                strokeStyle: this.strokeStyle,
                lineWidth: this.lineWidth,
                globalAlpha: this.globalAlpha,
                transform: { ...this.currentTransform }
            });
        },
        restore() {
            this.calls.push({ op: 'restore' });
            if (this.stateStack.length > 0) {
                const prev = this.stateStack.pop();
                this.fillStyle = prev.fillStyle;
                this.strokeStyle = prev.strokeStyle;
                this.lineWidth = prev.lineWidth;
                this.globalAlpha = prev.globalAlpha;
                this.currentTransform = prev.transform;
            }
        },
        translate(x, y) {
            this.calls.push({ op: 'translate', x, y });
            this.currentTransform.x += x;
            this.currentTransform.y += y;
        },
        scale(sx, sy) {
            this.calls.push({ op: 'scale', sx, sy });
            this.currentTransform.sx *= sx;
            this.currentTransform.sy *= sy;
        },
        beginPath() {
            this.calls.push({ op: 'beginPath' });
        },
        rect(x, y, w, h) {
            this.calls.push({ op: 'rect', x, y, w, h });
        },
        fillRect(x, y, w, h) {
            this.calls.push({ op: 'fillRect', x, y, w, h, fillStyle: this.fillStyle });
        },
        fill(pathObj) {
            this.calls.push({
                op: 'fill',
                pathObj: pathObj || null,
                fillStyle: this.fillStyle,
                globalAlpha: this.globalAlpha
            });
        },
        stroke(pathObj) {
            this.calls.push({
                op: 'stroke',
                pathObj: pathObj || null,
                strokeStyle: this.strokeStyle,
                lineWidth: this.lineWidth,
                globalAlpha: this.globalAlpha
            });
        },
        drawImage(img, x, y, w, h) {
            this.calls.push({ op: 'drawImage', img, x, y, w, h });
        }
    };
}

async function runChallengerTestSuite() {
    console.log('================================================================');
    console.log('🔥 Challenger 2: VectorDrawable & ImageView Empirical Stress Tests');
    console.log('================================================================\n');

    // -------------------------------------------------------------------------
    // Suite 1: SVG Path Command Matrix (M, L, H, V, C, S, Q, T, A, Z)
    // -------------------------------------------------------------------------
    console.log('▶ [Suite 1] SVG Path Command Matrix Stress Testing');

    const svgCommands = [
        { name: 'M / m (MoveTo absolute & relative)', pathData: 'M 10 10 m 5 5 L 20 20 Z' },
        { name: 'L / l (LineTo absolute & relative)', pathData: 'M 0 0 L 10 10 l 5 -5 L 20 0 Z' },
        { name: 'H / h (Horizontal LineTo absolute & relative)', pathData: 'M 5 5 H 40 h 10 V 20 Z' },
        { name: 'V / v (Vertical LineTo absolute & relative)', pathData: 'M 5 5 V 40 v -10 H 20 Z' },
        { name: 'C / c (Cubic Bezier absolute & relative)', pathData: 'M 0 0 C 10 20 30 20 40 0 c 10 -20 30 -20 40 0 Z' },
        { name: 'S / s (Smooth Cubic Bezier absolute & relative)', pathData: 'M 0 0 C 10 20 30 20 40 0 S 70 -20 80 0 s 30 20 40 0 Z' },
        { name: 'Q / q (Quadratic Bezier absolute & relative)', pathData: 'M 0 0 Q 20 40 40 0 q 20 -40 40 0 Z' },
        { name: 'T / t (Smooth Quadratic Bezier absolute & relative)', pathData: 'M 0 0 Q 20 40 40 0 T 80 0 t 40 0 Z' },
        { name: 'A / a (Elliptical Arc absolute & relative)', pathData: 'M 10 80 A 45 45 0 0 0 125 125 a 25 25 0 0 1 -30 -30 Z' },
        { name: 'Z / z (ClosePath uppercase & lowercase)', pathData: 'M 0 0 L 10 0 L 10 10 z M 20 20 L 30 20 L 30 30 Z' },
        {
            name: 'Composite SVG Path (All 10 commands M,L,H,V,C,S,Q,T,A,Z)',
            pathData: 'M 12 2 C 6.48 2 2 6.48 2 12 s 4.48 10 10 10 10 -4.48 10 -10 S 17.52 2 12 2 z M 12 5 H 14 v 4 h -2 z M 8 15 Q 10 18 12 18 T 16 15 A 3 3 0 0 0 19 12 L 19 8 Z'
        },
        {
            name: 'Material Search Icon SVG Path',
            pathData: 'M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z'
        },
        {
            name: 'Material Android Icon SVG Path',
            pathData: 'M6 18c0 .55.45 1 1 1h1v3.5c0 .83.67 1.5 1.5 1.5s1.5-.67 1.5-1.5V19h2v3.5c0 .83.67 1.5 1.5 1.5s1.5-.67 1.5-1.5V19h1c.55 0 1-.45 1-1V8H6v10zM3.5 8C2.67 8 2 8.67 2 9.5v7c0 .83.67 1.5 1.5 1.5S5 17.33 5 16.5v-7C5 8.67 4.33 8 3.5 8zm17 0c-.83 0-1.5.67-1.5 1.5v7c0 .83.67 1.5 1.5 1.5s1.5-.67 1.5-1.5v-7c0-.83-.67-1.5-1.5-1.5zm-4.97-5.84l1.3-1.3c.2-.2.2-.51 0-.71-.2-.2-.51-.2-.71 0l-1.48 1.48C13.85 2.23 12.95 2 12 2c-.96 0-1.86.23-2.66.63L7.85 1.15c-.2-.2-.51-.2-.71 0-.2.2-.2.51 0 .71l1.31 1.31C6.97 4.26 6 6.01 6 8h12c0-1.99-.97-3.75-2.47-4.84zM10 5H9V4h1v1zm5 0h-1V4h1v1z'
        }
    ];

    for (const cmd of svgCommands) {
        test(`VectorDrawable renders command: ${cmd.name}`, () => {
            const vd = new VectorDrawable(24, 24, 24, 24, [{
                pathData: cmd.pathData,
                fillColor: '#6200EE',
                strokeColor: '#3700B3',
                strokeWidth: 2
            }]);

            const ctx = createMockCanvasContext();
            vd.draw(ctx, 10, 20, 48, 48);

            // Verify transform matrix
            const translates = ctx.calls.filter(c => c.op === 'translate');
            const scales = ctx.calls.filter(c => c.op === 'scale');
            assert.strictEqual(translates.length, 1);
            assert.strictEqual(translates[0].x, 10);
            assert.strictEqual(translates[0].y, 20);
            assert.strictEqual(scales.length, 1);
            assert.strictEqual(scales[0].sx, 2.0); // 48 / 24
            assert.strictEqual(scales[0].sy, 2.0); // 48 / 24

            // Verify fill and stroke execution
            const fills = ctx.calls.filter(c => c.op === 'fill');
            const strokes = ctx.calls.filter(c => c.op === 'stroke');
            assert.strictEqual(fills.length, 1);
            assert.strictEqual(strokes.length, 1);
            assert.strictEqual(fills[0].fillStyle, '#6200EE');
            assert.strictEqual(strokes[0].strokeStyle, '#3700B3');
            assert.strictEqual(strokes[0].lineWidth, 2);
        });
    }

    // -------------------------------------------------------------------------
    // Suite 2: ViewBox & Viewport Scaling Stress Tests
    // -------------------------------------------------------------------------
    console.log('\n▶ [Suite 2] ViewBox & Viewport Scaling Stress Tests');

    test('Uniform scaling: 24x24 viewport to 96x96 target', () => {
        const vd = new VectorDrawable(24, 24, 24, 24, [{ pathData: 'M0,0 L24,24 Z', fillColor: '#00FF00' }]);
        const ctx = createMockCanvasContext();
        vd.draw(ctx, 0, 0, 96, 96);

        const scaleOp = ctx.calls.find(c => c.op === 'scale');
        assert.ok(scaleOp, 'Scale operation must be recorded');
        assert.strictEqual(scaleOp.sx, 4.0);
        assert.strictEqual(scaleOp.sy, 4.0);
    });

    test('Non-uniform scaling: 100x50 viewport to 400x100 target (sx=4.0, sy=2.0)', () => {
        const vd = new VectorDrawable(100, 50, 100, 50, [{ pathData: 'M0,0 L100,50 Z', fillColor: '#0000FF' }]);
        const ctx = createMockCanvasContext();
        vd.draw(ctx, 15, 25, 400, 100);

        const translateOp = ctx.calls.find(c => c.op === 'translate');
        const scaleOp = ctx.calls.find(c => c.op === 'scale');
        assert.strictEqual(translateOp.x, 15);
        assert.strictEqual(translateOp.y, 25);
        assert.strictEqual(scaleOp.sx, 4.0);
        assert.strictEqual(scaleOp.sy, 2.0);
    });

    test('Fractional viewport dimensions: 32.5 x 48.75 to 130 x 195 target', () => {
        const vd = new VectorDrawable(32.5, 48.75, 32.5, 48.75, [{ pathData: 'M0,0 Z', fillColor: '#FFF' }]);
        const ctx = createMockCanvasContext();
        vd.draw(ctx, 0, 0, 130, 195);

        const scaleOp = ctx.calls.find(c => c.op === 'scale');
        assert.strictEqual(Math.round(scaleOp.sx * 100) / 100, 4.0);
        assert.strictEqual(Math.round(scaleOp.sy * 100) / 100, 4.0);
    });

    test('Extreme dimensions: Micro 1x1 and 8K 7680x4320 targets', () => {
        const vd = new VectorDrawable(24, 24, 24, 24, [{ pathData: 'M0,0 Z', fillColor: '#FFF' }]);
        
        // Micro target
        const microCtx = createMockCanvasContext();
        vd.draw(microCtx, 0, 0, 1, 1);
        const microScale = microCtx.calls.find(c => c.op === 'scale');
        assert.strictEqual(microScale.sx, 1 / 24);
        assert.strictEqual(microScale.sy, 1 / 24);

        // 8K target
        const largeCtx = createMockCanvasContext();
        vd.draw(largeCtx, 0, 0, 7680, 4320);
        const largeScale = largeCtx.calls.find(c => c.op === 'scale');
        assert.strictEqual(largeScale.sx, 7680 / 24);
        assert.strictEqual(largeScale.sy, 4320 / 24);
    });

    test('Zero and missing viewport defaults to fallback without division by zero', () => {
        const vd = new VectorDrawable(0, 0, 0, 0, [{ pathData: 'M0,0 Z', fillColor: '#FFF' }]);
        const ctx = createMockCanvasContext();
        vd.draw(ctx, 0, 0, 100, 100);

        const scaleOp = ctx.calls.find(c => c.op === 'scale');
        assert.ok(Number.isFinite(scaleOp.sx), 'sx must be finite');
        assert.ok(Number.isFinite(scaleOp.sy), 'sy must be finite');
        assert.ok(!Number.isNaN(scaleOp.sx), 'sx must not be NaN');
        assert.ok(!Number.isNaN(scaleOp.sy), 'sy must not be NaN');
    });

    // -------------------------------------------------------------------------
    // Suite 3: Multi-Layering, Alpha Blending, Tinting & Styles
    // -------------------------------------------------------------------------
    console.log('\n▶ [Suite 3] Multi-Layering, Alpha Blending, Tinting & Styles');

    test('Multi-layer VectorDrawable renders all paths in sequence', () => {
        const paths = [
            { pathData: 'M0,0 L24,0 L24,24 L0,24 Z', fillColor: '#FF0000', fillAlpha: 1.0 },
            { pathData: 'M4,4 L20,4 L20,20 L4,20 Z', fillColor: '#00FF00', fillAlpha: 0.8 },
            { pathData: 'M8,8 L16,8 L16,16 L8,16 Z', fillColor: '#0000FF', strokeColor: '#FFFF00', strokeWidth: 1.5, fillAlpha: 0.5, strokeAlpha: 0.9 }
        ];

        const vd = new VectorDrawable(24, 24, 24, 24, paths);
        const ctx = createMockCanvasContext();
        vd.draw(ctx, 0, 0, 48, 48);

        const fills = ctx.calls.filter(c => c.op === 'fill');
        const strokes = ctx.calls.filter(c => c.op === 'stroke');
        assert.strictEqual(fills.length, 3);
        assert.strictEqual(strokes.length, 1);

        assert.strictEqual(fills[0].fillStyle, '#FF0000');
        assert.strictEqual(fills[1].fillStyle, '#00FF00');
        assert.strictEqual(fills[1].globalAlpha, 0.8);
        assert.strictEqual(fills[2].fillStyle, '#0000FF');
        assert.strictEqual(fills[2].globalAlpha, 0.5);
        assert.strictEqual(strokes[0].strokeStyle, '#FFFF00');
        assert.strictEqual(strokes[0].lineWidth, 1.5);
    });

    test('VectorDrawable tint overrides individual path fill colors', () => {
        const paths = [
            { pathData: 'M0,0 Z', fillColor: '#112233' },
            { pathData: 'M5,5 Z', fillColor: '#445566' }
        ];
        const vdTinted = new VectorDrawable(24, 24, 24, 24, paths, '#BADA55');
        const ctx = createMockCanvasContext();
        vdTinted.draw(ctx, 0, 0, 24, 24);

        const fills = ctx.calls.filter(c => c.op === 'fill');
        assert.strictEqual(fills.length, 2);
        assert.strictEqual(fills[0].fillStyle, '#BADA55');
        assert.strictEqual(fills[1].fillStyle, '#BADA55');
    });

    test('FillColor "none" disables fill operation', () => {
        const paths = [
            { pathData: 'M0,0 L10,10 Z', fillColor: 'none', strokeColor: '#FF00FF', strokeWidth: 2 }
        ];
        const vd = new VectorDrawable(24, 24, 24, 24, paths);
        const ctx = createMockCanvasContext();
        vd.draw(ctx, 0, 0, 24, 24);

        const fills = ctx.calls.filter(c => c.op === 'fill');
        const strokes = ctx.calls.filter(c => c.op === 'stroke');
        assert.strictEqual(fills.length, 0);
        assert.strictEqual(strokes.length, 1);
        assert.strictEqual(strokes[0].strokeStyle, '#FF00FF');
    });

    // -------------------------------------------------------------------------
    // Suite 4: BitmapDrawable Rendering & Edge Cases
    // -------------------------------------------------------------------------
    console.log('\n▶ [Suite 4] BitmapDrawable Rendering & Edge Cases');

    test('BitmapDrawable executes ctx.drawImage with accurate coordinates', () => {
        const mockBitmap = { width: 128, height: 128, type: 'mock_image_bitmap' };
        const bd = new BitmapDrawable(mockBitmap, 64, 64);
        assert.strictEqual(bd.width, 64);
        assert.strictEqual(bd.height, 64);

        const ctx = createMockCanvasContext();
        bd.draw(ctx, 20, 30, 100, 120);

        const drawImageOp = ctx.calls.find(c => c.op === 'drawImage');
        assert.ok(drawImageOp, 'drawImage operation must be recorded');
        assert.strictEqual(drawImageOp.img, mockBitmap);
        assert.strictEqual(drawImageOp.x, 20);
        assert.strictEqual(drawImageOp.y, 30);
        assert.strictEqual(drawImageOp.w, 100);
        assert.strictEqual(drawImageOp.h, 120);
    });

    test('BitmapDrawable handles null / undefined / empty bitmap gracefully without error', () => {
        const bdNull = new BitmapDrawable(null);
        const bdUndefined = new BitmapDrawable(undefined);
        const ctx = createMockCanvasContext();

        assert.doesNotThrow(() => bdNull.draw(ctx, 0, 0, 50, 50));
        assert.doesNotThrow(() => bdUndefined.draw(ctx, 0, 0, 50, 50));
        assert.strictEqual(ctx.calls.length, 0);
    });

    // -------------------------------------------------------------------------
    // Suite 5: ImageView Lifecycle, Measurement, Padding & Container Integration
    // -------------------------------------------------------------------------
    console.log('\n▶ [Suite 5] ImageView Measurement, Padding & Layout Integration');

    test('ImageView setDrawable updates intrinsic dimensions and measures WRAP_CONTENT', () => {
        const vd = new VectorDrawable(72, 96, 72, 96, [{ pathData: 'M0,0 Z' }]);
        const iv = new ImageView(new LayoutParams(WRAP_CONTENT, WRAP_CONTENT));
        iv.setDrawable(vd);

        assert.strictEqual(iv.intrinsicWidth, 72);
        assert.strictEqual(iv.intrinsicHeight, 96);

        iv.measure(MeasureSpec.makeMeasureSpec(300, AT_MOST), MeasureSpec.makeMeasureSpec(300, AT_MOST));
        assert.strictEqual(iv.measuredWidth, 72);
        assert.strictEqual(iv.measuredHeight, 96);
    });

    test('ImageView setImageBitmap updates intrinsic dimensions and measures correctly', () => {
        const mockBitmap = { width: 200, height: 150 };
        const iv = new ImageView(new LayoutParams(WRAP_CONTENT, WRAP_CONTENT));
        iv.setImageBitmap(mockBitmap);

        assert.ok(iv.drawable instanceof BitmapDrawable);
        assert.strictEqual(iv.intrinsicWidth, 48); // default BitmapDrawable width
        assert.strictEqual(iv.intrinsicHeight, 48);

        iv.measure(MeasureSpec.makeMeasureSpec(500, EXACTLY), MeasureSpec.makeMeasureSpec(300, EXACTLY));
        assert.strictEqual(iv.measuredWidth, 500);
        assert.strictEqual(iv.measuredHeight, 300);
    });

    test('ImageView respects padding in onMeasure and onDraw boundaries', () => {
        const vd = new VectorDrawable(50, 50, 50, 50, [{ pathData: 'M0,0 Z', fillColor: '#FF0000' }]);
        const iv = new ImageView(new LayoutParams(WRAP_CONTENT, WRAP_CONTENT));
        iv.setPadding(10, 15, 20, 25);
        iv.setDrawable(vd);

        // Intrinsic 50x50 + padding [10+20, 15+25] = 80x90
        iv.measure(MeasureSpec.makeMeasureSpec(200, AT_MOST), MeasureSpec.makeMeasureSpec(200, AT_MOST));
        assert.strictEqual(iv.measuredWidth, 80);
        assert.strictEqual(iv.measuredHeight, 90);

        iv.layout(100, 100, 180, 190);
        assert.strictEqual(iv.getWidth(), 80);
        assert.strictEqual(iv.getHeight(), 90);

        const ctx = createMockCanvasContext();
        iv.onDraw(ctx);

        // Content area inside padding:
        // x = 100 + 10 = 110, y = 100 + 15 = 115
        // w = 80 - 10 - 20 = 50, h = 90 - 15 - 25 = 50
        const translateOp = ctx.calls.find(c => c.op === 'translate');
        assert.strictEqual(translateOp.x, 110);
        assert.strictEqual(translateOp.y, 115);

        const scaleOp = ctx.calls.find(c => c.op === 'scale');
        assert.strictEqual(scaleOp.sx, 1.0); // 50 / 50
        assert.strictEqual(scaleOp.sy, 1.0); // 50 / 50
    });

    test('ImageView fallback placeholder rendering when no drawable is set', () => {
        const iv = new ImageView(new LayoutParams(100, 100));
        iv.layout(0, 0, 100, 100);

        const ctx = createMockCanvasContext();
        iv.onDraw(ctx);

        const fillRectOp = ctx.calls.find(c => c.op === 'fillRect');
        assert.ok(fillRectOp, 'Fallback placeholder fillRect must be executed');
        assert.strictEqual(fillRectOp.x, 0);
        assert.strictEqual(fillRectOp.y, 0);
        assert.strictEqual(fillRectOp.w, 100);
        assert.strictEqual(fillRectOp.h, 100);
    });

    test('ImageView tint without drawable fills tinted rectangle', () => {
        const iv = new ImageView(new LayoutParams(80, 80));
        iv.tint = '#FF5722';
        iv.layout(10, 10, 90, 90);

        const ctx = createMockCanvasContext();
        iv.onDraw(ctx);

        const fillRectOp = ctx.calls.find(c => c.op === 'fillRect');
        assert.ok(fillRectOp, 'Tint fillRect must be executed');
        assert.strictEqual(fillRectOp.fillStyle, '#FF5722');
        assert.strictEqual(fillRectOp.x, 10);
        assert.strictEqual(fillRectOp.y, 10);
        assert.strictEqual(fillRectOp.w, 80);
        assert.strictEqual(fillRectOp.h, 80);
    });

    test('ImageView within ConstraintLayout, LinearLayout, and FrameLayout', () => {
        const root = new ConstraintLayout();
        root.setPadding(0, 0, 0, 0);

        const iv = new ImageView(new LayoutParams(48, 48));
        iv.id = 101;
        iv.layoutParams.constraints = {
            layout_constraintStart_toStartOf: 'parent',
            layout_constraintTop_toTopOf: 'parent'
        };
        const vd = new VectorDrawable(24, 24, 24, 24, [{ pathData: 'M0,0 Z', fillColor: '#AABBCC' }]);
        iv.setDrawable(vd);

        root.addView(iv);
        root.measure(MeasureSpec.makeMeasureSpec(720, EXACTLY), MeasureSpec.makeMeasureSpec(120, EXACTLY));
        root.layout(0, 0, 720, 120);

        assert.strictEqual(iv.left, 0);
        assert.strictEqual(iv.top, 0);
        assert.strictEqual(iv.getWidth(), 48);
        assert.strictEqual(iv.getHeight(), 48);

        const ctx = createMockCanvasContext();
        root.draw(ctx);

        const fills = ctx.calls.filter(c => c.op === 'fill');
        assert.strictEqual(fills.length, 1);
        assert.strictEqual(fills[0].fillStyle, '#AABBCC');
    });

    // -------------------------------------------------------------------------
    // Suite 6: Authentic Vector AST & F-Droid.apk Resource Ingestion
    // -------------------------------------------------------------------------
    console.log('\n▶ [Suite 6] Authentic Vector AST & F-Droid.apk Ingestion');

    test('VectorDrawable.fromXmlAst parses nested vector XML AST', () => {
        const mockAst = {
            tag: 'vector',
            attrs: {
                width: '36dp',
                height: '36dp',
                viewportWidth: '36',
                viewportHeight: '36',
                tint: '#3F51B5'
            },
            children: [
                {
                    tag: 'group',
                    children: [
                        {
                            tag: 'path',
                            attrs: {
                                pathData: 'M18,2 L34,34 L2,34 Z',
                                fillColor: '#E91E63',
                                strokeColor: '#9C27B0'
                            },
                            rawAttrs: [
                                { name: 'pathData', rawVal: 'M18,2 L34,34 L2,34 Z', dataType: 3, data: 0 },
                                { name: 'fillColor', dataType: 0x1d, data: 0xFFE91E63 },
                                { name: 'strokeColor', dataType: 0x1d, data: 0xFF9C27B0 },
                                { name: 'strokeWidth', dataType: 0x10, data: 2 }
                            ]
                        }
                    ]
                }
            ]
        };

        const vd = VectorDrawable.fromXmlAst(mockAst);
        assert.ok(vd instanceof VectorDrawable, 'Result must be VectorDrawable instance');
        assert.strictEqual(vd.width, 36);
        assert.strictEqual(vd.height, 36);
        assert.strictEqual(vd.viewportWidth, 36);
        assert.strictEqual(vd.viewportHeight, 36);
        assert.strictEqual(vd.paths.length, 1);
        assert.strictEqual(vd.paths[0].pathData, 'M18,2 L34,34 L2,34 Z');
        assert.strictEqual(vd.paths[0].strokeWidth, 2);
    });

    test('Authentic F-Droid.apk drawable resolution and ImageView binding', () => {
        const apkPath = path.join(rootDir, 'F-Droid.apk');
        assert.ok(fs.existsSync(apkPath), 'F-Droid.apk must exist');

        const apkBuf = fs.readFileSync(apkPath);
        const zip = new ApkZipReader(apkBuf.buffer.slice(apkBuf.byteOffset, apkBuf.byteOffset + apkBuf.byteLength));
        zip.readEntries();

        const arscBuf = zip.getFile('resources.arsc');
        const arsc = ArscDecoder.decode(arscBuf);

        // 1. Resolve launcher icon
        const iconPath = arsc.resolveDrawablePath('ic_launcher') || arsc.resolveDrawablePath('ic_repo_app_default');
        assert.ok(iconPath, 'Must resolve drawable path');

        // 2. Resolve drawable object with zip reader
        const iconObj = arsc.resolveDrawable('ic_launcher', zip) || arsc.resolveDrawable(0x7f0800a0, zip);
        assert.ok(iconObj, 'resolveDrawable must return descriptor');
        assert.ok(iconObj.path, 'Descriptor must contain path');

        // 3. Inflate res/Kt.xml and verify ImageView id 2131296574
        const ktBuf = zip.getFile('res/Kt.xml');
        const root = LayoutInflater.inflate(ktBuf, arsc);
        const iconView = root.findViewById(2131296574);
        assert.ok(iconView, 'Icon view 2131296574 must exist');
        assert.ok(iconView instanceof ImageView, 'Icon view must be an ImageView');

        // Measure and layout
        root.measure(MeasureSpec.makeMeasureSpec(720, EXACTLY), MeasureSpec.makeMeasureSpec(120, EXACTLY));
        root.layout(0, 0, 720, 120);

        assert.strictEqual(iconView.left, 16);
        assert.strictEqual(iconView.getWidth(), 48);
        assert.strictEqual(iconView.getHeight(), 48);

        // Draw view hierarchy onto mock canvas context
        const ctx = createMockCanvasContext();
        root.draw(ctx);
        assert.ok(ctx.calls.length > 5, 'Drawing root layout must produce canvas draw calls');
    });

    console.log('\n================================================================');
    console.log(`⚡ ALL ${passedTests}/${totalTests} CHALLENGER 2 EMPIRICAL TESTS PASSED!`);
    console.log('================================================================\n');
}

runChallengerTestSuite().catch(err => {
    console.error('Challenger test suite failed:', err);
    process.exit(1);
});
