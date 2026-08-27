import puppeteer from 'puppeteer-core';
const CHROME='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL='http://localhost:8080/android.html';
const b=await puppeteer.launch({executablePath:CHROME, headless:true, args:['--no-sandbox','--disable-setuid-sandbox']});
const p=await b.newPage();
const logs=[];
p.on('console',m=>{ const t=m.text(); if(t.includes('synthetic')||t.includes('2.2')||t.includes('2.3')||t.includes('2.4')||t.includes('2.5')) logs.push(t.slice(0,400)); });
p.on('pageerror',e=>console.log('pageerror',e.message.slice(0,500)));
await p.goto(URL,{waitUntil:'domcontentloaded', timeout:20000});
await new Promise(r=>setTimeout(r,2500));
const res=await p.evaluate(async()=>{
  const {VirtioGpuDevice}=await import('./src/virtio_gpu_device.js');
  const {SyntheticGuestProbe}=await import('./src/synthetic_guest_probe.js');
  // replicate test_synthetic_virtqueue_proof setup: fakeV86 with shared buffer
  const guestMem=new Uint8Array(8*1024*1024);
  const fakeV86={ cpu:{ memory:{buffer:guestMem.buffer}, device_raise_irq:()=>{}, devices:{pci:{devices:{}, register_device:()=>{}} }}, io:{register_read:()=>{}, register_write:()=>{}} };
  const dev=new VirtioGpuDevice(fakeV86, null, {width:1280, height:720});
  // ensure dev references same buffer
  dev.v86=fakeV86;
  const probe=new SyntheticGuestProbe({device:dev, guestMemory:guestMem});
  probe.device=dev;
  probe.guestMemory=guestMem;
  probe.guestMemView=new DataView(guestMem.buffer);
  probe.pfn0=0x10; probe.pfn1=0x11;
  probe.descTableAddr=probe.pfn0*4096;
  probe.availRingAddr=probe.descTableAddr+256*16;
  probe.usedRingAddr=Math.ceil((probe.availRingAddr+4+2*256)/4096)*4096;
  const full=await probe.runFullProof();
  return {full, gates:probe.gates, distinct:[...probe.distinctOpcodes], lastCreate:probe.lastCreate2d, logs:probe.logs.slice(-8).map(l=>l.line)};
});
console.log(JSON.stringify(res,null,2));
console.log('captured logs',logs.slice(-5).join('\n'));
await b.close();
