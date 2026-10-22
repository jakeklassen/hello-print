#!/usr/bin/env node
// Spike probe (read-only): IPP Get-Printer-Attributes against a printer.
//
// Sends a single Get-Printer-Attributes request (no jobs, no settings changes),
// writes every attribute to a JSON file and prints the ones relevant to 1:1
// borderless printing.
//
// Usage: node get-attributes.mjs <printer-host> <out.json> [ipp-path]
//   e.g. node get-attributes.mjs 192.168.2.243 ../../../tmp/spike/ipp/attrs.json

import { writeFileSync, mkdirSync } from "node:fs";
import http from "node:http";
import https from "node:https";
import { dirname } from "node:path";

const [host, outPath, ippPath = "/ipp/print"] = process.argv.slice(2);

if (!host || !outPath) {
  console.error("Usage: node get-attributes.mjs <printer-host> <out.json> [ipp-path]");
  process.exit(1);
}

const printerUri = `ipp://${host}:631${ippPath}`;

function attr(tag, name, value) {
  const n = Buffer.from(name, "utf8");
  const v = Buffer.isBuffer(value) ? value : Buffer.from(value, "utf8");
  const head = Buffer.alloc(1 + 2);
  head.writeUInt8(tag, 0);
  head.writeUInt16BE(n.length, 1);
  const len = Buffer.alloc(2);
  len.writeUInt16BE(v.length, 0);
  return Buffer.concat([head, n, len, v]);
}

const request = Buffer.concat([
  Buffer.from([0x02, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x01]), // IPP/2.0 Get-Printer-Attributes, id 1
  Buffer.from([0x01]), // operation-attributes-tag
  attr(0x47, "attributes-charset", "utf-8"),
  attr(0x48, "attributes-natural-language", "en"),
  attr(0x45, "printer-uri", printerUri),
  attr(0x44, "requested-attributes", "all"),
  attr(0x44, "", "media-col-database"),
  Buffer.from([0x03]), // end-of-attributes-tag
]);

function decodeValue(tag, buf) {
  switch (tag) {
    case 0x21: // integer
    case 0x23: // enum
      return buf.readInt32BE(0);
    case 0x22:
      return buf.readUInt8(0) === 1;
    case 0x32:
      return `${buf.readInt32BE(0)}x${buf.readInt32BE(4)}${buf.readUInt8(8) === 3 ? "dpi" : "dpcm"}`;
    case 0x33:
      return { min: buf.readInt32BE(0), max: buf.readInt32BE(4) };
    case 0x10:
      return "<unsupported>";
    case 0x12:
      return "<unknown>";
    case 0x13:
      return "<no-value>";
    case 0x30:
    case 0x31:
      return buf.toString("hex");
    default:
      return buf.toString("utf8");
  }
}

// Parses attributes starting at `pos`; stops at a delimiter tag (top level) or
// endCollection (inside a collection). Returns [attributes, newPos].
function parseAttributes(buf, pos, inCollection) {
  const out = {};
  let current = null;
  let memberName = null;

  while (pos < buf.length) {
    const tag = buf.readUInt8(pos);

    if (!inCollection && tag < 0x10) {
      return [out, pos];
    }

    pos += 1;
    const nameLen = buf.readUInt16BE(pos);
    pos += 2;
    const name = buf.toString("utf8", pos, pos + nameLen);
    pos += nameLen;
    const valueLen = buf.readUInt16BE(pos);
    pos += 2;
    const raw = buf.subarray(pos, pos + valueLen);
    pos += valueLen;

    if (tag === 0x37) {
      // endCollection
      return [out, pos];
    }

    if (tag === 0x4a) {
      // memberAttrName: the next value belongs to this member
      memberName = raw.toString("utf8");
      continue;
    }

    let value;
    if (tag === 0x34) {
      [value, pos] = parseAttributes(buf, pos, true);
    } else {
      value = decodeValue(tag, raw);
    }

    const key = inCollection ? memberName ?? current : name || current;
    if (inCollection && memberName) {
      current = memberName;
      memberName = null;
    } else if (name) {
      current = name;
    }

    (out[key] ??= []).push(value);
  }

  return [out, pos];
}

function parseResponse(buf) {
  const status = buf.readUInt16BE(2);
  let pos = 8;
  const groups = {};
  while (pos < buf.length) {
    const tag = buf.readUInt8(pos);
    pos += 1;
    if (tag === 0x03) break;
    const [attrs, next] = parseAttributes(buf, pos, false);
    Object.assign((groups[tag] ??= {}), attrs);
    pos = next;
  }
  return { status: `0x${status.toString(16).padStart(4, "0")}`, printer: groups[0x04] ?? {} };
}

// Printers answer plain HTTP with 426 Upgrade Required when they insist on TLS
// (IPPS). Printers ship self-signed certificates, so certificate verification
// is skipped for this one local device.
function post(useTls) {
  const mod = useTls ? https : http;
  return new Promise((resolve, reject) => {
    const req = mod.request(
      {
        host,
        port: 631,
        path: ippPath,
        method: "POST",
        headers: { "Content-Type": "application/ipp", "Content-Length": request.length },
        rejectUnauthorized: false,
      },
      (res) => {
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () => resolve({ status: res.statusCode, body: Buffer.concat(chunks) }));
      },
    );
    req.on("error", reject);
    req.end(request);
  });
}

let res = await post(false);
if (res.status === 426) res = await post(true);

if (res.status !== 200) {
  console.error(`HTTP ${res.status}`);
  process.exit(1);
}

const parsed = parseResponse(res.body);
mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, JSON.stringify(parsed, null, 2));

const p = parsed.printer;
const pick = (k) => (p[k] ? JSON.stringify(p[k]) : "<absent>");
const keys = [
  "printer-make-and-model",
  "printer-firmware-string-version",
  "ipp-versions-supported",
  "document-format-supported",
  "media-source-supported",
  "media-type-supported",
  "print-scaling-supported",
  "print-scaling-default",
  "print-quality-supported",
  "printer-resolution-supported",
  "media-size-supported",
  "media-left-margin-supported",
  "media-right-margin-supported",
  "media-top-margin-supported",
  "media-bottom-margin-supported",
  "media-overprint-supported",
  "media-overprint-distance-supported",
];

console.log(`status ${parsed.status}, ${Object.keys(p).length} attributes -> ${outPath}`);
for (const k of keys) console.log(`${k}: ${pick(k)}`);

// media-col-database entries for 4x6 (10160 x 15240) and Letter (21590 x 27940)
const db = p["media-col-database"] ?? [];
const summarize = (c) => {
  const s = c["media-size"]?.[0] ?? {};
  const dim = (d) => (typeof d?.[0] === "object" ? `${d[0].min}-${d[0].max}` : d?.[0]);
  return [
    `size ${dim(s["x-dimension"])}x${dim(s["y-dimension"])}`,
    `margins L${c["media-left-margin"]} R${c["media-right-margin"]} T${c["media-top-margin"]} B${c["media-bottom-margin"]}`,
    c["media-source"] ? `source ${c["media-source"]}` : "",
    c["media-type"] ? `type ${c["media-type"]}` : "",
  ]
    .filter(Boolean)
    .join(", ");
};
const interesting = db
  .map(summarize)
  .filter((s) => /10160x15240|21590x27940|-/.test(s));
console.log(`media-col-database: ${db.length} entries; 4x6 / Letter / custom-range entries:`);
for (const s of [...new Set(interesting)]) console.log(`  ${s}`);
