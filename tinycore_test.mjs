import puppeteer from 'puppeteer-core';
const CHROME='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL='http://localhost:8080/android.html';
console.log('[tinycore] testing 6.6.8-tinycore 5.18M');
const b=await puppeteer.launch({executablePath:CHROME, headless:true, args:['--no-sandbox','--disable-setuid-sandbox']});
const p=await b.newPage();
const logs=[];
p.on('console',m=>{
  const t=m.text();
  logs.push(t);
  if(t.includes('panic')||t.includes('GUEST-PANIC')||t.includes('BIOS')||t.includes('Unimplemented')||t.includes('Kernel')||t.includes('virtio')||t.includes('synthetic')||t.includes('ERROR')) console.log('[page]',t.slice(0,600));
});
p.on('pageerror',e=>console.log('pageerror',e.message.slice(0,600)));
await p.goto(URL,{waitUntil:'domcontentloaded', timeout:25000});
await new Promise(r=>setTimeout(r,8000));
const res=await p.evaluate(async()=>{
  const out={};
  out.location=location.href;
  try{
    const r=await fetch('./guest/build/bzImage',{headers:{Range:'bytes=0-2048'}});
    const b=new Uint8Array(await r.arrayBuffer());
    out.bz={status:r.status, size:5437312, hdr:String.fromCharCode(...b.slice(0x202,0x206)), aa55:(b[0x1FE]|b[0x1FF]<<8).toString(16), type:r.headers.get('Content-Type')};
    // try to get kernel version via fetch and parsing? just show first bytes
  }catch(e){out.bzErr=e.message}
  try{
    const {VirtioGpuDevice}=await import('./src/virtio_gpu_device.js');
    const {SyntheticGuestProbe}=await import('./src/synthetic_guest_probe.js');
    const gm=new Uint8Array(8*1024*1024);
    const fakeV86={cpu:{memory:{buffer:gm.buffer},device_raise_irq:()=>{},devices:{pci:{devices:{},register_device:()=>{}}}},io:{register_read:()=>{}, register_write:()=>{}}};
    const dev=new VirtioGpuDevice(fakeV86,null,{width:1280,height:720}); dev.v86=fakeV86;
    const probe=new SyntheticGuestProbe({device:dev,guestMemory:gm});
    probe.device=dev; probe.guestMemory=gm; probe.guestMemView=new DataView(gm.buffer);
    probe.pfn0=0x10; probe.pfn1=0x11; probe.descTableAddr=0x10*4096; probe.availRingAddr=probe.descTableAddr+256*16; probe.usedRingAddr=Math.ceil((probe.availRingAddr+4+2*256)/4096)*4096;
    const full=await probe.runFullProof();
    out.probe={allPass:full.allPass, gates:full.gates, fps:full.metrics.fps, opcodes:full.distinctOpcodes.length};
  }catch(e){out.probeErr=e.message.slice(0,800)}
  // check global logcat for panic
  try{
    const {globalLogcat}=await import('./src/logger.js');
    out.logcatCount=globalLogcat.entries.length;
    out.lastLogs=globalLogcat.entries.slice(-8).map(e=>`[${e.tag}:${e.priority}] ${e.message}`.slice(0,200));
  }catch(e){}
  return out;
});
console.log(JSON.stringify(res,null,2));
console.log('filtered logs',logs.filter(l=>l.includes('panic')||l.includes('BIOS')||l.includes('virtio')||l.includes('ERROR')).slice(0,20).join('\n'));
await b.close();
console.log('[tinycore] done');
