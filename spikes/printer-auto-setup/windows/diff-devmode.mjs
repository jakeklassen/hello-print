#!/usr/bin/env node
// Spike tool: compare two DEVMODE blobs. Prints the public DEVMODEW fields
// side by side, then every byte that differs in the driver-private area
// (grouped into runs) so a single-setting change can be located.
//
// Usage: node diff-devmode.mjs <a.bin> <b.bin>

import { readFileSync } from "node:fs";
import { basename } from "node:path";

const [aPath, bPath] = process.argv.slice(2);

if (!aPath || !bPath) {
  console.error("Usage: node diff-devmode.mjs <a.bin> <b.bin>");
  process.exit(1);
}

const a = readFileSync(aPath);
const b = readFileSync(bPath);

const wstr = (buf, off) => buf.toString("utf16le", off, off + 64).replace(/\0.*$/s, "");

// DEVMODEW public fields (offset, reader); dmSize is 220 for this layout.
const fields = [
  ["dmDeviceName", (x) => wstr(x, 0)],
  ["dmDriverVersion", (x) => `0x${x.readUInt16LE(66).toString(16)}`],
  ["dmDriverExtra", (x) => x.readUInt16LE(70)],
  ["dmFields", (x) => `0x${x.readUInt32LE(72).toString(16)}`],
  ["dmOrientation", (x) => x.readInt16LE(76)],
  ["dmPaperSize", (x) => x.readInt16LE(78)],
  ["dmPaperLength (0.1mm)", (x) => x.readInt16LE(80)],
  ["dmPaperWidth (0.1mm)", (x) => x.readInt16LE(82)],
  ["dmScale", (x) => x.readInt16LE(84)],
  ["dmCopies", (x) => x.readInt16LE(86)],
  ["dmDefaultSource", (x) => x.readInt16LE(88)],
  ["dmPrintQuality", (x) => x.readInt16LE(90)],
  ["dmColor", (x) => x.readInt16LE(92)],
  ["dmYResolution", (x) => x.readInt16LE(96)],
  ["dmFormName", (x) => wstr(x, 102)],
  ["dmMediaType", (x) => x.readUInt32LE(196)],
];

console.log(`A = ${basename(aPath)} (${a.length} bytes)\nB = ${basename(bPath)} (${b.length} bytes)\n`);
for (const [name, read] of fields) {
  const va = read(a);
  const vb = read(b);
  console.log(`${va === vb ? " " : "*"} ${name.padEnd(22)} ${String(va).padEnd(28)} ${vb}`);
}

const publicSize = a.readUInt16LE(68);
const end = Math.min(a.length, b.length);
const runs = [];
for (let i = publicSize; i < end; i++) {
  if (a[i] === b[i]) continue;
  const last = runs.at(-1);
  if (last && i - last.end <= 4) last.end = i + 1;
  else runs.push({ start: i, end: i + 1 });
}

console.log(`\nPrivate area: ${runs.length} differing run(s)`);
const hex = (buf, s, e) => [...buf.subarray(s, e)].map((x) => x.toString(16).padStart(2, "0")).join(" ");
for (const r of runs) {
  const rel = r.start - publicSize;
  console.log(`  @${r.start} (private +${rel}, ${r.end - r.start}B)\n    A: ${hex(a, r.start, r.end)}\n    B: ${hex(b, r.start, r.end)}`);
}
