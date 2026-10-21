// Run with NODE_PATH pointing to an installed Playwright package. Generated proofs stay outside git.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const http=require('node:http');
const path=require('node:path');
const root=path.resolve(__dirname,'../..');
const dist=path.join(root,'dist/c0host');
const adapterOnly=process.argv.includes('--adapter-only');
let oracle=null;
if(process.env.C0VID_AAC_ORACLE){
  const bytes=fs.readFileSync(process.env.C0VID_AAC_ORACLE);const packets=[];let asc=null;
  for(let offset=0;offset+7<=bytes.length;){
    assert.equal(bytes[offset],255);const b=bytes.subarray(offset);const size=((b[3]&3)<<11)|(b[4]<<3)|(b[5]>>5);
    assert(size>=7 && offset+size<=bytes.length);const profile=(b[2]>>6)+1,index=(b[2]>>2)&15,channels=((b[2]&1)<<2)|(b[3]>>6);
    asc=[(profile<<3)|(index>>1),((index&1)<<7)|(channels<<3)];packets.push(Array.from(b.subarray((b[1]&1)?7:9,size)));offset+=size;
  }
  oracle={asc,packets};
}
const server=http.createServer((request,response)=>{
  const route=new URL(request.url,'http://localhost').pathname;
  const fixture=route==='/__fixture.mp4' && process.env.C0VID_MEDIA_FIXTURE;
  const file=fixture || (route==='/__adapter.js'?path.join(root,'apps/filmcraft-web/src/page_control/codecs.js'):path.join(dist,route==='/'?'index.html':route));
  if(!fixture && !file.startsWith(root+path.sep)){response.writeHead(403).end();return;}
  if(adapterOnly && route==='/'){response.setHeader('Content-Type','text/html');response.end('<!doctype html><title>Codec adapter test</title>');return;}
  if(!fs.existsSync(file)){response.writeHead(404).end();return;}
  response.setHeader('Content-Type',file.endsWith('.wasm')?'application/wasm':file.endsWith('.js')?'text/javascript':file.endsWith('.html')?'text/html':'application/octet-stream');
  fs.createReadStream(file).pipe(response);
});
(async()=>{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const url=`http://127.0.0.1:${server.address().port}`;
  const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader','--disable-gpu-sandbox']});
  try {
    const page=await browser.newPage({viewport:{width:1280,height:800}});
    const errors=[];page.on('pageerror',error=>errors.push(String(error)));
    page.on('console',message=>{if(message.type()==='error'||message.text().includes('download '))console.log('BROWSER-CONSOLE '+message.text());});
    await page.goto(url);
    const adapter=await page.evaluate(async(oracle)=>{
      const c=await import('/__adapter.js');const caps=await c.probeCodecs();
      const result={caps};
      if(caps.video){
        const video=c.createVideo(64,64,30,1,1000000,{codec:'avc1.640033',keyInterval:60,bitrateMode:'variable'});
        for(let i=0;i<3;i++)c.encodeVideo(video,new Uint8Array(64*64*4).fill(120+i*10),i);
        for(let i=0;!c.encoderFinishReady(video)&&i<500;i++)await new Promise(resolve=>setTimeout(resolve,5));
        const packets=c.takePackets(video);result.videoPackets=packets.length;result.avcDescriptionBytes=video.description?.length;result.avcProfile=video.description?.[1];c.closeEncoder(video);
      }
      if(caps.audio){
        const audio=c.createAudio(48000,2,128000);const planar=new Float32Array(4800*2);
        for(let i=0;i<4800;i++){planar[i]=planar[4800+i]=0.1*Math.sin(i*2*Math.PI*440/48000);}
        c.encodeAudio(audio,planar,4800);
        for(let i=0;!c.encoderFinishReady(audio)&&i<500;i++)await new Promise(resolve=>setTimeout(resolve,5));
        const packets=c.takePackets(audio);result.audioPackets=packets.length;result.aacDescriptionBytes=audio.description?.length;
        if(caps.audioDecode && packets.length){
          const decoder=c.createAudioDecoder(audio.description,48000,2);
          packets.forEach((packet,i)=>c.decodeAudio(decoder,packet.bytes,Math.round(i*1024*1e6/48000),Math.round(1024*1e6/48000)));
          c.flushAudioDecoder(decoder);let pcm=null;
          for(let i=0;pcm===null && i<500;i++){pcm=c.readAudio(decoder,0,4800,48000);if(pcm===null)await new Promise(resolve=>setTimeout(resolve,5));}
          result.decodedSamples=pcm?.length;result.audible=pcm?.some(sample=>Math.abs(sample)>0.001);c.closeAudioDecoder(decoder);
        }
        c.closeEncoder(audio);
      }
      if(oracle && caps.audioDecode){
        const decoder=c.createAudioDecoder(new Uint8Array(oracle.asc),48000,2);
        oracle.packets.forEach((bytes,index)=>c.decodeAudio(decoder,new Uint8Array(bytes),Math.round(index*1024*1e6/48000),Math.round(1024*1e6/48000)));
        c.flushAudioDecoder(decoder);let pcm=null;
        for(let i=0;pcm===null&&i<500;i++){pcm=c.readAudio(decoder,0,9600,48000);if(pcm===null)await new Promise(resolve=>setTimeout(resolve,5));}
        result.oracleSamples=pcm?.length;result.oracleAudible=pcm?.some(sample=>Math.abs(sample)>0.001);c.closeAudioDecoder(decoder);
      }
      return result;
    },oracle);
    if(adapter.caps.video){assert.equal(adapter.videoPackets,3);assert(adapter.avcDescriptionBytes>0);assert.equal(adapter.avcProfile,100);}
    if(adapter.caps.audio){assert(adapter.audioPackets>0);assert(adapter.aacDescriptionBytes>0);}
    if(adapter.caps.audio && adapter.caps.audioDecode){assert.equal(adapter.decodedSamples,9600);assert.equal(adapter.audible,true);}
    if(oracle && adapter.caps.audioDecode){assert.equal(adapter.oracleSamples,19200);assert.equal(adapter.oracleAudible,true);}
    console.log('CODEC-ADAPTER '+JSON.stringify(adapter));
    if(adapterOnly){assert.deepEqual(errors,[]);return;}
    await page.waitForFunction(()=>window.filmcraftLoad?.readyMs || window.filmcraftLoad?.error,{},{timeout:120000});
    const startup=await page.evaluate(()=>window.filmcraftLoad);assert(!startup.error,JSON.stringify(startup));
    const request=(method,params={})=>page.evaluate(async({method,params})=>JSON.parse(await window.__c0hostRequest(JSON.stringify({method,params}))),{method,params});
    const call=async(method,params)=>{const reply=await request(method,params);assert.equal(reply.ok,true,method+': '+JSON.stringify(reply));return reply.result;};
    const initial=await call('c0.inspect');assert.deepEqual(initial.sequences,[]);assert.equal(initial.dirty,false);
    assert.equal(initial.panes.community_links,false);assert.equal(initial.panes.start_screen,false);
    await call('c0.set',{themeTokens:{accent:'#123456'},panels:{status_bar:true}});
    const report=await call('c0.set',{themeTokens:{not_a_token:'#ffffff',text:'bad'},panels:{missing:true,header:2}});
    assert(report.unknown.includes('themeTokens.not_a_token'));assert(report.invalid.includes('panels.header'));
    await call('c0.newDocument',{template:'blank'});
    await call('engine.execute',{command:'file.newSequence',params:{name:'Smoke',width:64,height:64,fps:30,video:1,audio:1}});
    let state=await call('c0.inspect');const sequence=state.activeSequence;
    await call('c0.goTo',{sequence,frame:2});
    assert.equal((await call('c0.inspect')).playheadFrame,2);
    await call('c0.play',{action:'play'});await call('c0.play',{action:'pause'});await call('c0.play',{action:'toggle'});await call('c0.play',{action:'pause'});
    const bytes=await call('c0.documentBytes',{format:'fcproj'});assert(bytes.size>0);assert(Array.isArray(bytes.referencedMedia));
    const pending=await call('c0.openFile',{name:'real.fcproj',bytes:bytes.bytes});assert.equal(pending.pending,true);
    assert.equal((await call('c0.cancelFileDialog')).cancelled,true);
    await call('engine.execute',{command:'file.newProject',params:{name:'Clear'}});
    const opened=await call('c0.openFile',{name:'real.fcproj',bytes:bytes.bytes});assert.equal(opened.pending,false);
    assert.equal((await call('c0.inspect')).sequences[0].name,'Smoke');
    await page.evaluate(async bytes=>{const data=Uint8Array.from(atob(bytes),c=>c.charCodeAt(0));return window.filmcraft.openProject(new File([data],'browser.fcproj'));},bytes.bytes);
    assert.equal((await call('c0.inspect')).project.name,'Untitled');
    // All c0 methods settle even after animation frame scheduling is disabled.
    await page.evaluate(()=>{window.__savedRAF=window.requestAnimationFrame;window.__blockedRAF=[];window.requestAnimationFrame=callback=>{window.__blockedRAF.push(callback);return 0;};});
    await page.waitForTimeout(100);
    for(const [method,params] of [['c0.inspect',{}],['c0.documentBytes',{}],['c0.goTo',{sequence,frame:1}],['c0.play',{action:'pause'}],['c0.set',{}],['c0.cancelFileDialog',{}],['c0.newDocument',{}],['c0.openFile',{name:'hidden.fcproj',bytes:bytes.bytes}]])await call(method,params);
    await page.evaluate(()=>{window.requestAnimationFrame=window.__savedRAF;window.__blockedRAF.forEach(callback=>window.requestAnimationFrame(callback));});
    await call('engine.execute',{command:'sequence.open',params:{item:sequence}});
    if(process.env.C0VID_MEDIA_FIXTURE){
      const imported=await page.evaluate(()=>window.filmcraft.importUrl('/__fixture.mp4','fixture.mp4'));
      assert.equal(imported.errors.length,0,JSON.stringify(imported));assert(imported.items.length>0);
      const first=imported.items[0];const item=typeof first==='number'?first:first.id;
      assert.equal(typeof item,'number',JSON.stringify(imported));
      await call('engine.execute',{command:'file.newSequence',params:{name:'Media smoke',fromItem:item}});
      await call('c0.goTo',{frame:2});await page.mouse.click(100,100);
      await call('c0.play',{action:'play'});await page.waitForTimeout(500);await call('c0.play',{action:'pause'});
      await page.waitForFunction(()=>window.filmcraft.info().webcodecsStats.framesDecoded>0,{},{timeout:30000});
      console.log('BROWSER-MEDIA-IMPORT '+JSON.stringify(imported));
    }
    const info=await page.evaluate(()=>window.filmcraft.info());
    assert.equal(info.softwarePatentCodecs,false,"the shipping page must exclude software patent codecs");
    if(info.browserCodecs?.h264Encoder){
      const exportResult=await call('engine.execute',{command:'file.exportMedia',params:{format:'h264',path:'/exports/c0vid-smoke.mp4',range:'custom',startFrame:0,endFrame:3,width:64,height:64,fps:30,settings:{includeAudio:!!info.browserCodecs.aacEncoder,audio:{codec:'auto'}}}});
      await page.waitForFunction(()=>{const files=window.filmcraft.files();return files.some(file=>file.path==='/exports/c0vid-smoke.mp4');},{},{timeout:60000});
      let output;
      try {output=await page.evaluate(async()=>Array.from(await window.filmcraft.readFile('/exports/c0vid-smoke.mp4')));}
      catch(error){console.log('EXPORT-DEBUG '+JSON.stringify(await page.evaluate(async()=>({files:filmcraft.files(),jobs:await filmcraft.request('engine.execute',{command:'jobs.list',params:{}}),blobSize:new Blob([new Uint8Array([1,2])]).size,info:filmcraft.info()}))));throw error;}
      assert(output.length>128);fs.writeFileSync(process.env.C0VID_EXPORT_PROOF||'/tmp/c0vid-export.mp4',Buffer.from(output));
      console.log('BROWSER-MP4-EXPORT '+JSON.stringify({bytes:output.length,result:exportResult}));
    }
    await page.screenshot({path:process.env.C0VID_SCREENSHOT||'/tmp/c0vid-smoke.png'});
    const unavailable=await browser.newPage({viewport:{width:800,height:600}});
    unavailable.on('pageerror',error=>errors.push(String(error)));
    await unavailable.goto(url+'?nowebcodecs');
    await unavailable.waitForFunction(()=>window.filmcraftLoad?.readyMs || window.filmcraftLoad?.error,{},{timeout:120000});
    const unsupported=await unavailable.evaluate(async()=>{
      const request=async(method,params={})=>JSON.parse(await window.__c0hostRequest(JSON.stringify({method,params})));
      if(window.filmcraftLoad.error)throw new Error(window.filmcraftLoad.error);
      const created=await request('engine.execute',{command:'file.newSequence',params:{name:'Unsupported smoke',width:64,height:64,fps:30}});
      if(!created.ok)throw new Error(JSON.stringify(created));
      const reply=await request('engine.execute',{command:'file.exportMedia',params:{format:'h264',path:'/exports/unsupported.mp4',range:'custom',startFrame:0,endFrame:1,audio:false}});
      return {info:window.filmcraft.info(),reply,alive:await request('c0.inspect')};
    });
    assert.equal(unsupported.info.softwarePatentCodecs,false);
    assert.equal(unsupported.info.browserCodecs.h264Encoder,false);
    assert.equal(unsupported.info.browserCodecs.aacEncoder,false);
    assert.equal(unsupported.info.browserCodecs.aacDecoder,false);
    assert.equal(unsupported.reply.ok,false,JSON.stringify(unsupported.reply));
    assert.match(unsupported.reply.error,/WebCodecs|not supported|not available|unsupported/i);
    assert.equal(unsupported.alive.ok,true);
    console.log('BROWSER-UNSUPPORTED '+JSON.stringify(unsupported.reply));
    await unavailable.close();
    assert.deepEqual(errors,[]);
    console.log('PAGE-SMOKE '+JSON.stringify({startup,initial,info,hostMethods:8,realFileOpened:true,unpresentedRequests:8,errors}));
  }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(()=>server.close());
