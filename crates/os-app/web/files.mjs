const DB='lemon-studio';
function database(){return new Promise((resolve,reject)=>{
  const request=indexedDB.open(DB,1);
  request.onupgradeneeded=()=>request.result.createObjectStore('workspace');
  request.onerror=()=>reject(request.error);
  request.onblocked=()=>reject(new Error('Close another Lemon Studio tab to update browser storage.'));
  request.onsuccess=()=>resolve(request.result);
});}
export async function restore(){
  const db=await database();try{return await new Promise((resolve,reject)=>{const r=db.transaction('workspace').objectStore('workspace').get('current');r.onsuccess=()=>resolve(r.result??null);r.onerror=()=>reject(r.error);});}finally{db.close();}
}
export async function persist(snapshot){
  const db=await database();try{await new Promise((resolve,reject)=>{
    const tx=db.transaction('workspace','readwrite');
    tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error??new Error('Browser save aborted'));
    tx.objectStore('workspace').put(snapshot,'current');
  });}finally{db.close();}
}
export async function save_preferences(preferences){
  const db=await database();try{await new Promise((resolve,reject)=>{
    const tx=db.transaction('workspace','readwrite'),store=tx.objectStore('workspace'),r=store.get('current');
    tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);tx.onabort=()=>reject(tx.error??new Error('Preferences save aborted'));
    r.onsuccess=()=>{try{store.put({...r.result,preferences},'current');}catch(error){tx.abort();reject(error);}};
  });}finally{db.close();}
}
export function download(name,bytes,mime){
  const url=URL.createObjectURL(new Blob([Uint8Array.from(bytes)],{type:mime}));
  const link=document.createElement('a');link.href=url;link.download=name;link.hidden=true;
  try{document.body.append(link);link.click();}finally{link.remove();setTimeout(()=>URL.revokeObjectURL(url),30000);}
}
export function pick_files(kind){return new Promise((resolve,reject)=>{
  const input=document.createElement('input');input.type='file';input.hidden=true;
  input.accept=kind==='project'?'.flp':kind==='theme'?'.flstheme,.txt':'.wav,.flac,.ogg,.mp3,.aif,.aiff';
  if(kind==='folder'){input.webkitdirectory=true;input.multiple=true;input.accept='';}
  input.oncancel=()=>{input.remove();resolve(null);};
  input.onchange=async()=>{try{
    const selected=[...input.files];
    // The shared WASM memory is capped at 2 GiB; leave room for decoded audio and the editor.
    if(selected.reduce((n,f)=>n+f.size,0)>256*1024*1024)throw new Error('Import is larger than the 256 MiB browser limit.');
    resolve(await Promise.all(selected.map(async file=>({path:file.webkitRelativePath||file.name,bytes:new Uint8Array(await file.arrayBuffer())}))));
  }catch(error){reject(error);}finally{input.remove();}};
  document.body.append(input);input.click();
});}
export function dirty(value){globalThis.lemonUnsaved=value;}
if(typeof window!=='undefined')window.addEventListener('beforeunload',event=>{if(globalThis.lemonUnsaved){event.preventDefault();event.returnValue='';}});
