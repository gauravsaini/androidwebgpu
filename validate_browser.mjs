import puppeteer from 'puppeteer-core';
import fs from 'fs';

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL = 'http://localhost:8001/android.html';

console.log('[validate] launching chrome', CHROME);
const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: true,
  args: ['--no-sandbox','--disable-setuid-sandbox','--disable-gpu','--disable-dev-shm-usage','--window-size=1280,900','--enable-features=SharedArrayBuffer'],
});
const page = await browser.newPage();
await page.setViewport({width:1280, height:900});

// capture console and errors
const logs = [];
page.on('console', m => { const t=m.text(); logs.push(t); console.log('[page console]', m.type(), t.slice(0,300)); });
page.on('pageerror', e => console.log('[pageerror]', e.message));
page.on('requestfailed', r => console.log('[requestfailed]', r.url(), r.failure()?.errorText));

console.log('[validate] goto', URL);
await page.goto(URL, { waitUntil: 'domcontentloaded', timeout: 30000 });
await new Promise(r=>setTimeout(r,3000));

// check static DOM
const title = await page.title();
console.log('[validate] title', title);

const hasV86Script = await page.evaluate(()=> !!document.querySelector('script[src*="libv86"]'));
console.log('[validate] has libv86 script', hasV86Script);

const hasCanvas = await page.evaluate(()=> !!document.getElementById('screen'));
console.log('[validate] has canvas screen', hasCanvas);

const hasPhoneBezel = await page.evaluate(()=> !!document.getElementById('phone-bezel'));
console.log('[validate] has phone bezel', hasPhoneBezel);

// Check module availability - try import V86GuestManager
const modCheck = await page.evaluate(async () => {
  const out={};
  try{
    const {verifyBzImage, V86GuestManager} = await import('./src/v86_guest_manager.js');
    out.hasV86GuestManager = typeof V86GuestManager === 'function';
    out.hasVerify = typeof verifyBzImage === 'function';
    // try verify via fetch header bytes
    try{
      const res = await fetch('./guest/build/bzImage', {headers:{Range:'bytes=0-2048'}});
      const buf = new Uint8Array(await res.arrayBuffer());
      // manual HdrS check
      const hdr = String.fromCharCode(...buf.slice(0x202,0x206));
      const aa55 = buf[0x1FE] | (buf[0x1FF]<<8);
      out.bzImageHdr = hdr;
      out.bzImageAA55 = aa55.toString(16);
      out.bzImageSize = buf.length;
      out.fetchStatus = res.status;
      // try real verify if available
      if(out.hasVerify){
        // fetch more bytes
        const res2 = await fetch('./guest/build/bzImage');
        const hdr2 = await res2.arrayBuffer();
        // can't call verifyBzImage with buffer? it expects path - just manual
      }
    }catch(e){ out.fetchError = e.message; }
    out.location = location.href;
    out.crossOriginIsolated = self.crossOriginIsolated;
  }catch(e){ out.error = e.message + ' ' + e.stack?.slice(0,500); }
  return out;
});
console.log('[validate] modCheck', JSON.stringify(modCheck,null,2));

// Check VirtioGpuDevice slot
const virtioCheck = await page.evaluate(async ()=>{
  try{
    const m = await import('./src/virtio_gpu_device.js');
    const v = new m.VirtioGpuDevice(null,null,null);
    return {slot: v.pciSlot, irq: v.irqLine, bar0: v.ioBase, bar1: v.bar1Value, name: v.name, queues: v.queues.length};
  }catch(e){ return {error:e.message};}
});
console.log('[validate] virtioCheck', JSON.stringify(virtioCheck));

// Check SyntheticGuestProbe
const synthCheck = await page.evaluate(async ()=>{
  try{
    const {SyntheticGuestProbe} = await import('./src/synthetic_guest_probe.js');
    const p = new SyntheticGuestProbe(null);
    // dummy run? check methods exist
    return {hasRun: typeof p.run === 'function' || typeof p.runAll === 'function', hasExecute: typeof p.execute === 'function', keys: Object.getOwnPropertyNames(Object.getPrototypeOf(p)).slice(0,20)};
  }catch(e){ return {error:e.message + ' ' + e.stack?.slice(0,800)};}
});
console.log('[validate] synthCheck', JSON.stringify(synthCheck));

// Try to instantiate V86GuestManager and check config
const guestMgrCheck = await page.evaluate(async ()=>{
  try{
    const {V86GuestManager} = await import('./src/v86_guest_manager.js');
    const gm = new V86GuestManager({autostart:false});
    return {state: gm.state, kernelUrl: gm.config.kernelUrl, initrdUrl: gm.config.initrdUrl, cmdline: gm.config.cmdline.slice(0,200), hasStart: typeof gm.start==='function'};
  }catch(e){ return {error:e.message};}
});
console.log('[validate] guestMgrCheck', JSON.stringify(guestMgrCheck));

// Check fetch of bzImage via HTTP range support (critical for v86)
const rangeCheck = await page.evaluate(async ()=>{
  try{
    const r1 = await fetch('./guest/build/bzImage', {headers:{Range:'bytes=0-1023'}});
    const r2 = await fetch('./guest/build/bzImage');
    const buf = new Uint8Array(await r1.arrayBuffer());
    return {rangeStatus: r1.status, rangeHeader: r1.headers.get('Content-Range'), totalStatus: r2.status, acceptRanges: r2.headers.get('Accept-Ranges'), cc: r1.headers.get('Cross-Origin-Embedder-Policy'), first4: Array.from(buf.slice(0,4))};
  }catch(e){return {error:e.message}}
});
console.log('[validate] rangeCheck', JSON.stringify(rangeCheck));

// Check COOP/COEP headers
const headerCheck = await page.evaluate(async ()=>{
  return {
    crossOriginIsolated, 
    sab: typeof SharedArrayBuffer !== 'undefined',
    wasm: typeof WebAssembly !== 'undefined',
    webgpu: !!navigator.gpu,
    userAgent: navigator.userAgent.slice(0,120)
  };
});
console.log('[validate] headerCheck', JSON.stringify(headerCheck));

// Try synthetic probe end-to-end inside page with mock VirtioGpuDevice
const synthRun = await page.evaluate(async ()=>{
  try{
    const {VirtioGpuDevice}= await import('./src/virtio_gpu_device.js');
    const {SyntheticGuestProbe}= await import('./src/synthetic_guest_probe.js');
    // create mock v86 and bridge
    const mockBridge = { onCommand:()=>{}, handleDisplayInfo:()=>({width:1280,height:720}) };
    const mockV86 = { io: { register_devices:()=>{} }, bus: { register:()=>{} } };
    const canvas = document.createElement('canvas');
    const dev = new VirtioGpuDevice(mockV86, mockBridge, canvas);
    const probe = new SyntheticGuestProbe(dev);
    // probe has different API, try available methods
    let result=null;
    if(typeof probe.runAll === 'function') result = await probe.runAll();
    else if(typeof probe.run === 'function') result = await probe.run();
    else if(typeof probe.execute === 'function') result = await probe.execute();
    else result = {error:'no run method', proto:Object.getOwnPropertyNames(Object.getPrototypeOf(probe))};
    return {result: JSON.stringify(result).slice(0,3000), hasDev:true, devSlot: dev.pciSlot};
  }catch(e){ return {error:e.message + ' ' + e.stack?.slice(0,1000)};}
});
console.log('[validate] synthRun', JSON.stringify(synthRun,null,2));

// screenshot
await page.screenshot({path:'/tmp/validate_screenshot.png', fullPage:true});
console.log('[validate] screenshot saved to /tmp/validate_screenshot.png, exists?', fs.existsSync('/tmp/validate_screenshot.png'));

await browser.close();
console.log('[validate] DONE');
