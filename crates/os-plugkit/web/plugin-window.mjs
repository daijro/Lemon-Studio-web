const windows = new Map();
export function open(id, w, h, scale, rustCallback) {
  // Opening a dialog moves focus synchronously; queue its blur event after Rust returns.
  const callback=(...args)=>queueMicrotask(()=>{if(windows.has(id))rustCallback(...args);});
  close(id);
  const panel = document.createElement('section');
  panel.className = 'plugin-window';
  panel.style.cssText = 'position:fixed;z-index:20;left:50%;top:110px;transform:translateX(-50%);max-width:95vw;max-height:85vh;overflow:auto;background:#292d2f;border:1px solid #16191b;box-shadow:0 8px 30px #0009';
  const bar = document.createElement('div'); bar.style.cssText='display:flex;justify-content:space-between;padding:5px 10px;cursor:move;background:#373d40';
  const title=document.createElement('span');title.textContent='Lemon Studio';bar.append(title);
  const button=document.createElement('button'); button.textContent='×';button.ariaLabel='Close instrument editor';
  bar.append(button);
  const canvas=document.createElement('canvas'); canvas.tabIndex=0;canvas.ariaLabel='Instrument controls';
  canvas.width=w;canvas.height=h;canvas.style.cssText=`display:block;width:${w/scale}px;height:${h/scale}px;touch-action:none;outline:none`;
  for(const b of [button])b.style.cssText='color:#ddd;background:#454d50;border:1px solid #202729;border-radius:3px;padding:2px 8px';
  panel.append(bar,canvas);document.body.append(panel);
  const state={panel,bar,canvas,callback,scale,overlay:null,frame:0}; windows.set(id,state);
  const send=(kind,x=0,y=0,value=0,mods=0)=>callback(id,kind,x,y,value,mods);
  button.onclick=()=>send('close');
  const pos=e=>{const r=canvas.getBoundingClientRect();return[(e.clientX-r.left)*canvas.width/r.width,(e.clientY-r.top)*canvas.height/r.height];};
  const mods=e=>(e.ctrlKey?1:0)|(e.shiftKey?2:0)|(e.altKey?4:0);
  canvas.oncontextmenu=e=>e.preventDefault();
  canvas.onpointerdown=e=>{e.preventDefault();canvas.focus();canvas.setPointerCapture(e.pointerId);send(e.detail===2?'double':'down',...pos(e),e.button,mods(e));};
  canvas.onpointermove=e=>{send('move',...pos(e),0,mods(e));if(e.buttons)send('delta',e.movementX*scale,e.movementY*scale,0,mods(e));};
  canvas.onpointerup=e=>{send('up',0,0,e.button,mods(e));if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId);};
  canvas.onpointercancel=()=>send('blur');canvas.onlostpointercapture=()=>send('up');canvas.onblur=()=>send('blur');
  canvas.addEventListener('wheel',e=>{e.preventDefault();send('wheel',...pos(e),-e.deltaY/100,mods(e));},{passive:false});
  let drag;
  bar.onpointerdown=e=>{if(e.target.closest('button'))return;drag=[e.clientX-panel.offsetLeft,e.clientY-panel.offsetTop];bar.setPointerCapture(e.pointerId);};
  bar.onpointermove=e=>{if(drag){panel.style.left=`${e.clientX-drag[0]}px`;panel.style.top=`${e.clientY-drag[1]}px`;}};
  bar.onpointerup=()=>drag=null;
  const tick=()=>{if(!windows.has(id))return;send('frame');if(windows.has(id))state.frame=requestAnimationFrame(tick);};
  state.frame=requestAnimationFrame(tick);canvas.focus();
}
export function draw(id,pixels,w,h) {
  const s=windows.get(id);if(!s)return;
  if(s.canvas.width!==w||s.canvas.height!==h){s.canvas.width=w;s.canvas.height=h;s.canvas.style.width=`${w/s.scale}px`;s.canvas.style.height=`${h/s.scale}px`;}
  const image=new ImageData(w,h), out=new Uint32Array(image.data.buffer);
  for(let i=0;i<pixels.length;i++){const p=pixels[i];out[i]=0xff000000|((p&255)<<16)|(p&65280)|(p>>>16);}
  s.canvas.getContext('2d').putImageData(image,0,0);
}
export function close(id){const s=windows.get(id);if(s){cancelAnimationFrame(s.frame);s.overlay?.remove();const focused=s.panel.contains(document.activeElement);s.panel.remove();windows.delete(id);if(focused)document.querySelector('#lemon-canvas')?.focus();}}
function overlay(id){const s=windows.get(id);if(!s)return; s.overlay?.remove();const d=document.createElement('dialog');d.style.cssText='color:#eee;background:#303638;border:1px solid #727b80;padding:8px;max-height:80vh;overflow:auto';document.body.append(d);s.overlay=d;const reply=v=>{s.callback(id,v===null?'cancel':`reply:${v}`,0,0,0,0);d.close();d.remove();s.overlay=null;s.canvas.focus();};d.oncancel=e=>{e.preventDefault();reply(null);};d.onclick=e=>{if(e.target===d)reply(null);};return {s,d,reply};}
export function menu(id,json){const o=overlay(id);if(!o)return;const append=(rows,root)=>{for(const row of rows){if(row.children){const group=document.createElement('details'),label=document.createElement('summary');label.textContent=row.label;group.append(label);append(row.children,group);root.append(group);}else{const b=document.createElement('button');b.textContent=row.label||'—';b.disabled=!row.enabled;b.style.cssText='display:block;width:100%;text-align:left;color:inherit;background:transparent;border:0;padding:6px 14px;cursor:pointer';b.onclick=e=>{e.stopPropagation();o.reply(String(row.id));};root.append(b);}}};append(JSON.parse(json),o.d);o.d.showModal();}
export function type_in(id,text){const o=overlay(id);if(!o)return;const form=document.createElement('form'),input=document.createElement('input'),ok=document.createElement('button');input.value=text;input.ariaLabel='Parameter value';ok.textContent='Apply';form.append(input,ok);o.d.append(form);form.onsubmit=e=>{e.preventDefault();o.reply(input.value);};o.d.showModal();input.focus();input.select();}

export function pages(id,count){const s=windows.get(id);if(!s)return;for(let i=0;i<count;i++){const b=document.createElement("button");b.textContent=i===0?"Instrument / effect":"Envelope";b.onclick=()=>s.callback(id,"page",i,0,0,0);b.style.cssText="color:#ddd;background:#454d50;border:1px solid #202729;border-radius:3px;padding:2px 8px";s.bar.insertBefore(b,s.bar.lastElementChild);}}

export function download_sample(bytes){
  const url=URL.createObjectURL(new Blob([Uint8Array.from(bytes)],{type:'audio/wav'})),link=document.createElement('a');
  link.href=url;link.download='sample.wav';link.hidden=true;
  try{document.body.append(link);link.click();}finally{link.remove();setTimeout(()=>URL.revokeObjectURL(url),30000);}
}
