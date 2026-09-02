// Rasterizes the hilvan brand SVGs in ../../assets into the PNGs the canon
// asks for, and places the copies the docs site and the SPA serve. Run from
// this package so it resolves the local sharp install:
//   pnpm assets
import sharp from "sharp";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ASSETS = path.resolve(HERE, "../../assets");
const PUBLIC = path.join(HERE, "public");
const SITE_ASSETS = path.join(HERE, "src/assets");
const WEB_PUBLIC = path.resolve(HERE, "../../web/public");

// The S tile (filled hex, bold code) is what reads at icon sizes; the L tile
// carries the full mark wherever it is 64px or larger.
const S = path.join(ASSETS, "logo-s.svg");
const L = path.join(ASSETS, "logo.svg");
const BANNER = path.join(ASSETS, "banner.svg");

async function png(src, size, out) {
  await sharp(src, { density: 384 }).resize(size, size).png().toFile(out);
}

fs.mkdirSync(PUBLIC, { recursive: true });
fs.mkdirSync(SITE_ASSETS, { recursive: true });
fs.mkdirSync(WEB_PUBLIC, { recursive: true });

// Favicon and touch icons. The favicon is the SVG itself; the PNGs cover the
// platforms that will not take one. No .ico: nothing in this product is a
// Windows executable.
await png(S, 32, path.join(ASSETS, "favicon-32.png"));
await png(L, 180, path.join(ASSETS, "apple-touch-icon.png"));
await png(L, 512, path.join(ASSETS, "logo-512.png"));
for (const dir of [PUBLIC, WEB_PUBLIC]) {
  fs.copyFileSync(path.join(ASSETS, "apple-touch-icon.png"), path.join(dir, "apple-touch-icon.png"));
  fs.copyFileSync(S, path.join(dir, "favicon.svg"));
}
fs.copyFileSync(L, path.join(SITE_ASSETS, "logo.svg"));
console.log("wrote pngs");

// GitHub social preview: 1280x640. Two adjustments to the banner: its plate
// spans the full 720px while the artwork only fills the left ~560px (trim the
// tail, or it lands off-centre), and the rounded plate over an identical
// background leaves a visible seam (drop it and keep the inner rows only).
const bannerWidth = 1600;
const bannerHeight = Math.round((bannerWidth * 170) / 720);
const inset = Math.round((bannerWidth * 6) / 720); // clears the plate's rounded edge
const artworkWidth = Math.round((bannerWidth * 560) / 720);
const banner = await sharp(BANNER, { density: 384 })
  .resize({ width: bannerWidth })
  .extract({ left: inset, top: inset, width: artworkWidth - inset, height: bannerHeight - 2 * inset })
  .png()
  .toBuffer();

await sharp({
  create: { width: 1280, height: 640, channels: 4, background: "#1B2126" },
})
  .composite([{ input: await sharp(banner).resize({ width: 880 }).png().toBuffer(), gravity: "centre" }])
  .png()
  .toFile(path.join(ASSETS, "social-preview.png"));
console.log("wrote social-preview.png");
