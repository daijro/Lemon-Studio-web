import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, sep } from 'node:path';
import { chromium } from 'playwright';

const mode = process.argv[2] ?? 'dsp';
assert.ok(['dsp', 'plugins', 'editor','audio'].includes(mode));
const root = resolve(['editor','audio'].includes(mode) ? (process.env.WEB_BUILD_DIR ?? 'dist') : (process.env.DSP_BUILD_DIR ?? 'target/web-dsp'));
const worklet = `import { initSync, stretch_probe } from './stretch_probe.js';
registerProcessor('stretch-parity', class extends AudioWorkletProcessor {
  constructor(options) {
    super();
    try {
      initSync({module: options.processorOptions});
      const samples = stretch_probe();
      this.port.postMessage({samples}, [samples.buffer]);
    } catch (error) { this.port.postMessage({error: String(error)}); }
  }
  process(_inputs, outputs) { for (const c of outputs[0]) c.fill(0); return true; }
});`;
const server = createServer(async (req, res) => {
  res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
  try {
    const url = new URL(req.url, 'http://localhost');
    if (url.pathname === '/favicon.ico') { res.writeHead(204).end(); return; }
    if (url.pathname === '/fl-studio/') { res.setHeader('Content-Type', 'text/html'); res.end(['editor','audio'].includes(mode) ? await readFile(resolve(root, 'index.html')) : '<!doctype html><title>Lemon DSP regression check</title>'); return; }
    if (url.pathname === '/fl-studio/probe-worklet.mjs') { res.setHeader('Content-Type', 'text/javascript'); res.end(worklet); return; }
    const path = resolve(root, decodeURIComponent(url.pathname).replace(/^\/fl-studio\//, ''));
    if (!path.startsWith(root + sep)) { res.writeHead(404).end(); return; }
    res.setHeader('Content-Type', path.endsWith('.wasm') ? 'application/wasm' : (path.endsWith('.js') || path.endsWith('.mjs')) ? 'text/javascript' : 'application/octet-stream');
    res.end(await readFile(path));
  } catch { res.writeHead(404).end(); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
let browser;
try {
  browser = await chromium.launch({executablePath: process.env.BROWSER_EXECUTABLE || undefined, headless: true, args: ['dsp','plugins'].includes(mode)?['--autoplay-policy=no-user-gesture-required']:[]});
  const page = await browser.newPage();
  await page.addInitScript(() => { Error.stackTraceLimit = 100; });
  if(mode==='audio')await page.addInitScript(()=>{const Original=AudioContext;globalThis.__audioContexts=[];globalThis.AudioContext=class extends Original{constructor(...args){if(globalThis.__denyAudio)throw new Error('Injected audio device failure');super(...args);globalThis.__audioContexts.push(this);}};});
  const errors = [];
  page.on('pageerror', e => errors.push(String(e)));
  page.on('console', m => { if (m.type() === 'error') console.error('Browser:', m.text()); });
  await page.goto(`http://127.0.0.1:${server.address().port}/fl-studio/`);
  if (mode === 'editor') {
    await page.locator('html[data-lemon-ready=true]').waitFor({timeout:30000});
    await page.waitForTimeout(300);
    const snapshot = () => page.evaluate(async () => JSON.parse((await import('./os_app.js')).snapshot()));
    const click = async (action, options) => {
      const state = await snapshot(), hit = state.hits.findLast(h => h.action === action);
      assert.ok(hit, `Missing control ${action}`);
      const [x,y,w,h] = hit.rect; await page.mouse.click(x+w/2,y+h/2,options);
      await page.waitForTimeout(80);
    };
    let state = await snapshot();
    assert.ok(state.hits.length > 50, 'The real editor must expose its controls');
    await click('ChannelSelect(0)');
    await page.locator('.plugin-window canvas').waitFor({timeout:15000});
    await page.waitForFunction(() => { const c=document.querySelector('.plugin-window canvas');return c && c.getContext('2d').getImageData(0,0,c.width,c.height).data.some(v=>v>0); }, null, {timeout:15000});
    const canvas = page.locator('.plugin-window canvas');
    const bounds = await canvas.boundingBox();
    const param = async () => (await snapshot()).parameters[0].values.find(p => /start.*offset/i.test(p.name))?.value;
    const before = await param(); assert.equal(before, 0);
    await page.mouse.click(bounds.x+40, bounds.y+210, {button:'right'});
    await page.getByRole('button',{name:'Type in value...'}).click();
    await page.getByRole('textbox',{name:'Parameter value'}).fill('100');
    await page.getByRole('button',{name:'Apply',exact:true}).click();
    await page.waitForTimeout(200);
    assert.ok(await param() > before, 'Typed parameter must reach the plugin');
    await page.mouse.move(bounds.x+40,bounds.y+210); await page.mouse.down();
    await page.mouse.move(bounds.x+40,bounds.y+185,{steps:5}); await page.mouse.up();
    await page.waitForTimeout(200);
    const after = await param();
    await page.mouse.move(bounds.x+40,bounds.y+150,{steps:5}); await page.waitForTimeout(100);
    assert.equal(await param(),after,'Released pointer must stop changing the parameter');
    await page.screenshot({path:'/tmp/lemon-web-evidence/sampler-editor.png'});
    await page.getByRole('button',{name:'Close instrument editor'}).click();
    await page.locator('.plugin-window').waitFor({state:'detached'});
    await click('ToggleWin(PianoRoll)');
    state = await snapshot();
    const grid=state.hits.find(h=>h.action==='PianoGrid');assert.ok(grid);
    await page.mouse.click(grid.rect[0]+100,grid.rect[1]+100);
    await page.waitForFunction(async () => JSON.parse((await import('./os_app.js')).snapshot()).notes===1);
    await click('PianoNote(0)',{button:'right'});
    await page.waitForFunction(async () => JSON.parse((await import('./os_app.js')).snapshot()).notes===0);
    await page.keyboard.press('Control+z');
    await page.waitForFunction(async () => JSON.parse((await import('./os_app.js')).snapshot()).notes===1);
    await click('ToggleWin(Mixer)');
    await page.setViewportSize({width:1440,height:900});
    await page.waitForFunction(async () => JSON.parse((await import('./os_app.js')).snapshot()).screen[0]===1440);
    await page.screenshot({path: '/tmp/lemon-web-evidence/editor.png'});
    const hidpi = await browser.newContext({viewport:{width:1280,height:720},deviceScaleFactor:2});
    const high = await hidpi.newPage(); high.on('pageerror',e=>errors.push(String(e)));
    await high.goto(page.url()); await high.locator('html[data-lemon-ready=true]').waitFor();
    await high.waitForTimeout(300);
    const hs=await high.evaluate(async()=>JSON.parse((await import('./os_app.js')).snapshot()));
    assert.deepEqual(hs.screen,[1280,720], 'High DPI must keep the same logical workspace');
    const hh=hs.hits.find(h=>h.action==='ChannelSelect(0)'); const [hx,hy,hw,hhgt]=hh.rect;
    await high.mouse.click(hx+hw/2,hy+hhgt/2);
    await high.locator('.plugin-window canvas').waitFor(); await high.waitForTimeout(300);
    const dimensions=await high.locator('.plugin-window canvas').evaluate(c=>({pixels:c.width,css:c.getBoundingClientRect().width}));
    assert.equal(dimensions.pixels,dimensions.css*2,'Plugin canvas must match display density');
    await high.screenshot({path:'/tmp/lemon-web-evidence/editor-hidpi.png'}); await hidpi.close();
    assert.deepEqual(errors, []);
    console.log('PASS: real editor, sampler controls, note create/delete/undo, mixer and canvas resize');
  } else if (mode === 'audio') {
    await page.locator('html[data-lemon-ready=true]').waitFor(); await page.waitForTimeout(300);
    const snapshot=()=>page.evaluate(async()=>JSON.parse((await import('./os_app.js')).snapshot()));
    const click=async(action)=>{const s=await snapshot(), h=s.hits.findLast(h=>h.action===action || h.hint===action);assert.ok(h,`Missing ${action}: ${JSON.stringify(s.hits.filter(h=>h.action.includes('Popup')))}`);const[x,y,w,z]=h.rect;await page.mouse.click(x+w/2,y+z/2);await page.waitForTimeout(250);};
    await page.waitForFunction(async()=>{const s=(await import('./os_app.js')).snapshot();return s && !JSON.parse(s).loading;});
    await click('Menu(2)'); await click('MenuItem(2, 0)');

    await click('Lemon Trio');
    await page.locator('.plugin-window canvas').waitFor();
    await page.getByRole('button',{name:'Close instrument editor'}).click();
    await page.locator('.plugin-window').waitFor({state:'detached'});
    await click('ToggleWin(PianoRoll)');
    const grid=(await snapshot()).hits.find(h=>h.action==='PianoGrid');
    await page.mouse.click(grid.rect[0]+40,grid.rect[1]+100); await page.waitForTimeout(300);
    await click('Play'); let measuredPeak=0;
    for(let i=0;i<24;i++){await page.waitForTimeout(50);measuredPeak=Math.max(measuredPeak,(await snapshot()).audio.peak);}
    const playing=await snapshot(); console.log('playing',playing.audio,playing.message);
    assert.ok(playing.audio.rendered>0,'AudioWorklet must render');
    assert.ok(measuredPeak>0.001 && measuredPeak<=2,'Oscillator must produce bounded audio');
    await page.keyboard.press('F8'); await page.waitForTimeout(150); await click('Lemon Parametric EQ');
    await page.locator('.plugin-window canvas').waitFor(); await page.waitForTimeout(300);
    await page.getByRole('button',{name:'Close instrument editor'}).click();
    await page.locator('.plugin-window').waitFor({state:'detached'});
    await page.waitForTimeout(2000); console.log('steady',(await snapshot()).audio); assert.ok((await snapshot()).audio.underruns-playing.audio.underruns<5,'Steady playback must not drop blocks');
    await click('Play'); await page.waitForTimeout(300); const paused=await snapshot();
    assert.equal(paused.audio.state,'Paused'); await page.waitForTimeout(300);
    assert.equal((await snapshot()).audio.tick,paused.audio.tick);
    await click('Play'); await page.waitForTimeout(400); assert.equal((await snapshot()).audio.state,'Playing');
    await page.evaluate(async()=>{for(const c of __audioContexts)if(c.state==='running')await c.suspend();});
    await page.locator('#audio-status').waitFor({state:'visible'});
    await page.evaluate(()=>{for(const c of __audioContexts)if(c.state==='suspended'){c.originalResume=c.resume;c.resume=()=>Promise.reject(new Error('Injected permission denial'));}});
    await page.locator('#audio-status').click(); await page.waitForTimeout(100);
    assert.match(await page.locator('#audio-status').textContent(),/could not start/);
    await page.evaluate(()=>{for(const c of __audioContexts)if(c.originalResume)c.resume=c.originalResume;});
    await page.locator('#audio-status').click(); await page.locator('#audio-status').waitFor({state:'hidden'});
    await page.waitForTimeout(200);
    await click('Stop'); await page.waitForTimeout(400); const stopped=await snapshot();
    assert.equal(stopped.audio.state,'Stopped'); assert.equal(stopped.audio.peak,0);
    await page.evaluate(()=>globalThis.__denyAudio=true); await click('Play'); await page.waitForTimeout(800);
    assert.match((await snapshot()).message,/audio|context|output|device/i,'Audio startup failure must be visible');
    await page.evaluate(()=>globalThis.__denyAudio=false); await click('Stop');
    await page.evaluate(()=>window.dispatchEvent(new Event('pagehide')));
    assert.ok(await page.evaluate(()=>__audioContexts.every(c=>c.state==='closed')),'Teardown closes audio contexts');
    assert.deepEqual(errors,[]); console.log('PASS: AudioWorklet play/pause/resume/stop and bounded synth output');
  } else if (mode === 'plugins') {
    const result = await page.evaluate(async () => {
      const m = await import('./builtin_probe_web.js');
      await m.default();
      const result = JSON.parse(m.builtin_probe());
      m.start_worker_probe();
      const deadline = performance.now() + 15000;
      while (!m.worker_probe_done()) {
        if (performance.now() > deadline) throw Error('Worker did not complete');
        await new Promise(resolve => setTimeout(resolve, 25));
      }
      return result;
    });
    const native = JSON.parse(await readFile(resolve(root, 'plugins-native.json')));
    assert.deepEqual(result, native);
    assert.deepEqual(errors, []);
    console.log('PASS: built-in plugin lifecycle and native-compatible parameters/state');
  } else {
  const result = await page.evaluate(async () => {
    const {default: init, stretch_probe} = await import('./stretch_probe.js');
    const wasm = await init();
    const native = new Float32Array(await (await fetch('./native.f32')).arrayBuffer());
    function compare(actual) {
      if (actual.length !== native.length) throw Error('Output length differs from native');
      let sum = 0, peak = 0;
      for (let i=0; i<actual.length; i++) {
        if (!Number.isFinite(actual[i])) throw Error('Nonfinite audio');
        const error = Math.abs(actual[i] - native[i]); sum += error * error; peak = Math.max(peak, error);
      }
      return {frames: actual.length / 2, rmsError: Math.sqrt(sum / actual.length), peakError: peak};
    }
    const main = compare(stretch_probe());
    wasm.memory.grow(32);
    const grown = compare(stretch_probe());
    const module = await WebAssembly.compileStreaming(fetch('./stretch_probe_bg.wasm'));
    const context = new AudioContext();
    try {
      await context.audioWorklet.addModule('./probe-worklet.mjs');
      const node = new AudioWorkletNode(context, 'stretch-parity', {processorOptions: module, outputChannelCount: [2]});
      const data = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(Error('Worklet timed out')), 15000);
        node.port.onmessage = ({data}) => { clearTimeout(timer); data.error ? reject(Error(data.error)) : resolve(data.samples); };
        node.onprocessorerror = () => { clearTimeout(timer); reject(Error('Worklet processor failed')); };
        node.connect(context.destination);
        context.resume().catch(reject);
      });
      node.disconnect();
      return {main, grown, worklet: compare(data)};
    } finally { await context.close(); }
  });
  console.log(JSON.stringify(result, null, 2));
  assert.deepEqual(errors, []);
  for (const metrics of Object.values(result)) {
    assert.ok(metrics.rmsError < 0.001, 'Native/browser RMS difference exceeds 0.001');
    assert.ok(metrics.peakError < 0.02, 'Native/browser peak difference exceeds 0.02');
  }
  const failedPage = await browser.newPage();
  await failedPage.route('**/stretch_probe_bg.wasm', route => route.fulfill({status: 404, body: 'missing'}));
  await failedPage.goto(`http://127.0.0.1:${server.address().port}/fl-studio/`);
  const loadError = await failedPage.evaluate(async () => {
    try { await (await import('./stretch_probe.js')).default(); return ''; }
    catch (error) { return String(error); }
  });
  assert.match(loadError, /failed to fetch Wasm: 404/);
  console.log('PASS: native/browser DSP parity, reset, instance lifecycle, memory growth, missing-module error and AudioWorklet execution');
  }
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
