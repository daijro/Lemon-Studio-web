const contexts=new Set();
let notice;
function report(message) {
  if(!notice){notice=document.createElement('button');notice.id='audio-status';notice.style.cssText='position:fixed;z-index:90;right:16px;bottom:16px;max-width:420px;padding:12px;color:#eee;background:#363b3e;border:1px solid #ffb340';notice.onclick=resume;document.body.append(notice);}
  notice.textContent=message;notice.hidden=false;
}
function update(){
  for(const c of contexts)if(c.state==='closed')contexts.delete(c);
  if([...contexts].some(c=>c.state==='suspended'||c.state==='interrupted'))report('Audio is paused by the browser. Click to enable sound.');
  else if(notice)notice.hidden=true;
}
function resume(){
  for(const c of contexts)if(c.state!=='running'&&c.state!=='closed'){
    try{c.resume().then(update,error=>report(`Audio could not start: ${error.message??error}. Click to retry.`));}
    catch(error){report(`Audio could not start: ${error.message??error}. Click to retry.`);}
  }
}
if(typeof document!=='undefined'){
document.addEventListener('pointerdown',resume,true);
document.addEventListener('keydown',resume,true);
window.addEventListener('pagehide',()=>{for(const c of contexts)c.close();contexts.clear();});
}
export function watch_context(context){contexts.add(context);context.addEventListener('statechange',update);setTimeout(update,500);}
