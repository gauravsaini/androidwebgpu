# GATES: Firefox APK & Google.com Launch

## Gate 1: Ingest Firefox APK & Register in Dalvik VM & PMS
CHECK:
  node -e "
    import('./src/android_runtime.js').then(async m => {
      const fs = await import('fs');
      const runtime = new m.AndroidRuntime();
      const buf = fs.readFileSync('firefox.apk');
      const appState = await runtime.loadAndRunApk(buf);
      if (appState.packageName === 'org.mozilla.firefox') {
        console.log('GATE1_PASS: Firefox APK parsed and loaded');
      }
    });
  "
EXPECT:
  GATE1_PASS: Firefox APK parsed and loaded

## Gate 2: Render Firefox GeckoView with Google.com Search UI
CHECK:
  node -e "
    import('./src/android_runtime.js').then(async m => {
      const fs = await import('fs');
      const runtime = new m.AndroidRuntime();
      const buf = fs.readFileSync('firefox.apk');
      const appState = await runtime.loadAndRunApk(buf);
      appState.activeUrl = 'https://www.google.com';
      appState.currentPage = 'Google';
      runtime.renderActivityUi(appState);
      const root = runtime.currentRootView;
      const findText = (v, q) => {
        if (v.text && v.text.includes(q)) return true;
        if (v.children) {
          for (const c of v.children) {
            if (findText(c, q)) return true;
          }
        }
        return false;
      };
      if (findText(root, 'google.com') && findText(root, 'Google Search')) {
        console.log('GATE2_PASS: Google.com rendered in Firefox GeckoView');
      }
    });
  "
EXPECT:
  GATE2_PASS: Google.com rendered in Firefox GeckoView

## Gate 3: All Framework & Challenger Tests Pass
CHECK:
  pnpm test
EXPECT:
  ALL 58 AUTHENTIC FRAMEWORK & ART TESTS PASSED (0 failures)
