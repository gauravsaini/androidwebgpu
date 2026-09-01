# GATES: Android APK UI Rendering & App Lifecycle

## Gate 1: Firefox GeckoView & Google.com UI Rendering
CHECK:
  node -e "
    import('./src/android_runtime.js').then(async m => {
      const fs = await import('fs');
      const { ViewHierarchyRasterizer } = await import('./src/view_rasterizer.js');
      const runtime = new m.AndroidRuntime();
      const buf = fs.readFileSync('firefox.apk');
      const appState = await runtime.loadAndRunApk(buf);
      appState.activeUrl = 'https://www.google.com';
      appState.currentPage = 'Google';
      runtime.renderActivityUi(appState);
      const root = runtime.currentRootView;

      // Check 1: View tree text tokens (existing)
      const findText = (v, q) => {
        if (v.text && v.text.includes(q)) return true;
        if (v.children) {
          for (const c of v.children) {
            if (findText(c, q)) return true;
          }
        }
        return false;
      };
      if (!findText(root, 'google.com') || !findText(root, 'Google Search')) {
        console.error('GATE1_FAIL: View tree missing expected text tokens');
        process.exit(1);
      }

      // Check 2: Rasterize to RGBA and verify pixel output
      const rasterizer = new ViewHierarchyRasterizer(720, 1440);
      const frame = rasterizer.rasterize(root, 720, 1440);
      const rgba = frame.rgbaData;
      const totalPixels = rgba.length / 4;

      // Shannon entropy
      const freq = new Map();
      let bgCount = 0;
      for (let i = 0; i < rgba.length; i += 4) {
        const r = rgba[i], g = rgba[i+1], b = rgba[i+2], a = rgba[i+3];
        const color = (r << 24) | (g << 16) | (b << 8) | a;
        freq.set(color, (freq.get(color) || 0) + 1);
        if (r === 15 && g === 23 && b === 42 && a === 255) bgCount++;
      }
      let entropy = 0;
      for (const count of freq.values()) {
        const p = count / totalPixels;
        if (p > 0) entropy -= p * Math.log2(p);
      }

      if (entropy < 2.0) { console.error('GATE1_FAIL: Entropy ' + entropy.toFixed(3) + ' < 2.0'); process.exit(1); }
      if (freq.size < 50) { console.error('GATE1_FAIL: Only ' + freq.size + ' unique colors (need >= 50)'); process.exit(1); }
      const bgRatio = bgCount / totalPixels;
      if (bgRatio > 0.85) { console.error('GATE1_FAIL: Background dominance ' + (bgRatio*100).toFixed(1) + '% > 85%'); process.exit(1); }

      // Check 3: Spatial region sampling — header, body, footer must have non-bg pixels
      const sampleRegion = (startRow, endRow) => {
        let nonBg = 0;
        for (let y = startRow; y < endRow; y++) {
          for (let x = 0; x < 720; x++) {
            const i = (y * 720 + x) * 4;
            if (!(rgba[i] === 15 && rgba[i+1] === 23 && rgba[i+2] === 42)) nonBg++;
          }
        }
        return nonBg;
      };
      const headerNonBg = sampleRegion(0, 144);     // top 10%
      const bodyNonBg = sampleRegion(360, 1080);     // middle 50%
      const footerNonBg = sampleRegion(1296, 1440);  // bottom 10%
      if (headerNonBg === 0) { console.error('GATE1_FAIL: Header region all background'); process.exit(1); }
      if (bodyNonBg === 0) { console.error('GATE1_FAIL: Body region all background'); process.exit(1); }
      if (footerNonBg === 0) { console.error('GATE1_FAIL: Footer region all background'); process.exit(1); }

      console.log('GATE1_PASS: Google.com rendered in Firefox GeckoView (H=' + entropy.toFixed(2) + ', colors=' + freq.size + ', bgRatio=' + (bgRatio*100).toFixed(1) + '%)');
    });
  "
EXPECT:
  GATE1_PASS: Google.com rendered in Firefox GeckoView

## Gate 2: F-Droid App Store Catalog & Package List Rendering
CHECK:
  node -e "
    import('./src/android_runtime.js').then(async m => {
      const fs = await import('fs');
      const { ViewHierarchyRasterizer } = await import('./src/view_rasterizer.js');
      const runtime = new m.AndroidRuntime();
      const buf = fs.readFileSync('F-Droid.apk');
      const appState = await runtime.loadAndRunApk(buf);
      runtime.renderActivityUi(appState);
      const root = runtime.currentRootView;

      // Check 1: View tree text
      const findText = (v, q) => {
        if (v.text && v.text.includes(q)) return true;
        if (v.children) { for (const c of v.children) { if (findText(c, q)) return true; } }
        return false;
      };
      if (!findText(root, 'F-Droid') || !(findText(root, 'Latest') || findText(root, 'Categories') || findText(root, 'Search'))) {
        console.error('GATE2_FAIL: View tree missing F-Droid text tokens'); process.exit(1);
      }

      // Check 2: Rasterize and verify
      const rasterizer = new ViewHierarchyRasterizer(720, 1440);
      const frame = rasterizer.rasterize(root, 720, 1440);
      const rgba = frame.rgbaData;
      const totalPixels = rgba.length / 4;
      const freq = new Map();
      let bgCount = 0;
      for (let i = 0; i < rgba.length; i += 4) {
        const r = rgba[i], g = rgba[i+1], b = rgba[i+2], a = rgba[i+3];
        freq.set((r << 24) | (g << 16) | (b << 8) | a, (freq.get((r << 24) | (g << 16) | (b << 8) | a) || 0) + 1);
        if (r === 15 && g === 23 && b === 42 && a === 255) bgCount++;
      }
      let entropy = 0;
      for (const count of freq.values()) { const p = count / totalPixels; if (p > 0) entropy -= p * Math.log2(p); }

      if (entropy < 2.0) { console.error('GATE2_FAIL: Entropy ' + entropy.toFixed(3) + ' < 2.0'); process.exit(1); }
      if (freq.size < 50) { console.error('GATE2_FAIL: Only ' + freq.size + ' unique colors'); process.exit(1); }
      if (bgCount / totalPixels > 0.85) { console.error('GATE2_FAIL: Background dominance > 85%'); process.exit(1); }

      // Check 3: Spatial regions
      const sampleRegion = (startRow, endRow) => {
        let nonBg = 0;
        for (let y = startRow; y < endRow; y++)
          for (let x = 0; x < 720; x++) {
            const i = (y * 720 + x) * 4;
            if (!(rgba[i] === 15 && rgba[i+1] === 23 && rgba[i+2] === 42)) nonBg++;
          }
        return nonBg;
      };
      if (sampleRegion(0, 144) === 0) { console.error('GATE2_FAIL: Header blank'); process.exit(1); }
      if (sampleRegion(360, 1080) === 0) { console.error('GATE2_FAIL: Body blank'); process.exit(1); }

      console.log('GATE2_PASS: F-Droid catalog UI rendered (H=' + entropy.toFixed(2) + ', colors=' + freq.size + ')');
    });
  "
EXPECT:
  GATE2_PASS: F-Droid catalog UI rendered

## Gate 3: Core Android 14 System Apps UI Rendering (Settings, Terminal, Files)
CHECK:
  node -e "
    import('./src/android_runtime.js').then(async m => {
      const { ViewHierarchyRasterizer } = await import('./src/view_rasterizer.js');
      const runtime = new m.AndroidRuntime();
      const rasterizer = new ViewHierarchyRasterizer(720, 1440);

      const verifyApp = (pkgName, actName, label) => {
        runtime.startActivity(pkgName, actName);
        const root = runtime.currentRootView;
        const findText = (v, q) => {
          if (v.text && v.text.includes(q)) return true;
          if (v.children) { for (const c of v.children) { if (findText(c, q)) return true; } }
          return false;
        };
        if (!findText(root, label)) {
          console.error('GATE3_FAIL: ' + label + ' text not found in view tree');
          process.exit(1);
        }

        // Rasterize and verify pixel output
        const frame = rasterizer.rasterize(root, 720, 1440);
        const rgba = frame.rgbaData;
        const totalPixels = rgba.length / 4;
        const freq = new Map();
        let bgCount = 0;
        for (let i = 0; i < rgba.length; i += 4) {
          const r = rgba[i], g = rgba[i+1], b = rgba[i+2], a = rgba[i+3];
          const color = (r << 24) | (g << 16) | (b << 8) | a;
          freq.set(color, (freq.get(color) || 0) + 1);
          if (r === 15 && g === 23 && b === 42 && a === 255) bgCount++;
        }
        let entropy = 0;
        for (const count of freq.values()) { const p = count / totalPixels; if (p > 0) entropy -= p * Math.log2(p); }

        if (entropy < 2.0) { console.error('GATE3_FAIL: ' + label + ' entropy ' + entropy.toFixed(3) + ' < 2.0'); process.exit(1); }
        if (freq.size < 30) { console.error('GATE3_FAIL: ' + label + ' only ' + freq.size + ' unique colors'); process.exit(1); }
        if (bgCount / totalPixels > 0.85) { console.error('GATE3_FAIL: ' + label + ' background dominance > 85%'); process.exit(1); }

        // Header region must have non-bg pixels
        let headerNonBg = 0;
        for (let y = 0; y < 144; y++)
          for (let x = 0; x < 720; x++) {
            const i = (y * 720 + x) * 4;
            if (!(rgba[i] === 15 && rgba[i+1] === 23 && rgba[i+2] === 42)) headerNonBg++;
          }
        if (headerNonBg === 0) { console.error('GATE3_FAIL: ' + label + ' header blank'); process.exit(1); }

        console.log('  ' + label + ': H=' + entropy.toFixed(2) + ' colors=' + freq.size + ' bgRatio=' + (bgCount/totalPixels*100).toFixed(1) + '%');
      };

      verifyApp('com.android.settings', 'com.android.settings.SettingsActivity', 'Settings');
      verifyApp('com.android.terminal', 'com.android.terminal.TerminalActivity', 'Terminal');
      verifyApp('com.android.files', 'com.android.files.FilesActivity', 'Files');

      console.log('GATE3_PASS: Core Android 14 system apps rendered');
    });
  "
EXPECT:
  GATE3_PASS: Core Android 14 system apps rendered

## Gate 4: Generic & Custom APK Layout Discovery and Inflation
CHECK:
  node -e "
    import('./src/android_runtime.js').then(async m => {
      const { ViewHierarchyRasterizer } = await import('./src/view_rasterizer.js');
      const runtime = new m.AndroidRuntime();
      const appState = {
        packageName: 'com.example.customapp',
        appName: 'Custom App',
        manifest: { activities: ['com.example.customapp.MainActivity'], targetSdkVersion: 34 },
        packageInfo: { packageName: 'com.example.customapp', appName: 'Custom App', versionName: '2.1.0' }
      };
      runtime.renderActivityUi(appState);
      const root = runtime.currentRootView;

      // Check 1: View tree text
      const findText = (v, q) => {
        if (v.text && v.text.includes(q)) return true;
        if (v.children) { for (const c of v.children) { if (findText(c, q)) return true; } }
        return false;
      };
      if (!findText(root, 'Custom App') || !findText(root, 'com.example.customapp')) {
        console.error('GATE4_FAIL: View tree missing Custom App text tokens'); process.exit(1);
      }

      // Check 2: Rasterize and verify
      const rasterizer = new ViewHierarchyRasterizer(720, 1440);
      const frame = rasterizer.rasterize(root, 720, 1440);
      const rgba = frame.rgbaData;
      const totalPixels = rgba.length / 4;
      const freq = new Map();
      let bgCount = 0;
      for (let i = 0; i < rgba.length; i += 4) {
        const r = rgba[i], g = rgba[i+1], b = rgba[i+2], a = rgba[i+3];
        const color = (r << 24) | (g << 16) | (b << 8) | a;
        freq.set(color, (freq.get(color) || 0) + 1);
        if (r === 15 && g === 23 && b === 42 && a === 255) bgCount++;
      }
      let entropy = 0;
      for (const count of freq.values()) { const p = count / totalPixels; if (p > 0) entropy -= p * Math.log2(p); }

      if (entropy < 2.0) { console.error('GATE4_FAIL: Entropy ' + entropy.toFixed(3) + ' < 2.0'); process.exit(1); }
      if (freq.size < 30) { console.error('GATE4_FAIL: Only ' + freq.size + ' unique colors'); process.exit(1); }
      if (bgCount / totalPixels > 0.85) { console.error('GATE4_FAIL: Background dominance > 85%'); process.exit(1); }

      // Check 3: Header region
      let headerNonBg = 0;
      for (let y = 0; y < 144; y++)
        for (let x = 0; x < 720; x++) {
          const i = (y * 720 + x) * 4;
          if (!(rgba[i] === 15 && rgba[i+1] === 23 && rgba[i+2] === 42)) headerNonBg++;
        }
      if (headerNonBg === 0) { console.error('GATE4_FAIL: Header blank'); process.exit(1); }

      console.log('GATE4_PASS: Custom APK UI rendered (H=' + entropy.toFixed(2) + ', colors=' + freq.size + ')');
    });
  "
EXPECT:
  GATE4_PASS: Custom APK UI rendered

## Gate 5: All Framework & Challenger Tests Pass
CHECK:
  pnpm test
EXPECT:
  ALL 58 AUTHENTIC FRAMEWORK & ART TESTS PASSED (0 failures)
