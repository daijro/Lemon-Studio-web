// AudioWorklet has no Encoding API. These fallbacks cover wasm-bindgen's UTF-8 calls.
export class WorkletTextDecoder {
  constructor(label = 'utf-8', options = {}) {
    if (!['utf-8', 'utf8'].includes(label.toLowerCase()) || !options.fatal) {
      throw new TypeError('The WASM worklet decoder requires fatal UTF-8 decoding');
    }
    this.ignoreBOM = !!options.ignoreBOM;
  }
  decode(input = new Uint8Array(), options = {}) {
    if (options.stream) throw new TypeError('Streaming worklet text decoding is unsupported');
    const bytes = ArrayBuffer.isView(input) ? new Uint8Array(input.buffer, input.byteOffset, input.byteLength) : new Uint8Array(input);
    let escaped = '';
    for (const byte of bytes) escaped += '%' + byte.toString(16).padStart(2, '0');
    const text = decodeURIComponent(escaped);
    return this.ignoreBOM ? text : text.replace(/^\uFEFF/, '');
  }
}
export class WorkletTextEncoder {
  encode(input = '') {
    const bytes = [];
    for (const char of String(input)) {
      let c = char.codePointAt(0);
      if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd;
      if (c < 0x80) bytes.push(c);
      else if (c < 0x800) bytes.push(0xc0 | c >> 6, 0x80 | c & 63);
      else if (c < 0x10000) bytes.push(0xe0 | c >> 12, 0x80 | c >> 6 & 63, 0x80 | c & 63);
      else bytes.push(0xf0 | c >> 18, 0x80 | c >> 12 & 63, 0x80 | c >> 6 & 63, 0x80 | c & 63);
    }
    return new Uint8Array(bytes);
  }
}
export const TextDecoder = globalThis.TextDecoder ?? WorkletTextDecoder;
export const TextEncoder = globalThis.TextEncoder ?? WorkletTextEncoder;
