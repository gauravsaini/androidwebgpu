import puppeteer from 'puppeteer-core';
const CHROME='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
for(const url of ['http://127.0.0.1:8001/android.html','http://localhost:8001/android.html','http://example.com']){
  console.log('trying',url);
  const b=await puppeteer.launch({executablePath:CHROME, headless:true, args:['--no-sandbox','--disable-setuid-sandbox','--disable-dev-shm-usage']});
  const p=await b.newPage();
  p.on('requestfailed',r=>console.log('reqfail',r.url(), r.failure()?.errorText));
  try{
    const resp=await p.goto(url,{waitUntil:'domcontentloaded', timeout:8000});
    console.log('success',url, resp?.status());
    console.log('content', (await p.content()).slice(0,200));
  }catch(e){ console.log('fail',url, e.message.slice(0,300));}
  await b.close();
}
