import puppeteer from 'puppeteer-core';
import fs from 'fs';
const CHROME='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL='http://localhost:8080/android.html';
console.log('[retry] launching',URL);
const browser=await puppeteer.launch({executablePath:CHROME, headless:true, args:['--no-sandbox','--disable-setuid-sandbox','--disable-dev-shm-usage','--window-size=1280,900']});
const page=await browser.newPage();
await page.setViewport({width:1280,height:900});
const consoleLogs=[];
page.on('console',m=>{
  const t=m.text();
  consoleLogs.push(`[${m.type()}] ${t}`);
  if(t.includes('virtio')||t.includes('V86')||t.includes('synthetic')||t.includes('BIOS')||t.includes('panic')||t.includes('ERROR')||t.includes('bridge')) console.log(`[page:${m.type()}]`,t.slice(0,600));
});
page.on('pageerror',e=>console.log('[pageerror]',e.message.slice(0,800)));
page.on('requestfailed',r=>console.log('[reqfail]',r.url().slice(-50),r.failure()?.errorText));
await page.goto(URL,{waitUntil:'domcontentloaded', timeout:30000});
console.log('[retry] waiting 10s for v86 boot...');
await new Promise(r=>setTimeout(r,10000));

const title=await page.title();
console.log('[retry] title',title);

const evalRes=await page.evaluate(async()=>{
  const out={};
  out.location=location.href;
  out.crossOriginIsolated=self.crossOriginIsolated;
  out.hasSAB=typeof SharedArrayBuffer!=='undefined';
  out.hasWebGPU=!!navigator.gpu;
  out.hasCanvas=!!document.getElementById('screen');
  out.hasV86Script=!!document.querySelector('script[src*="libv86"]');
  // manager
  try{
    const {V86GuestManager}=await import('./src/v86_guest_manager.js');
    const gm=new V86GuestManager({autostart:false});
    out.gm={state:gm.state, kernel:gm.config.kernelUrl, initrd:gm.config.initrdUrl, cmdline:gm.config.cmdline.slice(0,150)};
    const r=await fetch('./guest/build/bzImage',{headers:{Range:'bytes=0-2048'}});
    const b=new Uint8Array(await r.arrayBuffer());
    out.bz={status:r.status, hdr:String.fromCharCode(...b.slice(0x202,0x206)), aa55:(b[0x1FE]|b[0x1FF]<<8).toString(16), cr:r.headers.get('Content-Range'), ar:r.headers.get('Accept-Ranges')};
  }catch(e){out.gmErr=e.message.slice(0,600)}
  try{
    const {VirtioGpuDevice}=await import('./src/virtio_gpu_device.js');
    const d=new VirtioGpuDevice(null,{},document.createElement('canvas'));
    out.virtio={slot:d.pciSlot, irq:d.irqLine, ioBase:d.ioBase, bar1:d.bar1Value, name:d.name};
  }catch(e){out.virtioErr=e.message.slice(0,500)}
  try{
    const {VirtioGpuDevice}=await import('./src/virtio_gpu_device.js');
    const {SyntheticGuestProbe}=await import('./src/synthetic_guest_probe.js');
    const dev=new VirtioGpuDevice(null,{},document.createElement('canvas'));
    const probe=new SyntheticGuestProbe(dev);
    const res=await probe.runFullProof();
    out.probe={res, keys:Object.getOwnPropertyNames(Object.getPrototypeOf(probe)).slice(0,10)};
    // try report if exists
    if(probe.report) out.probeReport=probe.report();
    if(probe.gates) out.probeGates=probe.gates;
  }catch(e){out.probeErr=e.message.slice(0,1000); out.probeStack=e.stack?.slice(0,800)}
  // check globals
  try{
    out.windowKeys=Object.keys(window).filter(k=>k.toLowerCase().includes('v86')||k.toLowerCase().includes('gpu')||k.toLowerCase().includes('log')).slice(0,15);
    out.hasV86=!!window.V86;
    out.hasV86Starter=!!window.V86Starter;
    out.hasEmu=!!window.v86emulator || !!window.emulator;
    // try to find guestManager via any global
    if(window.v86emulator) out.emuKeys=Object.keys(window.v86emulator).slice(0,10);
    // globalLogcat
    if(window.globalLogcat) out.logcatLen=window.globalLogcat.entries?.length || window.globalLogcat.length || 'unknown';
  }catch(e){}
  // serial logs via any exposed manager
  try{
    // try to locate V86GuestManager instance from DOM/bootstrap
    // search for any element with __v86
    out.serialSnippet='not found - check bootstrap';
    // attempt to fetch serial via evaluating SystemBootstrap if exported
    const {globalLogcat}=await import('./src/logger.js');
    out.globalLogcatCount=globalLogcat.entries.length;
    out.lastLogs=globalLogcat.entries.slice(-10).map(e=>`[${e.tag}] ${e.message}`.slice(0,200));
  }catch(e){out.loggerErr=e.message.slice(0,500)}
  return out;
});
console.log('[retry] eval',JSON.stringify(evalRes,null,2));
console.log('[retry] console total',consoleLogs.length);
const filtered=consoleLogs.filter(l=>l.toLowerCase().includes('panic')||l.toLowerCase().includes('error')||l.toLowerCase().includes('synthetic')||l.toLowerCase().includes('virtio')||l.toLowerCase().includes('status')||l.toLowerCase().includes('gate'));
console.log('[retry] filtered logs\n',filtered.slice(0,30).join('\n'));
const errors=consoleLogs.filter(l=>l.includes('[E]')||l.includes('panic'));
console.log('[retry] ERRORS',errors.slice(0,10).join('\n'));
await browser.close();
console.log('[retry] DONE');
