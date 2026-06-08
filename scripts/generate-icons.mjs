// Generates app icons (PNG set + Windows .ico) with no external dependencies.
// Run: node scripts/generate-icons.mjs
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const OUT = join(dirname(fileURLToPath(import.meta.url)), "..", "app", "icons");
mkdirSync(OUT, { recursive: true });

// ---- CRC32 ----------------------------------------------------------------
const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

// ---- pixel drawing --------------------------------------------------------
function draw(size) {
  const px = Buffer.alloc(size * size * 4);
  const set = (x, y, r, g, b, a = 255) => {
    if (x < 0 || y < 0 || x >= size || y >= size) return;
    const o = (y * size + x) * 4;
    // simple alpha-over onto existing pixel
    const ia = a / 255;
    px[o] = Math.round(r * ia + px[o] * (1 - ia));
    px[o + 1] = Math.round(g * ia + px[o + 1] * (1 - ia));
    px[o + 2] = Math.round(b * ia + px[o + 2] * (1 - ia));
    px[o + 3] = Math.max(px[o + 3], a);
  };
  const rrect = (x0, y0, w, h, rad, r, g, b, a = 255) => {
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        // rounded-corner mask
        const dx = Math.min(x, w - 1 - x);
        const dy = Math.min(y, h - 1 - y);
        if (dx < rad && dy < rad) {
          const cx = dx - rad,
            cy = dy - rad;
          if (cx * cx + cy * cy > rad * rad) continue;
        }
        set(x0 + x, y0 + y, r, g, b, a);
      }
    }
  };

  const s = size;
  // background rounded square (dark)
  rrect(0, 0, s, s, Math.round(s * 0.22), 0x1e, 0x1e, 0x24);
  // clipboard body (light)
  const bw = Math.round(s * 0.5),
    bh = Math.round(s * 0.6);
  const bx = Math.round((s - bw) / 2),
    by = Math.round(s * 0.26);
  rrect(bx, by, bw, bh, Math.round(s * 0.06), 0xe6, 0xe6, 0xec);
  // clip at the top (accent)
  const cw = Math.round(s * 0.22),
    ch = Math.round(s * 0.1);
  rrect(Math.round((s - cw) / 2), Math.round(by - ch * 0.55), cw, ch, Math.round(s * 0.03), 0x4a, 0x9d, 0xd8);
  // text lines on the clipboard (dark)
  const lh = Math.round(s * 0.045);
  for (let i = 0; i < 3; i++) {
    rrect(
      bx + Math.round(bw * 0.16),
      by + Math.round(bh * (0.28 + i * 0.2)),
      Math.round(bw * (i === 2 ? 0.45 : 0.68)),
      lh,
      Math.round(lh / 2),
      0x55,
      0x55,
      0x60
    );
  }
  return px;
}

// ---- PNG encode -----------------------------------------------------------
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td), 0);
  return Buffer.concat([len, td, crc]);
}
function encodePng(size) {
  const raw = draw(size);
  const stride = size * 4;
  const filtered = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y++) {
    filtered[y * (stride + 1)] = 0; // filter: none
    raw.copy(filtered, y * (stride + 1) + 1, y * stride, y * stride + stride);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(filtered, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// ---- ICO encode (PNG-encoded entries) ------------------------------------
function encodeIco(sizes) {
  const pngs = sizes.map((s) => encodePng(s));
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(sizes.length, 4);
  const dir = Buffer.alloc(16 * sizes.length);
  let offset = 6 + dir.length;
  sizes.forEach((s, i) => {
    const o = i * 16;
    dir[o] = s >= 256 ? 0 : s;
    dir[o + 1] = s >= 256 ? 0 : s;
    dir[o + 2] = 0;
    dir[o + 3] = 0;
    dir.writeUInt16LE(1, o + 4); // color planes
    dir.writeUInt16LE(32, o + 6); // bpp
    dir.writeUInt32LE(pngs[i].length, o + 8);
    dir.writeUInt32LE(offset, o + 12);
    offset += pngs[i].length;
  });
  return Buffer.concat([header, dir, ...pngs]);
}

// ---- write outputs --------------------------------------------------------
const outputs = {
  "32x32.png": encodePng(32),
  "128x128.png": encodePng(128),
  "128x128@2x.png": encodePng(256),
  "icon.png": encodePng(512),
  "icon.ico": encodeIco([16, 32, 48, 64, 256]),
};
for (const [name, buf] of Object.entries(outputs)) {
  writeFileSync(join(OUT, name), buf);
  console.log(`wrote app/icons/${name} (${buf.length} bytes)`);
}
