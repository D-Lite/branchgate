import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const brandDir = join(root, "public", "brand");
const iconsDir = join(root, "src-tauri", "icons");

function renderPng(svgPath, size, outPath) {
  const svg = readFileSync(svgPath, "utf8");
  const resvg = new Resvg(svg, {
    fitTo: { mode: "width", value: size },
    background: "transparent",
  });
  const png = resvg.render().asPng();
  writeFileSync(outPath, png);
  console.log(`wrote ${outPath} (${size}px)`);
}

const appIconSvg = join(brandDir, "branchgate-app-icon.svg");
const markSvg = join(brandDir, "branchgate-logo.svg");

renderPng(appIconSvg, 1024, join(iconsDir, "icon.png"));
renderPng(appIconSvg, 512, join(brandDir, "branchgate-app-icon-512.png"));
renderPng(appIconSvg, 256, join(brandDir, "branchgate-app-icon-256.png"));
renderPng(markSvg, 128, join(brandDir, "branchgate-logo-128.png"));
renderPng(markSvg, 32, join(brandDir, "branchgate-logo-32.png"));
