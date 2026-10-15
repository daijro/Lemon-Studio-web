import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, sep } from 'node:path';
import { chromium } from 'playwright';

const mode = process.argv[2] ?? 'dsp';
assert.ok(['dsp', 'plugins'].includes(mode));
const root = resolve(process.env.DSP_BUILD_DIR ?? 'target/web-dsp');
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
    if (url.pathname === '/fl-studio/') { res.end('<!doctype html><title>Lemon DSP regression check</title>'); return; }
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
  browser = await chromium.launch({executablePath: process.env.BROWSER_EXECUTABLE || undefined, headless: true, args: ['--autoplay-policy=no-user-gesture-required']});
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(String(e)));
  page.on('console', m => { if (m.type() === 'error') console.error('Browser:', m.text()); });
  await page.goto(`http://127.0.0.1:${server.address().port}/fl-studio/`);
  if (mode === 'plugins') {
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
