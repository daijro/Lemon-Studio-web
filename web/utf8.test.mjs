import {test} from 'node:test';
import assert from 'node:assert/strict';
import {WorkletTextDecoder, WorkletTextEncoder} from './utf8.mjs';
test('worklet UTF-8 matches native codecs for Rust strings and JS lone surrogates', () => {
  const decoder = new WorkletTextDecoder('utf-8', {fatal: true, ignoreBOM: true});
  for (const input of ['', 'Lemon', 'é Ω 日本 🎹 \0', '\uFEFFsong', '\ud800end', 'end\udfff']) {
    const bytes = new WorkletTextEncoder().encode(input);
    assert.deepEqual(bytes, new TextEncoder().encode(input));
    assert.equal(decoder.decode(bytes), new TextDecoder('utf-8', {fatal:true, ignoreBOM:true}).decode(bytes));
    assert.equal(decoder.decode(new DataView(bytes.buffer)), decoder.decode(bytes));
  }
  assert.throws(() => decoder.decode(new Uint8Array([0xc0, 0xaf])));
  assert.throws(() => decoder.decode(new Uint8Array([0xf0, 0x90])));
  assert.equal(new WorkletTextDecoder('utf-8', {fatal: true}).decode(new Uint8Array([239,187,191,65])), 'A');
});
