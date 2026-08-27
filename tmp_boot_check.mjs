import puppeteer from 'puppeteer-core';
const CHROME='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const URL='http://localhost:8080/android.html';
const b=await puppeteer.launch({executablePath:CHROME, headless:true, args:['--no-sandbox','--disable-setuid-sandbox']});
const p=await b.newPage();
const logs=[];
p.on('console',m=>{
  const t=m.text();
  logs.push(t);
  console.log('[page]',t.slice(0,800));
});
p.on('pageerror',e=>console.log('pageerror',e.message.slice(0,800)));
await p.goto(URL,{waitUntil:'domcontentloaded', timeout:25000});
await new Promise(r=>setTimeout(r,15000));
const res=await p.evaluate(async()=>{
  const out={};
  try{
    const {globalLogcat}=await import('./src/logger.js');
    out.milestones = globalLogcat.entries.filter(e=>e.message.includes('BOOT-MILESTONE')||e.message.includes('KERNEL')||e.message.includes('panic')||e.message.includes('GUEST')).slice(-20).map(e=>`[${e.tag}:${e.priority}] ${e.message}`.slice(0,400));
    out.serial = globalLogcat.entries.filter(e=>e.tag==='serial' || e.message.includes('[init]')||e.message.includes('Linux version')).slice(-30).map(e=>`[${e.tag}] ${e.message}`.slice(0,400));
    out.bios = globalLogcat.entries.filter(e=>e.message.includes('BIOS')).slice(-5).map(e=>e.message.slice(0,300));
  }catch(e){out.logErr=e.message.slice(0,500)}
  try{
    const c=document.querySelector('canvas');
    out.canvas={w:c?.width, h:c?.height, hasContext:!!c?.getContext('2d')};
    const ctx=c?.getContext('2d');
    if(ctx){
      const d=ctx.getImageData(0,0,10,10).data;
      out.pixels=Array.from(d.slice(0,20));
      let nonBlack=0;
      for(let i=0;i<d.length;i+=4){ if(d[i]!==0||d[i+1]!==0||d[i+2]!==0) nonBlack++; }
      out.nonBlack=nonBlack;
    }
  }catch(e){out.canvasErr=e.message.slice(0,300)}
  try{
    const r=await fetch('./guest/build/bzImage',{headers:{Range:'bytes=0-2048'}});
    const b=new Uint8Array(await r.arrayBuffer());
    let ver="";
    for(let i=0;i<200;i++){ if(b[0x200+i]===0) break; ver+=String.fromCharCode(b[0x200+i]); }
    out.bzVersion=ver.slice(0,100);
    out.bzHdr=String.fromCharCode(...b.slice(0x202,0x206));
  }catch(e){out.bzErr=e.message.slice(0,500)}
  return out;
});
console.log(JSON.stringify(res,null,2));
console.log('--- last 30 logs ---');
console.log(logs.slice(-30).join('\n').slice(0,4000));
await b.close();
