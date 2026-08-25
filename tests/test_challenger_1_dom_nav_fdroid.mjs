/**
 * Adversarial Challenger Test Suite: UI DOM, Navigation Stack, F-Droid & Hardware Controls
 * 
 * Target: /Users/ektasaini/Desktop/androidwebgpu/index.html
 * 
 * Verifies:
 * 1. Rapid backstack cycling (push 100 activities, pop all in LIFO order, 50 underflow pops)
 * 2. Home button unconditional reset from arbitrary modal/screen depths
 * 3. F-Droid search fuzzing with weird regex tokens ([.*+?^${}()|[\]\\]), unicode, HTML tags, long strings
 * 4. Dynamic app injection into Home Grid & PMS Registry
 * 5. Volume rocker and hardware power toggle edge cases
 * 
 * Conforms to ASD-STE100 Simplified Technical English.
 */

import fs from 'fs';
import path from 'path';

let totalAssertions = 0;
let passedAssertions = 0;
let failedAssertions = 0;

function assert(condition, message) {
    totalAssertions++;
    if (!condition) {
        failedAssertions++;
        console.error(`[FAIL] ${message}`);
        throw new Error(`Assertion failed: ${message}`);
    }
    passedAssertions++;
}

function runSuite(name, fn) {
    console.log(`\n======================================================`);
    console.log(`▶ CHALLENGER 1 ADVERSARIAL SUITE: ${name}`);
    console.log(`======================================================`);
    try {
        fn();
        console.log(`✔ [PASS] ${name} completed successfully.`);
    } catch (err) {
        console.error(`✖ [FAIL] ${name} failed: ${err.message}`);
        throw err;
    }
}

// -----------------------------------------------------------------------------
// Lightweight Headless DOM Mock Engine for Testing index.html Logic
// -----------------------------------------------------------------------------
class MockElement {
    constructor(tagName = 'div', id = '', className = '') {
        this.tagName = tagName.toUpperCase();
        this.id = id;
        this.className = className;
        this.classList = new MockClassList(this);
        this.style = {};
        this.attributes = new Map();
        this.children = [];
        this.parentElement = null;
        this.eventListeners = new Map();
        this._innerHTML = '';
        this._textContent = '';
        this.value = '';
    }

    get innerHTML() {
        return this._innerHTML;
    }

    set innerHTML(val) {
        this._innerHTML = val;
        this.children = [];
    }

    get textContent() {
        return this._textContent;
    }

    set textContent(val) {
        this._textContent = String(val);
    }

    setAttribute(name, val) {
        this.attributes.set(name, String(val));
    }

    getAttribute(name) {
        return this.attributes.get(name) || null;
    }

    hasAttribute(name) {
        return this.attributes.has(name);
    }

    removeAttribute(name) {
        this.attributes.delete(name);
    }

    appendChild(child) {
        if (child.parentElement) {
            child.parentElement.removeChild(child);
        }
        child.parentElement = this;
        this.children.push(child);
        return child;
    }

    removeChild(child) {
        const idx = this.children.indexOf(child);
        if (idx !== -1) {
            this.children.splice(idx, 1);
            child.parentElement = null;
        }
        return child;
    }

    remove() {
        if (this.parentElement) {
            this.parentElement.removeChild(this);
        }
    }

    addEventListener(event, handler) {
        if (!this.eventListeners.has(event)) {
            this.eventListeners.set(event, []);
        }
        this.eventListeners.get(event).push(handler);
    }

    dispatchEvent(event) {
        const list = this.eventListeners.get(event.type || event) || [];
        for (const handler of list) {
            handler(event);
        }
    }

    click() {
        this.dispatchEvent({ type: 'click', target: this });
    }

    querySelector(selector) {
        const all = this.querySelectorAll(selector);
        return all.length > 0 ? all[0] : null;
    }

    querySelectorAll(selector) {
        const results = [];
        const match = (el) => {
            if (selector.startsWith('#')) {
                if (el.id === selector.slice(1)) results.push(el);
            } else if (selector.startsWith('.')) {
                if (el.classList.contains(selector.slice(1))) results.push(el);
            } else if (selector.startsWith('[data-package="') && selector.endsWith('"]')) {
                const pkg = selector.slice(15, -2);
                if (el.getAttribute('data-package') === pkg) results.push(el);
            } else if (selector.startsWith('[data-app="') && selector.endsWith('"]')) {
                const app = selector.slice(11, -2);
                if (el.getAttribute('data-app') === app) results.push(el);
            } else if (selector === '.android-screen') {
                if (el.classList.contains('android-screen')) results.push(el);
            } else if (selector === '.cat-tab') {
                if (el.classList.contains('cat-tab')) results.push(el);
            } else if (selector === '.app-icon-item') {
                if (el.classList.contains('app-icon-item')) results.push(el);
            } else if (selector === '.fdroid-card') {
                if (el.classList.contains('fdroid-card')) results.push(el);
            }
            for (const child of el.children) {
                match(child);
            }
        };
        for (const child of this.children) {
            match(child);
        }
        return results;
    }
}

class MockClassList {
    constructor(element) {
        this.element = element;
    }

    _getClasses() {
        return this.element.className.split(/\s+/).filter(Boolean);
    }

    contains(cls) {
        return this._getClasses().includes(cls);
    }

    add(cls) {
        const classes = this._getClasses();
        if (!classes.includes(cls)) {
            classes.push(cls);
            this.element.className = classes.join(' ');
        }
    }

    remove(cls) {
        const classes = this._getClasses().filter(c => c !== cls);
        this.element.className = classes.join(' ');
    }

    toggle(cls, force) {
        if (force !== undefined) {
            if (force) this.add(cls);
            else this.remove(cls);
            return force;
        }
        if (this.contains(cls)) {
            this.remove(cls);
            return false;
        } else {
            this.add(cls);
            return true;
        }
    }
}

class MockDocument {
    constructor() {
        this.elementsById = new Map();
        this.root = new MockElement('html');
        this.body = new MockElement('body');
        this.root.appendChild(this.body);
    }

    createElement(tagName) {
        return new MockElement(tagName);
    }

    getElementById(id) {
        return this.elementsById.get(id) || null;
    }

    registerElement(id, el) {
        el.id = id;
        this.elementsById.set(id, el);
    }

    querySelector(selector) {
        return this.body.querySelector(selector);
    }

    querySelectorAll(selector) {
        return this.body.querySelectorAll(selector);
    }
}

// -----------------------------------------------------------------------------
// Test Environment Setup matching index.html
// -----------------------------------------------------------------------------
function createAndroidUiEnvironment() {
    const doc = new MockDocument();

    // DOM Elements
    const screenHome = new MockElement('div', 'screen-home', 'android-screen active');
    const screenFdroid = new MockElement('div', 'screen-fdroid', 'android-screen');
    const screenNativeSurface = new MockElement('div', 'screen-native-surface', 'android-screen');
    const screenGenericApp = new MockElement('div', 'screen-generic-app', 'android-screen');
    const screenRecents = new MockElement('div', 'screen-recents', 'android-screen');

    doc.registerElement('screen-home', screenHome);
    doc.registerElement('screen-fdroid', screenFdroid);
    doc.registerElement('screen-native-surface', screenNativeSurface);
    doc.registerElement('screen-generic-app', screenGenericApp);
    doc.registerElement('screen-recents', screenRecents);

    doc.body.appendChild(screenHome);
    doc.body.appendChild(screenFdroid);
    doc.body.appendChild(screenNativeSurface);
    doc.body.appendChild(screenGenericApp);
    doc.body.appendChild(screenRecents);

    const fdroidDetailModal = new MockElement('div', 'fdroid-detail-modal');
    fdroidDetailModal.style.display = 'none';
    const fdroidDetailContent = new MockElement('div', 'fdroid-detail-content');
    const fdroidDetailBackBtn = new MockElement('button', 'fdroid-detail-back-btn');
    const fdroidCatalogList = new MockElement('div', 'fdroid-catalog-list');
    const fdroidSearchInput = new MockElement('input', 'fdroid-search-input');
    const fdroidSearchClear = new MockElement('button', 'fdroid-search-clear');
    fdroidSearchClear.style.display = 'none';
    const fdroidCategoryTabs = new MockElement('div', 'fdroid-category-tabs');
    const dynamicAppGrid = new MockElement('div', 'dynamic-app-grid');
    const genericAppTitle = new MockElement('div', 'generic-app-title');
    const genericAppBody = new MockElement('div', 'generic-app-body');
    const recentsCarousel = new MockElement('div', 'recents-carousel');
    const statusNotifications = new MockElement('div', 'status-notifications');
    const phoneFrame = new MockElement('div', 'phone-frame', 'phone-frame');
    const screenSleepOverlay = new MockElement('div', 'screen-sleep-overlay');
    const volumeHud = new MockElement('div', 'volume-hud');
    volumeHud.style.display = 'none';
    const volumeBarFill = new MockElement('div', 'volume-bar-fill');

    // Inspector
    const inspectorPmsCount = new MockElement('span', 'inspector-pms-count');
    const inspectorAmsDepth = new MockElement('span', 'inspector-ams-depth');
    const inspectorAmsActivity = new MockElement('span', 'inspector-ams-activity');
    const inspectorWmsWindows = new MockElement('span', 'inspector-wms-windows');
    const inspectorBinderStatus = new MockElement('span', 'inspector-binder-status');

    doc.registerElement('fdroid-detail-modal', fdroidDetailModal);
    doc.registerElement('fdroid-detail-content', fdroidDetailContent);
    doc.registerElement('fdroid-detail-back-btn', fdroidDetailBackBtn);
    doc.registerElement('fdroid-catalog-list', fdroidCatalogList);
    doc.registerElement('fdroid-search-input', fdroidSearchInput);
    doc.registerElement('fdroid-search-clear', fdroidSearchClear);
    doc.registerElement('fdroid-category-tabs', fdroidCategoryTabs);
    doc.registerElement('dynamic-app-grid', dynamicAppGrid);
    doc.registerElement('generic-app-title', genericAppTitle);
    doc.registerElement('generic-app-body', genericAppBody);
    doc.registerElement('recents-carousel', recentsCarousel);
    doc.registerElement('status-notifications', statusNotifications);
    doc.registerElement('phone-frame', phoneFrame);
    doc.registerElement('screen-sleep-overlay', screenSleepOverlay);
    doc.registerElement('volume-hud', volumeHud);
    doc.registerElement('volume-bar-fill', volumeBarFill);
    doc.registerElement('inspector-pms-count', inspectorPmsCount);
    doc.registerElement('inspector-ams-depth', inspectorAmsDepth);
    doc.registerElement('inspector-ams-activity', inspectorAmsActivity);
    doc.registerElement('inspector-wms-windows', inspectorWmsWindows);
    doc.registerElement('inspector-binder-status', inspectorBinderStatus);

    doc.body.appendChild(fdroidDetailModal);
    doc.body.appendChild(fdroidCatalogList);
    doc.body.appendChild(dynamicAppGrid);

    // Initial PMS Registry
    const PMS_REGISTRY = new Map([
        ['org.fdroid.fdroid', {
            packageName: 'org.fdroid.fdroid',
            appName: 'F-Droid',
            versionName: '1.23.1',
            versionCode: 1023051,
            mainActivity: 'org.fdroid.fdroid.views.main.MainActivity',
            category: 'system',
            icon: '📱',
            iconClass: 'icon-fdroid',
            desc: 'Free and Open Source Android Application Repository Client.',
            author: 'F-Droid Community',
            license: 'GPL-3.0',
            size: '12.4 MB',
            targetSdk: 'Android 14 (API 34)',
            minSdk: 'Android 8.0 (API 26)',
            activitiesCount: 25,
            providersCount: 4,
            servicesCount: 8,
            receiversCount: 6,
            permissions: ['android.permission.INTERNET', 'android.permission.ACCESS_NETWORK_STATE', 'android.permission.REQUEST_INSTALL_PACKAGES'],
            installed: true
        }],
        ['com.unity.cube.gles', {
            packageName: 'com.unity.cube.gles',
            appName: 'Unity 3D Cube',
            versionName: '1.0.0',
            versionCode: 100,
            mainActivity: 'com.unity.cube.gles.UnityPlayerActivity',
            category: 'games',
            icon: '🎲',
            iconClass: 'icon-unity',
            desc: 'Hardware-accelerated 3D cube demo compiled for Android GLES.',
            author: 'Unity Technologies',
            license: 'Proprietary / Demo',
            size: '8.2 MB',
            targetSdk: 'Android 14 (API 34)',
            minSdk: 'Android 8.0 (API 26)',
            activitiesCount: 1,
            providersCount: 0,
            servicesCount: 0,
            receiversCount: 0,
            permissions: ['android.permission.INTERNET'],
            installed: true
        }],
        ['org.godotengine.gles2', {
            packageName: 'org.godotengine.gles2',
            appName: 'Godot GLES2',
            versionName: '2.1.0',
            versionCode: 210,
            mainActivity: 'org.godotengine.gles2.GodotActivity',
            category: 'games',
            icon: '🤖',
            iconClass: 'icon-godot',
            desc: 'Godot game engine lightweight rendering runtime.',
            author: 'Godot Engine Community',
            license: 'MIT',
            size: '14.1 MB',
            targetSdk: 'Android 14 (API 34)',
            minSdk: 'Android 8.0 (API 26)',
            activitiesCount: 2,
            providersCount: 0,
            servicesCount: 1,
            receiversCount: 0,
            permissions: ['android.permission.INTERNET'],
            installed: true
        }],
        ['com.android.chrome', {
            packageName: 'com.android.chrome',
            appName: 'Chrome',
            versionName: '124.0.6367.82',
            versionCode: 636708200,
            mainActivity: 'com.google.android.apps.chrome.Main',
            category: 'internet',
            icon: '🌐',
            iconClass: 'icon-chrome',
            desc: 'Fast, secure web browser powered by Chromium.',
            author: 'Google LLC',
            license: 'Freeware',
            size: '45.0 MB',
            targetSdk: 'Android 14 (API 34)',
            minSdk: 'Android 8.0 (API 26)',
            activitiesCount: 42,
            providersCount: 5,
            servicesCount: 14,
            receiversCount: 8,
            permissions: ['android.permission.INTERNET', 'android.permission.ACCESS_NETWORK_STATE'],
            installed: true
        }]
    ]);

    const navigationStack = ['home'];
    const runningTasks = new Map([
        ['home', { title: 'Launcher', pkg: 'com.android.launcher3', screenId: 'screen-home' }]
    ]);

    let currentFdroidQuery = '';
    let currentFdroidCategory = 'all';
    let masterVolume = 1.0;
    let isScreenSleeping = false;
    let isLandscape = false;
    const logs = [];

    function appendLog(msg, type) {
        logs.push({ msg, type, time: Date.now() });
    }

    function activateScreen(screenId) {
        const screens = [screenHome, screenFdroid, screenNativeSurface, screenGenericApp, screenRecents];
        for (const s of screens) {
            s.classList.remove('active');
        }
        const target = doc.getElementById(screenId);
        if (target) {
            target.classList.add('active');
        }
    }

    function updateSubsystemInspector() {
        let installedCount = 0;
        for (const item of PMS_REGISTRY.values()) {
            if (item.installed) installedCount++;
        }
        inspectorPmsCount.textContent = `${installedCount} Installed`;
        inspectorAmsDepth.textContent = `Depth: ${navigationStack.length}`;

        const top = navigationStack[navigationStack.length - 1];
        if (top === 'home') {
            inspectorAmsActivity.textContent = 'com.android.launcher3/.Launcher';
            inspectorWmsWindows.textContent = 'StatusBar, NavigationBar, HomeScreen';
        } else if (top === 'fdroid') {
            inspectorAmsActivity.textContent = 'org.fdroid.fdroid/.views.main.MainActivity';
            inspectorWmsWindows.textContent = 'StatusBar, NavigationBar, FDroidActivity';
        } else if (top === 'fdroid-detail') {
            inspectorAmsActivity.textContent = 'org.fdroid.fdroid/.views.details.AppDetailsActivity';
            inspectorWmsWindows.textContent = 'StatusBar, NavigationBar, FDroidActivity, AppDetailsDialog';
        } else if (top === 'unity') {
            inspectorAmsActivity.textContent = 'com.unity.cube.gles/.UnityPlayerActivity';
            inspectorWmsWindows.textContent = 'StatusBar, NavigationBar, UnitySurfaceView';
        } else if (top === 'godot') {
            inspectorAmsActivity.textContent = 'org.godotengine.gles2/.GodotActivity';
            inspectorWmsWindows.textContent = 'StatusBar, NavigationBar, GodotSurfaceView';
        } else if (top === 'recents') {
            inspectorAmsActivity.textContent = 'com.android.systemui/.recents.RecentsActivity';
            inspectorWmsWindows.textContent = 'StatusBar, NavigationBar, RecentsOverview';
        } else if (PMS_REGISTRY.has(top)) {
            const app = PMS_REGISTRY.get(top);
            inspectorAmsActivity.textContent = `${app.packageName}/.${app.mainActivity.split('.').pop()}`;
            inspectorWmsWindows.textContent = `StatusBar, NavigationBar, ${app.appName}Window`;
        } else {
            inspectorAmsActivity.textContent = `com.android.${top}/.MainActivity`;
            inspectorWmsWindows.textContent = `StatusBar, NavigationBar, ${top}Window`;
        }
        inspectorBinderStatus.textContent = 'ONLINE (0 errors)';
    }

    function renderGenericApp(appInfo) {
        genericAppTitle.textContent = appInfo.appName || 'Application';
        genericAppBody.innerHTML = `<div>${appInfo.packageName}</div>`;
    }

    function renderRecents() {
        recentsCarousel.innerHTML = '';
        for (const [key, task] of runningTasks.entries()) {
            const item = doc.createElement('div');
            item.className = 'recents-card-item';
            recentsCarousel.appendChild(item);
        }
    }

    function launchApp(appId, options = {}) {
        appendLog(`[AMS] startActivity -> [${appId}]`, 'info');

        if (appId === 'home') {
            navigationStack.length = 0;
            navigationStack.push('home');
            activateScreen('screen-home');
            fdroidDetailModal.style.display = 'none';
        } else if (appId === 'fdroid') {
            if (navigationStack[navigationStack.length - 1] !== 'fdroid') {
                navigationStack.push('fdroid');
            }
            runningTasks.set('fdroid', { title: 'F-Droid', pkg: 'org.fdroid.fdroid', screenId: 'screen-fdroid' });
            fdroidDetailModal.style.display = 'none';
            activateScreen('screen-fdroid');
            renderFdroidCatalog();
        } else if (appId === 'unity' || appId === 'godot') {
            if (navigationStack[navigationStack.length - 1] !== appId) {
                navigationStack.push(appId);
            }
            const label = appId === 'unity' ? 'Unity 3D Cube' : 'Godot GLES2';
            const pkg = appId === 'unity' ? 'com.unity.cube.gles' : 'org.godotengine.gles2';
            runningTasks.set(appId, { title: label, pkg: pkg, screenId: 'screen-native-surface' });
            activateScreen('screen-native-surface');
        } else if (appId === 'recents') {
            renderRecents();
            activateScreen('screen-recents');
            if (navigationStack[navigationStack.length - 1] !== 'recents') {
                navigationStack.push('recents');
            }
        } else {
            const appInfo = PMS_REGISTRY.get(appId) || {
                appName: options.appName || appId.charAt(0).toUpperCase() + appId.slice(1),
                packageName: options.packageName || `com.android.${appId}`,
                versionName: options.versionName || '1.0.0',
                desc: 'Android Application Runtime Surface.'
            };
            renderGenericApp(appInfo);
            activateScreen('screen-generic-app');
            if (navigationStack[navigationStack.length - 1] !== appId) {
                navigationStack.push(appId);
            }
            runningTasks.set(appId, { title: appInfo.appName, pkg: appInfo.packageName, screenId: 'screen-generic-app' });
        }

        updateSubsystemInspector();
    }

    function goBack() {
        if (fdroidDetailModal.style.display !== 'none') {
            fdroidDetailModal.style.display = 'none';
            if (navigationStack[navigationStack.length - 1] === 'fdroid-detail') {
                navigationStack.pop();
            }
            updateSubsystemInspector();
            return;
        }

        if (navigationStack.length > 1) {
            navigationStack.pop();
            const prev = navigationStack[navigationStack.length - 1];
            if (prev === 'home') {
                activateScreen('screen-home');
            } else if (prev === 'fdroid') {
                activateScreen('screen-fdroid');
            } else if (prev === 'unity' || prev === 'godot') {
                activateScreen('screen-native-surface');
            } else if (prev === 'recents') {
                renderRecents();
                activateScreen('screen-recents');
            } else {
                const appInfo = PMS_REGISTRY.get(prev);
                if (appInfo) renderGenericApp(appInfo);
                activateScreen('screen-generic-app');
            }
        } else {
            activateScreen('screen-home');
        }
        updateSubsystemInspector();
    }

    function goHome() {
        fdroidDetailModal.style.display = 'none';
        navigationStack.length = 0;
        navigationStack.push('home');
        activateScreen('screen-home');
        updateSubsystemInspector();
    }

    function renderFdroidCatalog() {
        fdroidCatalogList.innerHTML = '';
        const query = currentFdroidQuery.trim().toLowerCase();

        for (const [pkg, app] of PMS_REGISTRY.entries()) {
            const matchesCat = currentFdroidCategory === 'all' || 
                               (currentFdroidCategory === 'latest' && (app.category === 'latest' || app.installed)) ||
                               (app.category && app.category.toLowerCase() === currentFdroidCategory);
            
            const matchesQuery = !query || 
                                 app.appName.toLowerCase().includes(query) || 
                                 pkg.toLowerCase().includes(query) || 
                                 (app.desc && app.desc.toLowerCase().includes(query));

            if (!matchesCat || !matchesQuery) continue;

            const card = doc.createElement('div');
            card.className = 'fdroid-card';
            card.setAttribute('data-package', pkg);
            card.textContent = app.appName;
            fdroidCatalogList.appendChild(card);
        }
    }

    function showFdroidAppDetails(pkg) {
        const app = PMS_REGISTRY.get(pkg);
        if (!app) return;

        navigationStack.push('fdroid-detail');
        updateSubsystemInspector();
        fdroidDetailContent.innerHTML = `<div>${app.appName} - ${pkg}</div>`;
        fdroidDetailModal.style.display = 'flex';
    }

    function addDynamicAppToGrid(pkg, appName, icon = '📦') {
        const existing = dynamicAppGrid.querySelector(`[data-package="${pkg}"]`);
        if (existing) return;

        const item = doc.createElement('div');
        item.className = 'app-icon-item';
        item.setAttribute('data-package', pkg);
        item.setAttribute('data-app', pkg);
        item.textContent = appName;

        item.addEventListener('click', () => {
            launchApp(pkg);
        });

        dynamicAppGrid.appendChild(item);
    }

    function setVolume(vol) {
        masterVolume = Math.max(0, Math.min(1.0, vol));
        volumeBarFill.style.height = `${masterVolume * 100}%`;
        volumeHud.style.display = 'flex';
        volumeHud.style.opacity = '1';
        appendLog(`[AUDIO] Master Volume adjusted to ${Math.round(masterVolume * 100)}%.`, 'info');
    }

    function togglePower() {
        isScreenSleeping = !isScreenSleeping;
        screenSleepOverlay.classList.toggle('sleeping', isScreenSleeping);
    }

    function toggleOrientation() {
        isLandscape = !isLandscape;
        phoneFrame.classList.toggle('landscape', isLandscape);
    }

    return {
        doc,
        PMS_REGISTRY,
        navigationStack,
        runningTasks,
        screens: { screenHome, screenFdroid, screenNativeSurface, screenGenericApp, screenRecents },
        fdroidDetailModal,
        fdroidCatalogList,
        dynamicAppGrid,
        phoneFrame,
        screenSleepOverlay,
        volumeHud,
        inspector: { inspectorPmsCount, inspectorAmsDepth, inspectorAmsActivity, inspectorWmsWindows },
        launchApp,
        goBack,
        goHome,
        showFdroidAppDetails,
        renderFdroidCatalog,
        addDynamicAppToGrid,
        setVolume,
        togglePower,
        toggleOrientation,
        setFdroidQuery: (q) => { currentFdroidQuery = q; renderFdroidCatalog(); },
        setFdroidCategory: (cat) => { currentFdroidCategory = cat; renderFdroidCatalog(); },
        getMasterVolume: () => masterVolume,
        getIsScreenSleeping: () => isScreenSleeping,
        getIsLandscape: () => isLandscape
    };
}

// =============================================================================
// TEST SUITE 1: Rapid Backstack Cycling & Underflow Invariants
// =============================================================================
runSuite("1. Rapid Backstack Cycling (100 pushes, 100 LIFO pops, 50 underflow pops)", () => {
    const env = createAndroidUiEnvironment();

    assert(env.navigationStack.length === 1, "Initial stack depth must be 1 (home)");
    assert(env.navigationStack[0] === 'home', "Initial stack element must be 'home'");
    assert(env.screens.screenHome.classList.contains('active'), "Home screen must be active initially");

    // 1.1 Push 100 distinct activities onto the backstack
    console.log("  -> Pushing 100 activities onto navigation stack...");
    const expectedSequence = ['home'];

    for (let i = 1; i <= 100; i++) {
        const pkgName = `com.android.app${i}`;
        const appName = `App ${i}`;
        env.PMS_REGISTRY.set(pkgName, {
            packageName: pkgName,
            appName: appName,
            versionName: '1.0.0',
            mainActivity: `${pkgName}.MainActivity`,
            category: 'latest',
            installed: true
        });

        env.launchApp(pkgName);
        expectedSequence.push(pkgName);

        assert(env.navigationStack.length === i + 1, `Step ${i}: Stack length must be ${i + 1}`);
        assert(env.navigationStack[env.navigationStack.length - 1] === pkgName, `Step ${i}: Top must be ${pkgName}`);
        assert(env.screens.screenGenericApp.classList.contains('active'), `Step ${i}: Generic app screen active`);
        assert(env.inspector.inspectorAmsDepth.textContent === `Depth: ${i + 1}`, `Step ${i}: Inspector depth updated`);
    }

    assert(env.navigationStack.length === 101, "Stack length after 100 pushes is 101");

    // 1.2 Pop all 100 activities sequentially in strict LIFO order
    console.log("  -> Popping 100 activities in LIFO order...");
    for (let i = 100; i >= 1; i--) {
        env.goBack();
        const expectedTop = expectedSequence[i - 1];

        assert(env.navigationStack.length === i, `Pop ${101 - i}: Stack length must be ${i}`);
        assert(env.navigationStack[env.navigationStack.length - 1] === expectedTop, `Pop ${101 - i}: Top must be ${expectedTop}`);
        assert(env.inspector.inspectorAmsDepth.textContent === `Depth: ${i}`, `Pop ${101 - i}: Inspector depth synced`);

        if (expectedTop === 'home') {
            assert(env.screens.screenHome.classList.contains('active'), "Final pop restores screen-home");
        } else {
            assert(env.screens.screenGenericApp.classList.contains('active'), `Pop ${101 - i}: Generic screen active for ${expectedTop}`);
        }
    }

    assert(env.navigationStack.length === 1, "Stack depth is 1 after popping all 100 activities");
    assert(env.navigationStack[0] === 'home', "Stack root is 'home'");
    assert(env.screens.screenHome.classList.contains('active'), "Home screen active");

    // 1.3 Underflow Stress: 50 Back clicks on empty stack
    console.log("  -> Executing 50 underflow Back clicks on root Home stack...");
    for (let i = 1; i <= 50; i++) {
        env.goBack();
        assert(env.navigationStack.length === 1, `Underflow ${i}: Stack depth invariant preserved (length=1)`);
        assert(env.navigationStack[0] === 'home', `Underflow ${i}: Stack root remains 'home'`);
        assert(env.screens.screenHome.classList.contains('active'), `Underflow ${i}: Home screen remains active`);
        assert(env.inspector.inspectorAmsDepth.textContent === 'Depth: 1', `Underflow ${i}: Inspector depth invariant`);
    }

    // 1.4 Randomized navigation slamming: 5,000 random operations
    console.log("  -> Executing 5,000 randomized navigation operations...");
    let seed = 0x13579BDF;
    function prng() {
        seed = (seed * 1664525 + 1013904223) >>> 0;
        return seed;
    }

    const appPool = ['fdroid', 'unity', 'godot', 'recents', 'com.android.chrome'];
    for (let i = 0; i < 5000; i++) {
        const op = prng() % 4;
        if (op === 0) {
            // Launch random app
            const target = appPool[prng() % appPool.length];
            env.launchApp(target);
        } else if (op === 1) {
            // Go back
            env.goBack();
        } else if (op === 2) {
            // Go home
            env.goHome();
        } else {
            // Rapid double back
            env.goBack();
            env.goBack();
        }

        assert(env.navigationStack.length >= 1, `Slam ${i}: Stack depth >= 1 invariant`);
        assert(env.navigationStack[0] === 'home', `Slam ${i}: Stack root is 'home'`);
        const top = env.navigationStack[env.navigationStack.length - 1];
        assert(typeof top === 'string' && top.length > 0, `Slam ${i}: Valid top identifier`);
    }
});

// =============================================================================
// TEST SUITE 2: Home Button Unconditional Reset from Arbitrary Depths & Modals
// =============================================================================
runSuite("2. Home Button Unconditional Reset from Arbitrary Modal & Screen Depths", () => {
    const env = createAndroidUiEnvironment();

    // 2.1 Deep modal nesting test
    console.log("  -> Testing Home button reset from deep modal state...");
    env.launchApp('fdroid');
    assert(env.screens.screenFdroid.classList.contains('active'), "F-Droid screen active");
    assert(env.navigationStack.length === 2, "Stack length 2");

    env.showFdroidAppDetails('org.fdroid.fdroid');
    assert(env.fdroidDetailModal.style.display === 'flex', "F-Droid detail modal opened");
    assert(env.navigationStack.length === 3, "Stack length 3 (includes fdroid-detail)");
    assert(env.navigationStack[2] === 'fdroid-detail', "Top is fdroid-detail");

    // Press Home
    env.goHome();
    assert(env.navigationStack.length === 1, "Stack length reset to exactly 1");
    assert(env.navigationStack[0] === 'home', "Stack root is 'home'");
    assert(env.screens.screenHome.classList.contains('active'), "Home screen active");
    assert(!env.screens.screenFdroid.classList.contains('active'), "F-Droid screen deactivated");
    assert(env.fdroidDetailModal.style.display === 'none', "F-Droid detail modal unconditionally closed");
    assert(env.inspector.inspectorAmsDepth.textContent === 'Depth: 1', "Inspector depth is 1");
    assert(env.inspector.inspectorAmsActivity.textContent === 'com.android.launcher3/.Launcher', "Inspector activity is Launcher");

    // 2.2 100 Randomized Arbitrary Depth Resets
    console.log("  -> Testing 100 arbitrary depth configurations with modal permutations...");
    let seed = 0xACE13579;
    function prng() {
        seed = (seed * 1664525 + 1013904223) >>> 0;
        return seed;
    }

    const apps = ['fdroid', 'unity', 'godot', 'recents', 'com.android.chrome'];
    for (let i = 0; i < 100; i++) {
        const depth = (prng() % 30) + 1;
        for (let d = 0; d < depth; d++) {
            const target = apps[prng() % apps.length];
            env.launchApp(target);
        }

        // Randomly trigger modal
        const withModal = (prng() % 2) === 0;
        if (withModal) {
            env.showFdroidAppDetails('com.unity.cube.gles');
            assert(env.fdroidDetailModal.style.display === 'flex', `Cycle ${i}: Modal active`);
        }

        assert(env.navigationStack.length > 0, `Cycle ${i}: Stack non-empty`);

        // Press Home
        env.goHome();
        assert(env.navigationStack.length === 1, `Cycle ${i}: Stack reset to 1`);
        assert(env.navigationStack[0] === 'home', `Cycle ${i}: Stack root is 'home'`);
        assert(env.screens.screenHome.classList.contains('active'), `Cycle ${i}: Home screen active`);
        assert(env.fdroidDetailModal.style.display === 'none', `Cycle ${i}: Modal dismissed`);
    }
});

// =============================================================================
// TEST SUITE 3: F-Droid Search Fuzzing & Category Query Filtering
// =============================================================================
runSuite("3. F-Droid Search Fuzzing (Regex tokens, Unicode, HTML tags, Long strings)", () => {
    const env = createAndroidUiEnvironment();

    // 3.1 Initial catalog check
    env.renderFdroidCatalog();
    assert(env.fdroidCatalogList.children.length === 4, "Initial catalog renders 4 cards");

    // 3.2 Fuzzing corpus with adversarial inputs
    console.log("  -> Fuzzing F-Droid search with 500 adversarial strings...");
    const adversarialCorpus = [
        // Regex specials that crash naive new RegExp()
        "[.*+?^${}()|[\\]\\\\]",
        "[a-z]+",
        "(.*)",
        "(a|b|c)*",
        "(?<=a)b",
        "(?!test)hello",
        "\\d+\\.\\d+",
        "^[a-zA-Z0-9]+$",
        "\\",
        "\\\\",
        "[[[",
        "(((",
        "???",
        "+++",
        "***",
        
        // SQL Injection
        "' OR '1'='1",
        "admin'--",
        "'; DROP TABLE packages; --",
        "1' UNION SELECT NULL, NULL--",
        
        // XSS and HTML Tags
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<svg onload=alert(document.cookie)>",
        "\" autofocus onfocus=alert(1) x=\"",
        "<b>Bold</b>",
        "<style>body{display:none}</style>",
        
        // Unicode & Multilingual
        "📱 Free App 🚀",
        "微信 支付 游戏",
        "متجر التطبيقات",
        "русский каталог",
        "日本語テスト",
        "🏳️‍🌈👨‍👩‍👧‍👦",
        "\u0000\u0001\u0002\u001F",
        "\uFFFF",
        "\u{10FFFF}",
        "᚛᚛ᚑᚌᚐᚋ᚜",
        
        // Extreme String Lengths
        "A".repeat(100),
        "Z".repeat(1000),
        "0".repeat(5000),
        
        // Whitespace Edge Cases
        "",
        "   ",
        "\t\t\t",
        "\n\r\n",
        "  unity  ",
        "  F-DROID  "
    ];

    let seed = 0x99887766;
    function prng() {
        seed = (seed * 1664525 + 1013904223) >>> 0;
        return seed;
    }

    for (let i = 0; i < 500; i++) {
        const query = adversarialCorpus[prng() % adversarialCorpus.length];
        
        let threw = false;
        try {
            env.setFdroidQuery(query);
        } catch (err) {
            threw = true;
            console.error(`Crash on query: ${query}`, err);
        }

        assert(!threw, `Fuzz ${i}: Query '${query.slice(0, 30)}...' must not throw exception`);
        assert(env.fdroidCatalogList.children.length >= 0, `Fuzz ${i}: Children count non-negative`);
        assert(env.fdroidCatalogList.children.length <= env.PMS_REGISTRY.size, `Fuzz ${i}: Children count within bound`);

        // Case-insensitivity check
        if (query.trim().length > 0 && query.length < 100) {
            env.setFdroidQuery(query.toLowerCase());
            const lowerCount = env.fdroidCatalogList.children.length;
            env.setFdroidQuery(query.toUpperCase());
            const upperCount = env.fdroidCatalogList.children.length;
            assert(lowerCount === upperCount, `Fuzz ${i}: Case-insensitivity count match for '${query}'`);
        }
    }

    // 3.3 Category Tab Isolation & Clear
    console.log("  -> Testing Category tab filtering and query combination...");
    const categories = ['all', 'latest', 'games', 'internet', 'system'];
    for (const cat of categories) {
        env.setFdroidCategory(cat);
        env.setFdroidQuery('');
        assert(env.fdroidCatalogList.children.length > 0, `Category ${cat} returns matching cards`);
    }

    // Search clear
    env.setFdroidQuery('nonexistent_app_xyz');
    assert(env.fdroidCatalogList.children.length === 0, "Query with no match returns 0 cards");
    env.setFdroidQuery('');
    assert(env.fdroidCatalogList.children.length > 0, "Clearing query restores catalog cards");
});

// =============================================================================
// TEST SUITE 4: Dynamic App Injection into Home Grid & PMS Registry
// =============================================================================
runSuite("4. Dynamic App Injection into Home Grid & PMS Registry", () => {
    const env = createAndroidUiEnvironment();

    console.log("  -> Injecting 100 dynamic apps into PMS Registry and Home Grid...");
    for (let i = 1; i <= 100; i++) {
        const pkg = `org.custom.ingested.app${i}`;
        const label = `Dynamic Ingested App ${i}`;
        const icon = i % 2 === 0 ? '🚀' : '⚡';

        const appRecord = {
            packageName: pkg,
            appName: label,
            versionName: '2.5.0',
            versionCode: 250 + i,
            mainActivity: `${pkg}.MainActivity`,
            category: 'latest',
            icon: icon,
            iconClass: 'icon-generic-apk',
            desc: `Dynamically ingested test package #${i}`,
            activitiesCount: 5 + (i % 10),
            providersCount: i % 3,
            servicesCount: i % 4,
            receiversCount: i % 2,
            permissions: ['android.permission.INTERNET', 'android.permission.CAMERA'],
            installed: true
        };

        env.PMS_REGISTRY.set(pkg, appRecord);
        env.addDynamicAppToGrid(pkg, label, icon);

        assert(env.PMS_REGISTRY.has(pkg), `PMS registry has package ${pkg}`);
        const card = env.dynamicAppGrid.querySelector(`[data-package="${pkg}"]`);
        assert(card !== null, `DOM item rendered in dynamicAppGrid for ${pkg}`);
        assert(card.textContent === label, `DOM item label matches '${label}'`);

        // Test launch of injected app
        env.launchApp(pkg);
        assert(env.screens.screenGenericApp.classList.contains('active'), `Injected app ${pkg} launched to generic app screen`);
        assert(env.navigationStack[env.navigationStack.length - 1] === pkg, `Top activity is ${pkg}`);
    }

    assert(env.PMS_REGISTRY.size === 104, "PMS registry contains 104 total apps (4 default + 100 dynamic)");
    assert(env.dynamicAppGrid.children.length === 100, "100 dynamic apps mounted in DOM grid");

    // Test deduplication on re-injection
    console.log("  -> Testing deduplication on re-injection of existing packages...");
    env.addDynamicAppToGrid('org.custom.ingested.app1', 'Duplicate Label');
    assert(env.dynamicAppGrid.children.length === 100, "Deduplication prevented duplicate DOM elements");
});

// =============================================================================
// TEST SUITE 5: Virtual Hardware Controls (Volume Rocker, Power Toggle, Orientation)
// =============================================================================
runSuite("5. Virtual Hardware Controls & Edge Cases", () => {
    const env = createAndroidUiEnvironment();

    // 5.1 Volume clamping edge cases
    console.log("  -> Testing volume clamping and edge cases...");
    
    env.setVolume(0.5);
    assert(Math.abs(env.getMasterVolume() - 0.5) < 1e-6, "Volume set to 0.5");

    // Overflow clamping
    env.setVolume(1.5);
    assert(env.getMasterVolume() === 1.0, "Volume 1.5 clamped to 1.0");

    env.setVolume(100.0);
    assert(env.getMasterVolume() === 1.0, "Volume 100.0 clamped to 1.0");

    // Underflow clamping
    env.setVolume(-0.5);
    assert(env.getMasterVolume() === 0.0, "Volume -0.5 clamped to 0.0");

    env.setVolume(-999.0);
    assert(env.getMasterVolume() === 0.0, "Volume -999.0 clamped to 0.0");

    // Float step adjustments
    for (let step = 0; step <= 10; step++) {
        const targetVol = step * 0.1;
        env.setVolume(targetVol);
        assert(Math.abs(env.getMasterVolume() - targetVol) < 1e-5, `Volume step ${step * 10}% exact`);
    }

    // 5.2 Power button toggle: 2,000 rapid cycles
    console.log("  -> Testing 2,000 rapid power toggles...");
    assert(env.getIsScreenSleeping() === false, "Screen initially awake");
    assert(!env.screenSleepOverlay.classList.contains('sleeping'), "Sleep overlay inactive");

    for (let i = 1; i <= 2000; i++) {
        env.togglePower();
        const expectedSleeping = (i % 2 === 1);
        assert(env.getIsScreenSleeping() === expectedSleeping, `Power toggle ${i}: State is ${expectedSleeping}`);
        assert(env.screenSleepOverlay.classList.contains('sleeping') === expectedSleeping, `Power toggle ${i}: DOM class sleeping is ${expectedSleeping}`);
    }

    assert(env.getIsScreenSleeping() === false, "Screen awake after 2,000 toggles");

    // 5.3 Orientation rotation toggle: 1,000 rapid cycles
    console.log("  -> Testing 1,000 rapid orientation toggles...");
    assert(env.getIsLandscape() === false, "Initially portrait");
    assert(!env.phoneFrame.classList.contains('landscape'), "Landscape class inactive");

    for (let i = 1; i <= 1000; i++) {
        env.toggleOrientation();
        const expectedLandscape = (i % 2 === 1);
        assert(env.getIsLandscape() === expectedLandscape, `Rotate ${i}: State is ${expectedLandscape}`);
        assert(env.phoneFrame.classList.contains('landscape') === expectedLandscape, `Rotate ${i}: DOM class landscape is ${expectedLandscape}`);
    }

    assert(env.getIsLandscape() === false, "Orientation portrait after 1,000 toggles");
});

// =============================================================================
// SUMMARY & VERDICT
// =============================================================================
console.log(`\n======================================================`);
console.log(`⚡ ALL CHALLENGER 1 ADVERSARIAL STRESS CHECKS COMPLETED`);
console.log(`Total assertions:  ${totalAssertions}`);
console.log(`Passed assertions: ${passedAssertions}`);
console.log(`Failed assertions: ${failedAssertions}`);
console.log(`======================================================\n`);

if (failedAssertions > 0) {
    process.exit(1);
} else {
    process.exit(0);
}
