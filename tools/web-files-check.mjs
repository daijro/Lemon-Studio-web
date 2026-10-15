import assert from 'node:assert/strict';
import { readFile, mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export async function checkFiles(page) {
  page.setDefaultTimeout(20000);
  page.on('dialog',d=>{d.accept();});
  const snapshot=()=>page.evaluate(async()=>JSON.parse((await import('./os_app.js')).snapshot()));
  const wait=async(predicate)=>{const end=Date.now()+20000;let state;while(!predicate(state=await snapshot())){assert.ok(Date.now()<end,`Timed out: ${predicate}; ${JSON.stringify({notes:state.notes,channels:state.channels,dirty:state.dirty,message:state.message})}`);await page.waitForTimeout(25);}};
  const click=async(action,options)=>{const s=await snapshot(),h=s.hits.findLast(h=>h.action===action||h.hint===action);assert.ok(h,`Missing ${action}`);const[x,y,w,z]=h.rect;await page.mouse.click(x+w/2,y+z/2,options);await page.waitForTimeout(150);};
  const note=async(offset)=>{const h=(await snapshot()).hits.find(h=>h.action==='PianoGrid');await page.mouse.click(h.rect[0]+offset,h.rect[1]+100);await page.waitForTimeout(150);};
  const save=async()=>{const done=page.waitForEvent('download');await page.keyboard.press('Control+s');return done;};
  const storage=()=>page.evaluate(async()=>{const db=await new Promise((ok,no)=>{const r=indexedDB.open('lemon-studio',1);r.onsuccess=()=>ok(r.result);r.onerror=()=>no(r.error);});try{return await new Promise((ok,no)=>{const r=db.transaction('workspace').objectStore('workspace').get('current');r.onsuccess=()=>ok({project:[...r.result.project],files:r.result.files.map(f=>({path:f.path,bytes:[...f.bytes]}))});r.onerror=()=>no(r.error);});}finally{db.close();}});
  await page.locator('html[data-lemon-ready=true]').waitFor(); await wait(s=>!s.loading);
  await click('Menu(2)');await click('MenuItem(2, 0)');await click('Lemon Trio');
  await page.locator('.plugin-window canvas').waitFor();await page.getByRole('button',{name:'Close instrument editor'}).click();
  await click('ToggleWin(PianoRoll)');await note(40);await wait(s=>s.notes===1);
  // Saving must remain responsive when the audio worker owns an instrument during completion.
  await page.evaluate(()=>{globalThis.originalTransaction=IDBDatabase.prototype.transaction;IDBDatabase.prototype.transaction=function(...args){const tx=originalTransaction.apply(this,args);if(args[1]==='readwrite'){const until=performance.now()+1000,store=tx.objectStore('workspace');const keep=()=>{if(performance.now()<until)store.get('current').onsuccess=keep;};keep();}return tx;};});
  await page.evaluate(async()=>{(await import('./os_app.js')).hold_plugin_lock(500);});await wait(s=>s.pluginLocked);
  const downloaded=await save();
  await page.evaluate(async()=>{(await import('./os_app.js')).hold_plugin_lock(2000);});await wait(s=>s.pluginLocked);const flp=await readFile(await downloaded.path());assert.equal(flp.subarray(0,4).toString(),'FLhd');
  await wait(s=>!s.dirty&&s.message?.includes('Saved in this browser'));
  await page.evaluate(()=>IDBDatabase.prototype.transaction=originalTransaction);
  const good=await storage();
  await page.reload();await page.locator('html[data-lemon-ready=true]').waitFor();await wait(s=>!s.loading&&s.notes===1);
  // Malformed and cancelled selections must leave the current project untouched.
  let chooser=page.waitForEvent('filechooser');await page.keyboard.press('Control+o');await (await chooser).setFiles({name:'bad.flp',mimeType:'application/octet-stream',buffer:Buffer.from('broken')});
  await wait(s=>s.message?.startsWith('Import failed:'));assert.equal((await snapshot()).notes,1);
  chooser=page.waitForEvent('filechooser');await page.keyboard.press('Control+o');await chooser;
  await page.locator('input[type=file]').evaluate(e=>e.dispatchEvent(new Event('cancel')));assert.equal((await snapshot()).notes,1);
  await click('ToggleWin(PianoRoll)');await note(120);await wait(s=>s.notes===2&&s.dirty);
  await page.evaluate(()=>{globalThis.originalPut=IDBObjectStore.prototype.put;IDBObjectStore.prototype.put=function(){throw new DOMException('Injected quota failure','QuotaExceededError');};});
  await save();await wait(s=>s.message?.startsWith('Browser save failed:'));assert.equal((await snapshot()).dirty,true);assert.deepEqual(await storage(),good);
  await page.evaluate(()=>{IDBObjectStore.prototype.put=originalPut;globalThis.originalClick=HTMLAnchorElement.prototype.click;HTMLAnchorElement.prototype.click=function(){throw new Error('Injected download failure');};});
  await page.keyboard.press('Control+s');await wait(s=>s.message?.startsWith('Download failed:'));assert.equal((await snapshot()).dirty,true);assert.deepEqual(await storage(),good);
  await page.evaluate(()=>{HTMLAnchorElement.prototype.click=originalClick;});
  // Hold a transaction open while the user edits; its completion must not clear those newer edits.
  await page.evaluate(()=>{globalThis.originalTransaction=IDBDatabase.prototype.transaction;IDBDatabase.prototype.transaction=function(...args){const tx=originalTransaction.apply(this,args);if(args[1]==='readwrite'){const until=performance.now()+1500,store=tx.objectStore('workspace');const keep=()=>{if(performance.now()<until)store.get('current').onsuccess=keep;};keep();}return tx;};});
  await save();await note(200);await wait(s=>s.notes===3);
  await wait(s=>s.message?.startsWith('Earlier version saved'));
  assert.equal((await snapshot()).dirty,true);await page.evaluate(()=>IDBDatabase.prototype.transaction=originalTransaction);
  await page.evaluate(async()=>{(await import('./os_app.js')).hold_plugin_lock(500);});await wait(s=>s.pluginLocked);
  const exported=page.waitForEvent('download');await page.keyboard.press('Control+r');const wav=await readFile(await (await exported).path());
  assert.equal(wav.subarray(0,4).toString(),'RIFF');assert.equal(wav.readUInt32LE(24),48000);
  let peak=0;for(let i=44;i+4<=wav.length;i+=4)peak=Math.max(peak,Math.abs(wav.readFloatLE(i)));assert.ok(peak>0.001&&peak<=2,`WAV peak ${peak}`);assert.equal((await snapshot()).notes,3);assert.equal((await snapshot()).dirty,true);
  // An import that finishes after starting a new project must be ignored.
  await page.evaluate(()=>{globalThis.originalArrayBuffer=File.prototype.arrayBuffer;File.prototype.arrayBuffer=function(){return new Promise(resolve=>{globalThis.finishFile=()=>originalArrayBuffer.call(this).then(resolve);});};});
  chooser=page.waitForEvent('filechooser');await page.keyboard.press('Control+o');await (await chooser).setFiles({name:'earlier.flp',mimeType:'application/octet-stream',buffer:flp});
  await page.waitForFunction(()=>!!globalThis.finishFile);await click('Menu(0)');await click('MenuItem(0, 0)');await wait(s=>s.notes===0);
  await page.evaluate(()=>{File.prototype.arrayBuffer=originalArrayBuffer;finishFile();});await page.waitForTimeout(500);assert.equal((await snapshot()).notes,0);
  // A newly imported sample must reopen from a downloaded FLP beside the original sample.
  await wait(s=>!s.loading);
  if(!(await snapshot()).hits.some(h=>h.action==='ChannelSelect(0)'))await click('ToggleWin(ChannelRack)');
  await click('ChannelSelect(0)',{button:'right'});
  chooser=page.waitForEvent('filechooser');await click('Load sample...');
  const originalSample=await readFile(join(process.env.WEB_FILE_FIXTURE??'target/web-files','a/piano.wav'));
  await (await chooser).setFiles({name:'piano.wav',mimeType:'audio/wav',buffer:originalSample});
  await wait(s=>s.dirty);
  const importedDownload=await save();const importedFlp=await readFile(await importedDownload.path());
  assert.ok(importedFlp.includes(Buffer.from('piano.wav','utf16le')));
  assert.ok(!importedFlp.includes(Buffer.from('/imports/','utf16le')),'Downloaded FLP must not contain browser-only sample paths');
  await wait(s=>!s.dirty);
  const portable=await mkdtemp(join(tmpdir(),'lemon-portable-'));
  try {
    await writeFile(join(portable,'song.flp'),importedFlp);await writeFile(join(portable,'piano.wav'),originalSample);
    chooser=page.waitForEvent('filechooser');await click('Menu(0)');await click('MenuItem(0, 17)');await (await chooser).setFiles(portable);
    await wait(s=>!s.loading&&s.channels===1&&s.message?.endsWith('/song.flp'));
    if((await snapshot()).hits.some(h=>h.action==='CloseWin(ChannelRack)'))await click('CloseWin(ChannelRack)');
    if(!(await snapshot()).hits.some(h=>h.action==='PianoGrid'))await click('ToggleWin(PianoRoll)');
    await note(40);await wait(s=>s.notes===1);
    const roundTripExport=page.waitForEvent('download');await page.keyboard.press('Control+r');const audio=await readFile(await (await roundTripExport).path());
    let peak=0;for(let i=44;i+4<=audio.length;i+=4)peak=Math.max(peak,Math.abs(audio.readFloatLE(i)));
    assert.ok(peak>0.001,'Reopened downloaded project must resolve its imported sample');
  } finally {await rm(portable,{recursive:true,force:true});}
  // Folder import retains both same-named samples and unavailable native plugin bytes.
  chooser=page.waitForEvent('filechooser');await click('Menu(0)');await click('MenuItem(0, 17)');await (await chooser).setFiles(process.env.WEB_FILE_FIXTURE??'target/web-files');
  await wait(s=>!s.loading&&s.channels===6);const folderDownload=await save();const folderFlp=await readFile(await folderDownload.path());
  assert.ok(folderFlp.includes(Buffer.from([173,219,52,222,239,157,6,210])),'Unavailable plugin state must survive');
  await wait(s=>!s.dirty&&s.message?.includes('Saved in this browser'));const samples=(await storage()).files;
  assert.equal(samples.length,2);const a=samples.find(f=>f.path.endsWith('/a/piano.wav')),b=samples.find(f=>f.path.endsWith('/b/piano.wav'));assert.ok(a&&b);assert.notDeepEqual(a.bytes,b.bytes);
  await page.reload();await page.locator('html[data-lemon-ready=true]').waitFor();await wait(s=>!s.loading&&s.channels===6);assert.deepEqual((await storage()).files,samples);
  await click('ChannelSelect(0)');await page.locator('.plugin-window canvas').waitFor();await page.waitForTimeout(250);
  const sampler=await page.locator('.plugin-window canvas').boundingBox();await page.mouse.click(sampler.x+100,sampler.y+300,{button:'right'});
  const sampleDownload=page.waitForEvent('download');await page.getByRole('button',{name:'Save as...',exact:true}).click();const sampleWav=await readFile(await (await sampleDownload).path());assert.equal(sampleWav.subarray(0,4).toString(),'RIFF');assert.ok(sampleWav.length>38000);
  await page.getByRole('button',{name:'Close instrument editor'}).click();
  // A damaged saved project must be reported, not silently treated as a new empty session.
  await page.evaluate(async()=>{const db=await new Promise(ok=>{const r=indexedDB.open('lemon-studio',1);r.onsuccess=()=>ok(r.result);});await new Promise((ok,no)=>{const tx=db.transaction('workspace','readwrite');tx.objectStore('workspace').put({version:1,path:'broken.flp',project:new Uint8Array([1]),files:[]},'current');tx.oncomplete=ok;tx.onerror=()=>no(tx.error);});db.close();});
  await page.reload();await page.locator('html[data-lemon-ready=true]').waitFor();await wait(s=>/^Could not restore browser project:/.test(s.message??''));
  await page.addInitScript(()=>{indexedDB.open=()=>{throw new DOMException('Injected storage denial','SecurityError');};});
  await page.reload();await page.locator('html[data-lemon-ready=true]').waitFor();await wait(s=>/storage unavailable|Preferences could not be saved/.test(s.message??''));
  console.log('PASS: FLP save/reload, invalid/cancelled import, quota/download failure, stale save, non-silent WAV export and corrupt storage');
}
