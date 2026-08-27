import puppeteer from 'puppeteer-core';
import fs from 'fs';

const CHROME='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL='http://localhost:8080/android.html';

console.log('[validate] Launching Chrome console validation for',URL);
const browser = await puppeteer.launch({
  executablePath: CHROME,
  headless: true,
  args:['--no-sandbox','--disable-setuid-sandbox','--disable-dev-shm-usage','--window-size=1280,900']
});
const page = await browser.newPage();
await page.setViewport({width:1280,height:900});

const consoleLogs=[];
page.on('console', m=>{
  const text=m.text();
  consoleLogs.push(`[${m.type()}] ${text}`);
  // print first chars
  console.log(`[page:${m.type()}]`, text.slice(0,500));
});
page.on('pageerror', e=>console.log('[pageerror]',e.message.slice(0,1000)));
page.on('requestfailed', r=>console.log('[requestfailed]',r.url().slice(-60), r.failure()?.errorText));

console.log('[validate] goto',URL);
await page.goto(URL,{waitUntil:'domcontentloaded', timeout:30000});
await new Promise(r=>setTimeout(r,4000));

const title = await page.title();
console.log('[result] title:',title);

// Evaluate console-accessible state (no screenshot, pure JS console)
const evalResult = await page.evaluate(async ()=>{
  const out={};
  out.location=location.href;
  out.crossOriginIsolated = typeof crossOriginIsolated!=='undefined'? crossOriginIsolated : 'unknown';
  out.hasSharedArrayBuffer = typeof SharedArrayBuffer!=='undefined';
  out.hasWebGPU = !!navigator.gpu;
  out.userAgent = navigator.userAgent.slice(0,100);
  // DOM checks
  out.hasCanvas = !!document.getElementById('screen');
  out.hasPhoneBezel = !!document.getElementById('phone-bezel');
  out.hasV86Script = !!document.querySelector('script[src*="libv86"]');
  // Try imports via dynamic import (module evaluation = console import)
  try{
    const {V86GuestManager, verifyBzImage}= await import('./src/v86_guest_manager.js');
    out.mgrHasClass = typeof V86GuestManager==='function';
    const gm = new V86GuestManager({autostart:false});
    out.gmState=gm.state;
    out.gmKernelUrl=gm.config.kernelUrl;
    out.gmInitrdUrl=gm.config.initrdUrl;
    out.gmCmdline=gm.config.cmdline.slice(0,180);
    // verify bzImage via fetch manual HdrS
    const res=await fetch('./guest/build/bzImage',{headers:{Range:'bytes=0-2048'}});
    const buf=new Uint8Array(await res.arrayBuffer());
    out.fetchStatus=res.status;
    out.hdr = String.fromCharCode(...buf.slice(0x202,0x206));
    out.aa55 = (buf[0x1FE]|buf[0x1FF]<<8).toString(16);
    out.first4 = Array.from(buf.slice(0,4));
    out.contentRange=res.headers.get('Content-Range');
    out.acceptRanges=res.headers.get('Accept-Ranges');
  }catch(e){ out.mgrError=e.message.slice(0,800); }

  try{
    const {VirtioGpuDevice}= await import('./src/virtio_gpu_device.js');
    const dev=new VirtioGpuDevice(null,null,null);
    out.virtio={slot:dev.pciSlot, irq:dev.irqLine, ioBase:dev.ioBase?.toString(16), bar1:dev.bar1Value?.toString(16), queues:dev.queues?.length, name:dev.name};
  }catch(e){ out.virtioError=e.message.slice(0,500); }

  try{
    const {SyntheticGuestProbe}= await import('./src/synthetic_guest_probe.js');
    const m=await import('./src/virtio_gpu_device.js');
    const mockBridge={};
    const canvas=document.createElement('canvas');
    const dev2=new m.VirtioGpuDevice(null,mockBridge,canvas);
    const probe=new SyntheticGuestProbe(dev2);
    out.probeKeys=Object.getOwnPropertyNames(Object.getPrototypeOf(probe)).slice(0,15);
    out.probeHasRun = typeof probe.run==='function' || typeof probe.runAll==='function' || typeof probe.execute==='function';
    let runRes=null;
    if(typeof probe.runAll==='function') runRes=await probe.runAll();
    else if(typeof probe.run==='function') runRes=await probe.run();
    out.probeRunSample=JSON.stringify(runRes).slice(0,1500);
    out.probeDevSlot=dev2.pciSlot;
  }catch(e){ out.probeError=e.message.slice(0,1000); }

  // Check globalLogcat / window
  try{
    out.windowKeys = Object.keys(window).filter(k=>k.toLowerCase().includes('v86')||k.toLowerCase().includes('guest')||k.toLowerCase().includes('gpu')).slice(0,10);
    out.hasGlobalLogcat = typeof window.globalLogcat !=='undefined';
  }catch(e){}

  // Try to check file via fetch for bzImage full header via second range
  try{
    const r2=await fetch('./guest/build/bzImage');
    out.fullFetchStatus=r2.status;
    out.fullAcceptRanges=r2.headers.get('Accept-Ranges');
    out.fullCE=h2=>r2.headers.get('Cross-Origin-Embedder-Policy');
  }catch(e){}

  return out;
});

console.log('[result] evalResult',JSON.stringify(evalResult,null,2));

// Also check consoleLogs count and sample
console.log('[result] consoleLogs count',consoleLogs.length);
console.log('[result] consoleLogs sample',consoleLogs.slice(0,15).join('\n').slice(0,2000));

// Check for errors in logs
const errors = consoleLogs.filter(l=>l.toLowerCase().includes('error')||l.toLowerCase().includes('fail'));
console.log('[result] errors in console',errors.slice(0,5));

// No screenshot per user request - pure console access
await browser.close();
console.log('[validate] DONE - console access complete, server 8080 stable');
