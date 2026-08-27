import puppeteer from 'puppeteer-core';
const CHROME='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL='http://localhost:8080/android.html';
console.log('[real_boot] launching chrome for 5.10 real kernel boot');
const browser=await puppeteer.launch({executablePath:CHROME, headless:true, args:['--no-sandbox','--disable-setuid-sandbox','--disable-dev-shm-usage']});
const page=await browser.newPage();
const logs=[];
page.on('console',m=>{
  const t=m.text();
  logs.push({type:m.type(), text:t});
  // Show only important
  if(t.includes('SeaBIOS')||t.includes('Linux version')||t.includes('virtio')||t.includes('binder')||t.includes('panic')||t.includes('GUEST-PANIC')||t.includes('BOOT-MILESTONE')||t.includes('[init]')||t.includes('Kernel panic')||t.includes('Uncompressing')||t.includes('v86')||t.includes('ERROR')||t.includes('synthetic')){
    console.log('[page]',t.slice(0,900));
  }
});
page.on('pageerror',e=>console.log('pageerror',e.message.slice(0,900)));
console.log('[real_boot] goto',URL);
await page.goto(URL,{waitUntil:'domcontentloaded', timeout:30000});
console.log('[real_boot] waiting 35s for kernel to boot and reach init...');
await new Promise(r=>setTimeout(r,35000));
console.log('[real_boot] capture via evaluate');
const res=await page.evaluate(async()=>{
  const out={bz:{}, milestones:[], serial:[], state:null, v86:null};
  try{
    const r=await fetch('./guest/build/bzImage',{headers:{Range:'bytes=0-2048'}});
    const b=new Uint8Array(await r.arrayBuffer());
    out.bz={status:r.status, hdr:String.fromCharCode(...b.slice(0x202,0x206)), aa55:(b[0x1FE]|b[0x1FF]<<8).toString(16), sizeHint:b.length};
    // try to extract version string from bzImage header area (0x200+)
    let ver="";
    for(let i=0;i<256;i++){
      const c=b[0x200+i];
      if(c===0) break;
      if(c>=32&&c<127) ver+=String.fromCharCode(c);
    }
    out.bz.verRaw=ver.slice(0,120);
    // also fetch via header text via nm? just note
  }catch(e){out.bzErr=e.message.slice(0,400)}
  try{
    const {globalLogcat}=await import('./src/logger.js');
    const entries=globalLogcat.entries;
    out.totalEntries=entries.length;
    out.milestones=entries.filter(e=>e.message.includes('BOOT-MILESTONE')).map(e=>`[${e.tag}:${e.priority}] ${e.message}`.slice(0,500));
    out.serial=entries.filter(e=>e.message.includes('Linux version')||e.message.includes('SeaBIOS')||e.message.includes('virtio')||e.message.includes('binder')||e.message.includes('[init]')||e.message.includes('Kernel panic')||e.message.includes('Uncompressing')||e.message.includes('GUEST-TTY')||e.message.includes('zygote')||e.message.includes('servicemanager')).slice(-60).map(e=>`[${e.tag}:${e.priority}] ${e.message}`.slice(0,500));
    out.bios=entries.filter(e=>e.message.includes('BIOS')).slice(-5).map(e=>e.message.slice(0,300));
    out.panic=entries.filter(e=>e.message.includes('panic')||e.message.includes('GUEST-PANIC')||e.message.includes('Fatal')).map(e=>e.message.slice(0,400));
    // try to get V86GuestManager state if exposed
    try{
      // Look for window global or via import
      const mgrMod=await import('./src/v86_guest_manager.js');
      // Find any instance? Check if window has manager
      out.mgrStates=Object.keys(mgrMod);
    }catch(e){}
  }catch(e){out.logErr=e.message.slice(0,600)}
  try{
    const c=document.querySelector('canvas');
    out.canvas={w:c?.width, h:c?.height, hasCtx:!!c?.getContext('2d')};
    if(c){
      const ctx=c.getContext('2d');
      if(ctx){
        const d=ctx.getImageData(0,0,16,16).data;
        let nonBlack=0;
        for(let i=0;i<d.length;i+=4) if(d[i]!==0||d[i+1]!==0||d[i+2]!==0) nonBlack++;
        out.canvas.nonBlack16=nonBlack;
        out.canvas.sample=Array.from(d.slice(0,16));
      }
    }
    out.title=document.title;
    out.url=location.href;
  }catch(e){out.canvasErr=e.message.slice(0,300)}
  return out;
});
console.log(JSON.stringify(res,null,2));
console.log('--- recent logs filter ---');
console.log(logs.filter(l=>l.text.includes('Linux')||l.text.includes('SeaBIOS')||l.text.includes('virtio')||l.text.includes('panic')||l.text.includes('BOOT-MILESTONE')||l.text.includes('[init]')).slice(-30).map(l=>l.text.slice(0,500)).join('\n'));
await browser.close();
console.log('[real_boot] done');
