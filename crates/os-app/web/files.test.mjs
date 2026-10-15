import assert from 'node:assert/strict';
import { persist } from './files.mjs';
const limit=256*1024*1024;
await assert.rejects(persist({project:{byteLength:1},files:[{bytes:{byteLength:limit}},{bytes:{byteLength:1}}]}), /256 MiB/);
await assert.rejects(persist({project:{byteLength:limit+1},files:[]}), /256 MiB/);
console.log('PASS: oversized snapshots are rejected before opening storage');
